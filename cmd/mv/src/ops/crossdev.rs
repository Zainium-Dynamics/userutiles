// ops/crossdev.rs — Cross-device move implementation.
//
// When source and destination are on different filesystems, rename(2) fails
// with EXDEV. The correct strategy is:
//
// 1. Copy every file from src to dest (parallel, via copy_file_range)
// 2. Verify every file with XXH3-128 (optional but default-on)
// 3. Delete the source (only after successful verify)
//
// This is the only operation in xmv that moves data — all other operations
// are metadata-only rename() calls. Progress bars are shown here because
// moves can take seconds-to-minutes for large directories.
//
// The copy engine is intentionally re-implemented (not imported from xcp)
// so that xmv has no binary dependency on xcp. The logic is identical.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
};

use crossbeam_channel::Sender;
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::{
    error::{io_err, Result, XmvError},
    metadata,
    progress::ProgressEvent,
    verify,
};

// ─── Public entry point ───────────────────────────────────────────────────────

/// Move `src` to `dest` across filesystem boundaries.
///
/// Steps:
/// 1. Discover all files under `src`.
/// 2. Copy them in parallel to `dest` using copy_file_range / buffered fallback.
/// 3. If `do_verify` is true, hash every (src, dest) pair with XXH3-128.
/// 4. Delete the source tree only after all verifications pass.
/// 5. If `preserve_meta` is true, copy permissions / timestamps / xattrs.
pub fn move_cross_device(
    src: &Path,
    dest: &Path,
    jobs: usize,
    do_verify: bool,
    preserve_meta: bool,
    tx: Sender<ProgressEvent>,
) -> Result<()> {
    // ── Step 1: discover ──────────────────────────────────────────────────────
    let tasks = discover(src, dest)?;

    if tasks.is_empty() {
        // src is an empty directory — just recreate it at dest.
        fs::create_dir_all(dest).map_err(|e| io_err(dest, e))?;
        fs::remove_dir(src).map_err(|e| io_err(src, e))?;
        return Ok(());
    }

    // ── Step 2: parallel copy ─────────────────────────────────────────────────
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .map_err(|_| XmvError::ThreadJoin)?;

    let tx_ref = &tx;
    let meta_ref = preserve_meta;

    pool.install(|| -> Result<()> {
        tasks
            .par_iter()
            .try_for_each(|(src_file, dest_file)| copy_one(src_file, dest_file, tx_ref, meta_ref))
    })?;

    // Drop the sender so the progress thread can finish.
    drop(tx);

    // ── Step 3: verify ────────────────────────────────────────────────────────
    if do_verify {
        for (src_file, dest_file) in &tasks {
            verify::verify(src_file, dest_file)?;
        }
    }

    // ── Step 4: delete source ─────────────────────────────────────────────────
    // Remove files first, then prune now-empty directories bottom-up.
    for (src_file, _) in &tasks {
        fs::remove_file(src_file).map_err(|e| io_err(src_file, e))?;
    }
    remove_empty_dirs(src)?;

    Ok(())
}

// ─── File discovery ───────────────────────────────────────────────────────────

fn discover(src: &Path, dest: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    if src.is_file() {
        let dest_path = if dest.is_dir() {
            dest.join(src.file_name().expect("src has no filename"))
        } else {
            dest.to_owned()
        };
        return Ok(vec![(src.to_owned(), dest_path)]);
    }

    let dest_root: PathBuf = if dest.exists() {
        dest.join(src.file_name().expect("src dir has no name"))
    } else {
        dest.to_owned()
    };

    let entries: Vec<PathBuf> = WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| {
            let e = e.ok()?;
            if e.file_type().is_dir() {
                return None;
            }
            Some(e.into_path())
        })
        .collect();

    let src_ref = src;
    let dest_root_ref = &dest_root;

    let pairs: Result<Vec<_>> = entries
        .into_par_iter()
        .map(|src_path| {
            let rel = src_path.strip_prefix(src_ref).expect("always under src");
            let dest_path = dest_root_ref.join(rel);
            Ok((src_path, dest_path))
        })
        .collect();

    pairs
}

// ─── Per-file copy ────────────────────────────────────────────────────────────

