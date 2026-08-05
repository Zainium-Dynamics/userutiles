// ops/rename.rs — Intra-filesystem move via rename(2).
//
// rename(2) is the fastest possible move: it is atomic at the kernel level,
// executes in O(1) regardless of file size, and never copies data.
// It only works when source and destination are on the same filesystem
// (same st_dev). The orchestrator (main.rs) calls stat() to decide between
// this path and crossdev.rs.
//
// Flags supported here:
// - --no-clobber → renameat2(RENAME_NOREPLACE) [Linux ≥ 3.15]
// - --no-replace → same as above
// - Standard rename (overwrite) → rename(2) [always available]

use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

use crate::error::{io_err, Result, XmvError};

// renameat2 flags — from <linux/fs.h>
const RENAME_NOREPLACE: libc::c_uint = 1 << 0; // Fail if dest exists
const RENAME_EXCHANGE: libc::c_uint = 1 << 1; // Swap src ↔ dest atomically

// ─── Public API ───────────────────────────────────────────────────────────────

/// Standard atomic rename — overwrites destination if it exists.
/// Uses POSIX rename(2); always succeeds on same-device paths.
pub fn rename_overwrite(src: &Path, dest: &Path) -> Result<()> {
    std::fs::rename(src, dest).map_err(|e| io_err(src, e))
}

/// Atomic rename that FAILS if destination already exists.
/// Uses renameat2(RENAME_NOREPLACE) — no TOCTOU race unlike checking
/// existence then renaming separately.
pub fn rename_no_replace(src: &Path, dest: &Path) -> Result<()> {
    renameat2(src, dest, RENAME_NOREPLACE)
}

/// Atomically exchange two paths — both must exist on the same filesystem.
/// After this call: src is at dest's old location, dest is at src's old location.
/// Uses renameat2(RENAME_EXCHANGE); available on Linux ≥ 3.15.
pub fn rename_exchange(path_a: &Path, path_b: &Path) -> Result<()> {
    renameat2(path_a, path_b, RENAME_EXCHANGE)
}

// ─── renameat2 syscall wrapper ────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn renameat2(src: &Path, dest: &Path, flags: libc::c_uint) -> Result<()> {
    let src_c = path_to_cstring(src)?;
    let dest_c = path_to_cstring(dest)?;

    // renameat2 is syscall #316 on x86-64, #276 on ARM64, #357 on ARM32.
    // We use SYS_renameat2 from libc which resolves the correct number.
    //
    // SAFETY: `src_c`/`dest_c` are `CString`s kept alive across this call, so
    // `src_c.as_ptr()`/`dest_c.as_ptr()` are valid pointers to NUL-terminated C
    // strings for its duration, matching what `renameat2` expects for its path
    // arguments. `libc::AT_FDCWD` is a sentinel constant (not a real fd) telling
    // the kernel to resolve relative paths against the current working
    // directory, which is valid here since `src`/`dest` are ordinary paths, not
    // fd-relative ones. `flags` is a plain `c_uint` bitmask built from the
    // `RENAME_*` constants above. The syscall's return value (0 on success,
    // negative errno-derived value on failure) is checked below before use.
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
        // ENOSYS — kernel too old (< 3.15); caller should fall back.
        Some(libc::ENOSYS) => Err(XmvError::Renameat2Unsupported),
        // EXDEV — paths are on different filesystems (exchange can't cross).
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

// ─── Helper ───────────────────────────────────────────────────────────────────

fn path_to_cstring(path: &Path) -> Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| XmvError::Io {
        path: path.to_owned(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains interior NUL byte",
        ),
    })
}

// ─── Same-device detection ────────────────────────────────────────────────────

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
    // If path doesn't exist (e.g. dest), stat its parent instead.
    let p = if path.exists() {
        path.to_owned()
    } else {
        path.parent()?.to_owned()
    };
    std::fs::metadata(&p).ok().map(|m| m.dev())
}
