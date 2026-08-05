// ops/tree.rs — Recursive directory-tree copy.
//
// Walks the source tree using `symlink_metadata` exclusively — never
// `Path::is_dir()`, which follows symlinks and can spin forever on a
// self-referential symlink. Symlink entries are copied as symlinks (never
// descended into) unless `dereference` is set, in which case a visited-set
// keyed by (dev, ino) still catches true cycles (e.g. a symlink pointing at
// an ancestor directory) and reports `CpError::SymlinkLoop` instead of
// hanging.
//
// Directories are created first (serially, parents-before-children — walk
// order guarantees this), then symlinks are recreated, then regular files
// are copied in parallel on a rayon thread-pool. Directory metadata (mode,
// timestamps) is preserved last, after all children have been written, so
// a child write can't bump the parent's mtime back out from under us.

use std::{
    collections::HashSet,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crossbeam_channel::Sender;
use rayon::prelude::*;

use crate::{
    error::{io_err, CpError, Result},
    metadata,
    ops::file::{self, FileCopyOpts},
    progress::ProgressEvent,
    verify,
};

/// Options governing a recursive tree copy.
#[derive(Debug, Clone, Copy)]
pub struct TreeCopyOpts {
    pub preserve: bool,
    pub dereference: bool,
    pub no_clobber: bool,
    pub update: bool,
    pub one_file_system: bool,
    pub verify: bool,
    pub jobs: usize,
    pub file: FileCopyOpts,
}

enum Entry {
    Dir(PathBuf, PathBuf),
    File(PathBuf, PathBuf),
    Symlink(PathBuf, PathBuf),
}

/// Recursively copy `src` (a directory) to `dest`.
pub fn copy_tree(
    src: &Path,
    dest: &Path,
    opts: TreeCopyOpts,
    tx: Sender<ProgressEvent>,
) -> Result<()> {
    let root_meta = fs::symlink_metadata(src).map_err(|e| io_err(src, e))?;
    let root_dev = root_meta.dev();

    let mut entries = Vec::new();
    let mut visited: HashSet<(u64, u64)> = HashSet::new();
    if root_meta.is_dir() {
        visited.insert((root_meta.dev(), root_meta.ino()));
    }
    walk(
        src,
        dest,
        root_dev,
        opts.one_file_system,
        opts.dereference,
        &mut visited,
        &mut entries,
    )?;

    // ── Directories first, serially, parents before children ────────────────
    for entry in &entries {
        if let Entry::Dir(_, d) = entry {
            fs::create_dir_all(d).map_err(|e| io_err(d, e))?;
        }
    }

    // ── Symlinks: cheap, order-independent ───────────────────────────────────
    for entry in &entries {
        if let Entry::Symlink(s, d) = entry {
            copy_symlink(s, d, opts)?;
        }
    }

    // ── Regular files: parallel copy on a sized rayon pool ───────────────────
    let file_entries: Vec<(&Path, &Path)> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::File(s, d) => Some((s.as_path(), d.as_path())),
            _ => None,
        })
        .collect();

    if !file_entries.is_empty() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(opts.jobs.max(1))
            .build()
            .map_err(|_| CpError::ThreadJoin)?;

        let tx_ref = &tx;
        pool.install(|| -> Result<()> {
            file_entries
                .par_iter()
                .try_for_each(|(s, d)| copy_one_file(s, d, opts, tx_ref))
        })?;
    }

    drop(tx);

    // ── Directory metadata last ───────────────────────────────────────────────
    if opts.preserve {
        for entry in entries.iter().rev() {
            if let Entry::Dir(s, d) = entry {
                metadata::preserve(s, d)?;
            }
        }
        metadata::preserve(src, dest)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn walk(
    src: &Path,
    dest: &Path,
    root_dev: u64,
    one_fs: bool,
    dereference: bool,
    visited: &mut HashSet<(u64, u64)>,
    out: &mut Vec<Entry>,
) -> Result<()> {
    let meta = fs::symlink_metadata(src).map_err(|e| io_err(src, e))?;

    if meta.file_type().is_symlink() {
        if !dereference {
            out.push(Entry::Symlink(src.to_owned(), dest.to_owned()));
            return Ok(());
        }

        // -L/--dereference: follow the symlink. If it targets a directory,
        // descend into it too, guarding against cycles via (dev, ino).
        let target_meta = match fs::metadata(src) {
            Ok(m) => m,
            Err(e) => return Err(io_err(src, e)),
        };

        if target_meta.is_dir() {
            let key = (target_meta.dev(), target_meta.ino());
            if !visited.insert(key) {
                return Err(CpError::SymlinkLoop(src.to_owned()));
            }
            out.push(Entry::Dir(src.to_owned(), dest.to_owned()));
            descend(src, dest, root_dev, one_fs, dereference, visited, out)?;
        } else {
            out.push(Entry::File(src.to_owned(), dest.to_owned()));
        }
        return Ok(());
    }

    if meta.is_dir() {
        if one_fs && meta.dev() != root_dev {
            return Ok(());
        }
        out.push(Entry::Dir(src.to_owned(), dest.to_owned()));
        descend(src, dest, root_dev, one_fs, dereference, visited, out)?;
        return Ok(());
    }

    // Regular file (or other special file, copied as a regular file).
    out.push(Entry::File(src.to_owned(), dest.to_owned()));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn descend(
    src: &Path,
    dest: &Path,
    root_dev: u64,
    one_fs: bool,
    dereference: bool,
    visited: &mut HashSet<(u64, u64)>,
    out: &mut Vec<Entry>,
) -> Result<()> {
    for entry in fs::read_dir(src).map_err(|e| io_err(src, e))? {
        let entry = entry.map_err(|e| io_err(src, e))?;
        let s = entry.path();
        let d = dest.join(entry.file_name());
        walk(&s, &d, root_dev, one_fs, dereference, visited, out)?;
    }
    Ok(())
}

fn copy_symlink(src: &Path, dest: &Path, opts: TreeCopyOpts) -> Result<()> {
    if fs::symlink_metadata(dest).is_ok() {
        if opts.no_clobber {
            return Ok(());
        }
        fs::remove_file(dest).map_err(|e| io_err(dest, e))?;
    }
    let target = fs::read_link(src).map_err(|e| io_err(src, e))?;
    std::os::unix::fs::symlink(&target, dest).map_err(|e| io_err(dest, e))?;
    if opts.preserve {
        metadata::preserve(src, dest)?;
    }
    Ok(())
}

fn copy_one_file(src: &Path, dest: &Path, opts: TreeCopyOpts, tx: &Sender<ProgressEvent>) -> Result<()> {
    if opts.no_clobber && dest.exists() {
        return Ok(());
    }
    if opts.update && dest.exists() && dest_is_newer_or_equal(src, dest) {
        return Ok(());
    }

    file::copy_file_atomic(src, dest, opts.file, tx)?;

    if opts.preserve {
        metadata::preserve(src, dest)?;
    }
    if opts.verify {
        verify::verify(src, dest)?;
    }
    Ok(())
}

pub(crate) fn dest_is_newer_or_equal(src: &Path, dest: &Path) -> bool {
    let s = fs::metadata(src).and_then(|m| m.modified());
    let d = fs::metadata(dest).and_then(|m| m.modified());
    matches!((s, d), (Ok(s), Ok(d)) if d >= s)
}
