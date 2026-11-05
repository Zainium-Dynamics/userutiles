use std::path::Path;

use crate::{
    error::{Result, XmvError},
    ops::rename,
    undo::{Journal, Operation},
};

/// Atomically exchange the contents of `path_a` and `path_b`.
///
/// After a successful call:
/// - path_a holds what was previously at path_b
/// - path_b holds what was previously at path_a
///
/// The operation is recorded in `journal` so it can be reversed with --undo
/// (which simply calls atomic_exchange again with the same two paths).
pub fn atomic_exchange(path_a: &Path, path_b: &Path, journal: &mut Journal) -> Result<()> {
    if !path_a.exists() {
        return Err(XmvError::SourceNotFound(path_a.to_owned()));
    }
    if !path_b.exists() {
        return Err(XmvError::SourceNotFound(path_b.to_owned()));
    }

    if !rename::same_device(path_a, path_b) {
        return Err(XmvError::ExchangeCrossDevice(path_a.to_owned()));
    }

    journal.record(Operation::Exchange {
        path_a: path_a.to_owned(),
        path_b: path_b.to_owned(),
    })?;

    match rename::rename_exchange(path_a, path_b) {
        Ok(()) => Ok(()),
        Err(XmvError::Renameat2Unsupported) => {
            atomic_exchange_fallback(path_a, path_b)
        }
        Err(e) => Err(e),
    }
}

/// Non-atomic exchange fallback for kernels that lack renameat2.
/// Uses a temporary path on the same filesystem to avoid cross-device copies.
fn atomic_exchange_fallback(path_a: &Path, path_b: &Path) -> Result<()> {
    use crate::error::io_err;

    let tmp = path_a.with_extension("__xmv_swap__");

    std::fs::rename(path_a, &tmp).map_err(|e| io_err(path_a, e))?;
    std::fs::rename(path_b, path_a).map_err(|e| {
        let _ = std::fs::rename(&tmp, path_a);
        io_err(path_b, e)
    })?;
    std::fs::rename(&tmp, path_b).map_err(|e| {
        io_err(&tmp, e)
    })?;

    Ok(())
}

/// Move `src` to `dest` only if `dest` does not already exist.
///
/// Uses renameat2(RENAME_NOREPLACE): the atomicity guarantee means there
/// is no window between "check if dest exists" and "perform the rename",
/// eliminating the TOCTOU race that a naive `if !dest.exists() { rename }` has.
///
/// Falls back to a best-effort check + rename on older kernels.
pub fn atomic_no_replace(src: &Path, dest: &Path, journal: &mut Journal) -> Result<()> {
    if !src.exists() {
        return Err(XmvError::SourceNotFound(src.to_owned()));
    }

    journal.record(Operation::Move {
        src: src.to_owned(),
        dest: dest.to_owned(),
    })?;

    match rename::rename_no_replace(src, dest) {
        Ok(()) => Ok(()),
        Err(XmvError::Renameat2Unsupported) => {
            if dest.exists() {
                Err(XmvError::NoClobber(dest.to_owned()))
            } else {
                std::fs::rename(src, dest).map_err(|e| crate::error::io_err(src, e))
            }
        }
        Err(e) => Err(e),
    }
}
