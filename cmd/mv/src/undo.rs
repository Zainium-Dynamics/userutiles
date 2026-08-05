// undo.rs — Transaction journal for mv --undo.
//
// Every operation that mutates the filesystem is recorded in a TOML journal
// before the mutation occurs. If the operation succeeds, the entry is marked
// committed. If the process dies between the two writes, the entry is treated
// as uncommitted and skipped by --undo.
//
// Journal location (in priority order):
// 1. --journal <path> (user-specified)
// 2. $XDG_STATE_HOME/mv/journal.toml
// 3. $HOME/.local/state/mv/journal.toml
//
// Journal format (TOML only — user_utils never uses JSON):
// [[entries]]
// op = "move"
// src = "/a"
// dest = "/b"
// ts = 1700000000
// committed = true
//
// --undo reverses the most recent committed entry:
// move → rename dest back to src
// exchange → rename_exchange again (its own inverse)

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, XmvError},
    ops::crossdev,
    progress::ProgressEvent,
};

// ─── Data model ───────────────────────────────────────────────────────────────

/// A single reversible operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    Move { src: PathBuf, dest: PathBuf },
    Exchange { path_a: PathBuf, path_b: PathBuf },
}

/// One journal entry as stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    #[serde(flatten)]
    op: Operation,
    ts: u64,
    committed: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct JournalFile {
    #[serde(default)]
    entries: Vec<Entry>,
}

// ─── Journal handle ───────────────────────────────────────────────────────────

pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// Open (or create) a journal at the given path.
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| XmvError::Journal(e.to_string()))?;
        }
        Ok(Self { path })
    }

    /// Resolve the default journal path from XDG / HOME conventions.
    pub fn default_path() -> PathBuf {
        let base = std::env::var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
                PathBuf::from(home).join(".local").join("state")
            });
        base.join("mv").join("journal.toml")
    }

    /// Record an operation as *intent* (not yet committed).
    /// Returns the entry index so `commit()` can mark it.
    pub fn record(&mut self, op: Operation) -> Result<u64> {
        let mut file = self.read_all()?;
        let idx = file.entries.len() as u64;
        file.entries.push(Entry {
            op,
            ts: unix_ts(),
            committed: false,
        });
        self.write_all(&file)?;
        Ok(idx)
    }

    /// Mark the entry at `index` as committed.
    pub fn commit(&mut self, index: u64) -> Result<()> {
        let mut file = self.read_all()?;
        if let Some(entry) = file.entries.get_mut(index as usize) {
            entry.committed = true;
        }
        self.write_all(&file)
    }

    /// Mark the most recent uncommitted entry as committed.
    pub fn commit_last(&mut self) -> Result<()> {
        let mut file = self.read_all()?;
        if let Some(entry) = file.entries.iter_mut().rev().find(|e| !e.committed) {
            entry.committed = true;
        }
        self.write_all(&file)
    }

    /// Undo the most recent committed operation.
    pub fn undo_last(&mut self) -> Result<()> {
        let mut file = self.read_all()?;

        let last = file
            .entries
            .iter()
            .enumerate()
            .rev()
            .find(|(_, e)| e.committed);

        let (idx, entry) = match last {
            Some((i, e)) => (i, e.clone()),
            None => return Err(XmvError::Journal("No committed operations to undo.".into())),
        };

        match &entry.op {
            Operation::Move { src, dest } => undo_move(src, dest)?,
            Operation::Exchange { path_a, path_b } => {
                crate::ops::rename::rename_exchange(path_a, path_b)?;
            }
        }

        file.entries.remove(idx);
        self.write_all(&file)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn read_all(&self) -> Result<JournalFile> {
        if !self.path.exists() {
            return Ok(JournalFile::default());
        }
        let text = fs::read_to_string(&self.path).map_err(|e| XmvError::Journal(e.to_string()))?;
        if text.trim().is_empty() {
            return Ok(JournalFile::default());
        }
        toml::from_str(&text).map_err(|e| XmvError::Journal(format!("Corrupt journal: {e}")))
    }

    fn write_all(&self, file: &JournalFile) -> Result<()> {
        let text = toml::to_string_pretty(file).map_err(|e| XmvError::Journal(e.to_string()))?;
        fs::write(&self.path, text).map_err(|e| XmvError::Journal(e.to_string()))
    }
}

/// Reverse a recorded `Move { src, dest }` by moving `dest` back to `src`.
///
/// Tries a plain `rename(2)` first — the common case for same-device moves.
/// If `dest` and `src` are on different filesystems (`EXDEV`, which is
/// exactly what happens after undoing a *cross-device* move, since the
/// original source is already gone and the two paths live on different
/// devices), falls back to the same copy+verify+delete engine used for the
/// forward cross-device move, just with `src`/`dest` swapped.
fn undo_move(src: &Path, dest: &Path) -> Result<()> {
    match fs::rename(dest, src) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            let jobs = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            let (tx, rx) = crossbeam_channel::unbounded::<ProgressEvent>();
            let drain = std::thread::spawn(move || for _ in rx {});
            let result = crossdev::move_cross_device(dest, src, jobs, true, true, tx);
            let _ = drain.join();
            result
        }
        Err(e) => Err(XmvError::Journal(format!("Undo rename failed: {e}"))),
    }
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
