use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::{Result, XmvError};

/// Move `path` into the XDG Trash.
///
/// Returns the path inside the Trash where the item was stored, so the
/// caller can record it in the undo journal.
pub fn move_to_trash(path: &Path) -> Result<PathBuf> {
    let trash = TrashDir::resolve()?;
    trash.send(path)
}

struct TrashDir {
    files_dir: PathBuf,
    info_dir: PathBuf,
}

impl TrashDir {
    /// Resolve and initialise the XDG Trash directories.
    fn resolve() -> Result<Self> {
        let root = xdg_trash_root()?;
        let files_dir = root.join("files");
        let info_dir = root.join("info");

        fs::create_dir_all(&files_dir)
            .map_err(|e| XmvError::Trash(format!("Cannot create Trash/files: {e}")))?;
        fs::create_dir_all(&info_dir)
            .map_err(|e| XmvError::Trash(format!("Cannot create Trash/info: {e}")))?;

        Ok(Self {
            files_dir,
            info_dir,
        })
    }

    /// Move `path` into the Trash, writing the accompanying .trashinfo file.
    fn send(&self, path: &Path) -> Result<PathBuf> {
        let abs_path = path
            .canonicalize()
            .map_err(|e| crate::error::io_err(path, e))?;

        let base_name = abs_path
            .file_name()
            .ok_or_else(|| {
                XmvError::Trash(format!(
                    "Cannot determine filename for '{}'",
                    abs_path.display()
                ))
            })?
            .to_string_lossy()
            .into_owned();

        let trash_dest = self.unique_name(&base_name);
        let info_path = self.info_dir.join(format!(
            "{}.trashinfo",
            trash_dest.file_name().unwrap().to_string_lossy()
        ));

        write_trashinfo(&info_path, &abs_path)?;

        if fs::rename(&abs_path, &trash_dest).is_err() {
            copy_to_trash(&abs_path, &trash_dest)?;
            fs::remove_file(&abs_path).map_err(|e| crate::error::io_err(&abs_path, e))?;
        }

        Ok(trash_dest)
    }

    /// Return a path inside Trash/files/ that does not yet exist.
    /// Strategy: try the bare name first, then append ".2", ".3", etc.
    fn unique_name(&self, base: &str) -> PathBuf {
        let candidate = self.files_dir.join(base);
        if !candidate.exists() {
            return candidate;
        }

        let stem = Path::new(base)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| base.to_owned());
        let ext = Path::new(base)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();

        for n in 2u32.. {
            let name = format!("{stem}.{n}{ext}");
            let path = self.files_dir.join(&name);
            if !path.exists() {
                return path;
            }
        }
        self.files_dir.join(format!("{base}.{}", unix_ts()))
    }
}

fn write_trashinfo(info_path: &Path, original: &Path) -> Result<()> {
    let deletion_date = iso8601_now();
    let content = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        original.display(),
        deletion_date,
    );
    fs::write(info_path, content)
        .map_err(|e| XmvError::Trash(format!("Cannot write .trashinfo: {e}")))
}

fn copy_to_trash(src: &Path, dest: &Path) -> Result<()> {
    if src.is_dir() {
        for entry in walkdir::WalkDir::new(src) {
            let entry = entry.map_err(|e| XmvError::Trash(e.to_string()))?;
            let rel = entry.path().strip_prefix(src).expect("under src");
            let tgt = dest.join(rel);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&tgt).map_err(|e| crate::error::io_err(&tgt, e))?;
            } else {
                fs::copy(entry.path(), &tgt).map_err(|e| crate::error::io_err(entry.path(), e))?;
            }
        }
    } else {
        fs::copy(src, dest).map_err(|e| crate::error::io_err(src, e))?;
    }
    Ok(())
}

fn xdg_trash_root() -> Result<PathBuf> {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".local").join("share")
        });
    Ok(base.join("Trash"))
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// ISO 8601 local datetime string for .trashinfo: e.g. "2024-01-15T13:45:00".
/// We avoid pulling in a chrono dependency; the format only needs to be
/// parseable by file managers, not sub-second accurate.
fn iso8601_now() -> String {
    #[allow(deprecated)]
    let ts = unix_ts() as libc::time_t;
    let mut tm: libc::tm = libc::tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    // SAFETY: `ts` points to a valid, initialized `libc::time_t` on the stack and
    unsafe { libc::localtime_r(&ts, &mut tm) };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
    )
}
