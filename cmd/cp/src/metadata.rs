// metadata.rs — Preserve POSIX metadata after a copy (-p/--preserve, -a/--archive).
//
// Covers:
// - Unix permission bits (chmod via set_permissions)
// - Access + modification timestamps (atime / mtime via filetime)
// - Extended attributes including SELinux labels and POSIX ACLs
// - Ownership (uid/gid), best-effort — silently skipped without privileges
//
// Called after a successful copy so the destination ends up with metadata
// identical to what the source had.

use std::{
    ffi::CString,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
};

use filetime::FileTime;

use crate::error::{io_err, Result};

/// Copy all POSIX metadata from `src` to `dest`.
///
/// `src` is inspected with `symlink_metadata` so preserving a symlink's
/// metadata never follows it. Symlinks only carry meaningful ownership
/// (`lchown`); regular files and directories also get their mode bits,
/// timestamps, and extended attributes copied.
pub fn preserve(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(src).map_err(|e| io_err(src, e))?;

    if meta.file_type().is_symlink() {
        let _ = lchown(dest, meta.uid(), meta.gid());
        return Ok(());
    }

    // ── Permission bits ───────────────────────────────────────────────────────
    fs::set_permissions(dest, meta.permissions()).map_err(|e| io_err(dest, e))?;

    // ── Timestamps ────────────────────────────────────────────────────────────
    let atime = FileTime::from_last_access_time(&meta);
    let mtime = FileTime::from_last_modification_time(&meta);
    filetime::set_file_times(dest, atime, mtime).map_err(|e| io_err(dest, e))?;

    // ── Extended attributes ───────────────────────────────────────────────────
    // xattr::list is safe on filesystems without xattr support — returns empty.
    // Errors on individual attributes (e.g. security.* requiring root) are
    // silently skipped; we preserve what we can.
    for attr_name in xattr::list(src).map_err(|e| io_err(src, e))? {
        if let Some(value) = xattr::get(src, &attr_name).map_err(|e| io_err(src, e))? {
            let _ = xattr::set(dest, &attr_name, &value);
        }
    }

    // ── Ownership (best effort, may fail without privileges) ─────────────────
    let _ = chown(dest, meta.uid(), meta.gid());

    Ok(())
}

fn chown(path: &Path, uid: u32, gid: u32) -> std::io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for the
    // duration of this call, so `c.as_ptr()` is a sound `chown(2)` path
    // argument. `uid`/`gid` are plain integers taken from a successful
    // `fs::symlink_metadata` call above, so this call cannot cause memory
    // unsafety regardless of whether it succeeds or fails for permission
    // reasons; only its return value (checked below) is used.
    let ret = unsafe { libc::chown(c.as_ptr(), uid, gid) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn lchown(path: &Path, uid: u32, gid: u32) -> std::io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: same reasoning as `chown` above — `c` is a valid NUL-terminated
    // `CString` alive for the call, `uid`/`gid` are plain integers, and the
    // checked return value is the only thing observed afterwards. `lchown`
    // additionally never dereferences a symlink target, matching the intent
    // of preserving a symlink entry's own ownership.
    let ret = unsafe { libc::lchown(c.as_ptr(), uid, gid) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
