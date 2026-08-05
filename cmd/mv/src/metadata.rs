// metadata.rs — Preserve POSIX metadata during cross-device moves.
//
// Covers:
// - Unix permission bits (chmod via set_permissions)
// - Access + modification timestamps (atime / mtime via filetime)
// - Extended attributes including SELinux labels and POSIX ACLs
//
// Called after a successful cross-device copy before the source is deleted,
// so the destination has identical metadata to what the source had.

use std::{fs, path::Path};

use filetime::FileTime;

use crate::error::{io_err, Result};

/// Copy all POSIX metadata from `src` to `dest`.
pub fn preserve(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::metadata(src).map_err(|e| io_err(src, e))?;

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

    Ok(())
}