fn copy_one(
    src: &Path,
    dest: &Path,
    tx: &Sender<ProgressEvent>,
    preserve_meta: bool,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }

    let src_size = src.metadata().map_err(|e| io_err(src, e))?.len();

    let _ = tx.send(ProgressEvent::FileStart {
        path: src.to_owned(),
        total: src_size,
    });

    copy_file_range_or_fallback(src, dest, tx)?;

    if preserve_meta {
        metadata::preserve(src, dest)?;
    }

    let _ = tx.send(ProgressEvent::FileDone {
        path: src.to_owned(),
        bytes: src_size,
    });

    Ok(())
}

// ─── copy_file_range loop (same as xcp — kernel zero-copy) ───────────────────

fn copy_file_range_or_fallback(src: &Path, dest: &Path, tx: &Sender<ProgressEvent>) -> Result<()> {
    let src_file = File::open(src).map_err(|e| io_err(src, e))?;
    let dest_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest)
        .map_err(|e| io_err(dest, e))?;

    let src_size = src_file.metadata().map_err(|e| io_err(src, e))?.len();
    let mut src_off: i64 = 0;
    let mut dest_off: i64 = 0;
    let mut total: u64 = 0;

    loop {
        let remaining = src_size.saturating_sub(total) as usize;
        if remaining == 0 {
            break;
        }
        let chunk = remaining.min(128 * 1024 * 1024);

        // SAFETY: `src_file`/`dest_file` are open for the duration of this call and
        // their raw fds are valid; `&mut src_off`/`&mut dest_off` are valid pointers
        // to live, properly-aligned `i64` locals matching the syscall's `loff_t*`
        // ABI, which the kernel reads and updates in place. `chunk` is a `usize`
        // bounded above by `remaining` (itself bounded by `src_size`), so it cannot
        // overflow to something absurd, and `flags` is `0`. The return value is a
        // plain `i64` (bytes copied or a negative errno), checked below before use;
        // a negative/zero result (e.g. `ENOSYS` on old kernels) falls back to
        // `buffered_copy` rather than being trusted.
        #[cfg(target_os = "linux")]
        let n = unsafe {
            libc::syscall(
                libc::SYS_copy_file_range,
                src_file.as_raw_fd(),
                &mut src_off as *mut i64,
                dest_file.as_raw_fd(),
                &mut dest_off as *mut i64,
                chunk,
                0u32,
            )
        };

        #[cfg(target_os = "redox")]
        let n: i64 = -1;

        match n {
            n if n > 0 => {
                total += n as u64;
                let _ = tx.send(ProgressEvent::Progress {
                    path: src.to_owned(),
                    bytes: n as u64,
                });
            }
            0 => break,
            _ => return buffered_copy(src, dest, tx),
        }
    }

    Ok(())
}

fn buffered_copy(src: &Path, dest: &Path, tx: &Sender<ProgressEvent>) -> Result<()> {
    let mut src_file = File::open(src).map_err(|e| io_err(src, e))?;
    let mut dest_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest)
        .map_err(|e| io_err(dest, e))?;

    let mut buf = vec![0u8; 1024 * 1024];

    loop {
        let n = src_file.read(&mut buf).map_err(|e| io_err(src, e))?;
        if n == 0 {
            break;
        }
        dest_file
            .write_all(&buf[..n])
            .map_err(|e| io_err(dest, e))?;
        let _ = tx.send(ProgressEvent::Progress {
            path: src.to_owned(),
            bytes: n as u64,
        });
    }

    Ok(())
}

// ─── Source cleanup ───────────────────────────────────────────────────────────

/// Remove directories under `root` bottom-up after all files have been deleted.
fn remove_empty_dirs(root: &Path) -> Result<()> {
    if root.is_file() {
        return Ok(());
    }

    // Collect all directories, deepest first.
    let mut dirs: Vec<PathBuf> = WalkDir::new(root)
        .contents_first(true)
        .into_iter()
        .filter_map(|e| {
            let e = e.ok()?;
            if e.file_type().is_dir() {
                Some(e.into_path())
            } else {
                None
            }
        })
        .collect();

    // The root itself last.
    dirs.push(root.to_owned());

    for dir in dirs {
        // remove_dir only succeeds if the directory is empty.
        // Silently ignore errors (e.g. dir already removed, or not empty due
        // to files we didn't create — leave them untouched).
        let _ = fs::remove_dir(&dir);
    }

    Ok(())
}
