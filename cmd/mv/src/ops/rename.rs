use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

use crate::error::{io_err, Result, XmvError};

const RENAME_NOREPLACE: libc::c_uint = 1 << 0;
const RENAME_EXCHANGE: libc::c_uint = 1 << 1;

/// Standard atomic rename: overwrites destination if it exists.
/// Uses POSIX rename(2); always succeeds on same-device paths.
pub fn rename_overwrite(src: &Path, dest: &Path) -> Result<()> {
    std::fs::rename(src, dest).map_err(|e| io_err(src, e))
}

/// Atomic rename that FAILS if destination already exists.
/// Uses renameat2(RENAME_NOREPLACE): no TOCTOU race unlike checking
/// existence then renaming separately.
pub fn rename_no_replace(src: &Path, dest: &Path) -> Result<()> {
    renameat2(src, dest, RENAME_NOREPLACE)
}

/// Atomically exchange two paths: both must exist on the same filesystem.
/// After this call: src is at dest's old location, dest is at src's old location.
/// Uses renameat2(RENAME_EXCHANGE); available on Linux ≥ 3.15.
pub fn rename_exchange(path_a: &Path, path_b: &Path) -> Result<()> {
    renameat2(path_a, path_b, RENAME_EXCHANGE)
}

#[cfg(target_os = "linux")]
fn renameat2(src: &Path, dest: &Path, flags: libc::c_uint) -> Result<()> {
    let src_c = path_to_cstring(src)?;
    let dest_c = path_to_cstring(dest)?;

    // SAFETY: `src_c`/`dest_c` are `CString`s kept alive across this call, so
    let ret = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            src_c.as_ptr(),
            libc::AT_FDCWD,
            dest_c.as_ptr(),
            flags,
        )
    };

    if ret == 0 {
        return Ok(());
    }

    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::ENOSYS) => Err(XmvError::Renameat2Unsupported),
        Some(libc::EXDEV) => Err(XmvError::ExchangeCrossDevice(src.to_owned())),
        _ => Err(io_err(src, err)),
    }
}

/// Redox OS: rename(2) is available but renameat2 is not yet implemented.
/// --no-replace and --exchange degrade gracefully with an informative error.
#[cfg(target_os = "redox")]
fn renameat2(src: &Path, _dest: &Path, _flags: libc::c_uint) -> Result<()> {
    Err(XmvError::Renameat2Unsupported)
}

fn path_to_cstring(path: &Path) -> Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| XmvError::Io {
        path: path.to_owned(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains interior NUL byte",
        ),
    })
}

/// Returns true if `a` and `b` reside on the same filesystem (same st_dev).
/// If either stat() fails we conservatively return false (triggers cross-device path).
pub fn same_device(a: &Path, b_parent: &Path) -> bool {
    let dev_a = stat_dev(a);
    let dev_b = stat_dev(b_parent);
    match (dev_a, dev_b) {
        (Some(da), Some(db)) => da == db,
        _ => false,
    }
}

fn stat_dev(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    let p = if path.exists() {
        path.to_owned()
    } else {
        path.parent()?.to_owned()
    };
    std::fs::metadata(&p).ok().map(|m| m.dev())
}
