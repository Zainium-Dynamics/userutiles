use std::{
    ffi::CString,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
};

use filetime::FileTime;

use crate::error::{io_err, Result};

/// The mode a copy gets without -p, as POSIX cp gives it: an existing destination keeps
/// its own (`existing`), a new one gets the source's permission bits minus the umask.
pub fn plain_mode(src: &Path, dest: &Path, existing: Option<u32>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = match existing {
        Some(m) => m & 0o7777,
        None => {
            let src_mode = fs::metadata(src).map_err(|e| io_err(src, e))?.permissions().mode();
            // SAFETY: umask() only swaps the process mask; it is put back at once
            let mask = unsafe {
                let m = libc::umask(0);
                libc::umask(m);
                m as u32
            };
            src_mode & 0o777 & !mask
        }
    };
    fs::set_permissions(dest, fs::Permissions::from_mode(mode)).map_err(|e| io_err(dest, e))
}

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

    fs::set_permissions(dest, meta.permissions()).map_err(|e| io_err(dest, e))?;

    let atime = FileTime::from_last_access_time(&meta);
    let mtime = FileTime::from_last_modification_time(&meta);
    filetime::set_file_times(dest, atime, mtime).map_err(|e| io_err(dest, e))?;

    for attr_name in xattr::list(src).map_err(|e| io_err(src, e))? {
        if let Some(value) = xattr::get(src, &attr_name).map_err(|e| io_err(src, e))? {
            let _ = xattr::set(dest, &attr_name, &value);
        }
    }

    let _ = chown(dest, meta.uid(), meta.gid());

    Ok(())
}

fn chown(path: &Path, uid: u32, gid: u32) -> std::io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for the
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
    let ret = unsafe { libc::lchown(c.as_ptr(), uid, gid) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
