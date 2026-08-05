//! Path helpers with Zainium OS path-byte fidelity.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// Return true if `path` is exactly `-` (stdin/stdout sentinel).
pub fn is_stdin_dash(path: &OsStr) -> bool {
    path.as_bytes() == b"-"
}

/// Expand a user path; does not resolve symlinks.
pub fn to_path_buf(s: impl AsRef<OsStr>) -> PathBuf {
    PathBuf::from(s.as_ref())
}

/// Join base + relative safely (reject absolute `relative` overwriting base).
pub fn safe_join(base: &Path, relative: &Path) -> PathBuf {
    let mut out = base.to_path_buf();
    for c in relative.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => {
                // Absolute component — reset to that path.
                out = PathBuf::from(c.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(p) => out.push(p),
        }
    }
    out
}

/// Display path for errors (lossy UTF-8).
pub fn display_lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
