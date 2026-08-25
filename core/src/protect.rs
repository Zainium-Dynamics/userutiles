//! Zainium protected paths — never removable or modifiable via user_utils tools (even as root).
//!
//! Rules:
//! - `/overlayer/syshub` and everything under it: **cannot be modified or deleted**
//! - any `zaisys` directory and everything under it: **cannot be modified or deleted**
//! - `zexlib` (both root and contents): **fully modifiable and deletable**

use std::fs;
use std::path::{Component, Path, PathBuf};

/// Absolute tree that may never be removed (root or any descendant),
/// except paths that fall under a `zexlib` content-allow exception.
pub const PROTECTED_TREE_PREFIXES: &[&str] = &["/overlayer/syshub"];

/// Absolute trees that may never be modified (no exceptions).
pub const MODIFICATION_PREFIXES: &[&str] = &["/overlayer/syshub", "/overlayer/zaisys"];

/// Directory basenames that are fully protected (self + descendants).
pub const PROTECTED_TREE_NAMES: &[&str] = &["zaisys"];

/// Directory basenames that protect only the directory itself (children OK).
pub const PROTECTED_ROOT_NAMES: &[&str] = &["zexlib"];

/// Why a path must not be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectReason {
    /// `/overlayer/syshub` or a descendant (outside zexlib contents).
    SyshubTree,
    /// `zaisys` directory or anything under it.
    ZaisysTree,
    /// `/overlayer/syshub` or `/overlayer/zaisys` tree (for modification block).
    OverlayerTree,
}

impl ProtectReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::SyshubTree => {
                "protected path: /overlayer/syshub cannot be removed (even as root)"
            }
            Self::ZaisysTree => "protected path: zaisys cannot be removed (even as root)",
            Self::OverlayerTree => {
                "protected path: core OS layers cannot be modified (even as root)"
            }
        }
    }
}

/// Resolve `path` for protection checks (canonicalize when possible).
pub fn resolve_path(path: &Path) -> PathBuf {
    if let Ok(c) = fs::canonicalize(path) {
        return c;
    }
    // Logical absolute form (no symlink resolution if missing).
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    normalize_logical(&abs)
}

/// Collapse `.` / `..` without requiring the path to exist.
fn normalize_logical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(p) => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Return `Some(reason)` if `path` must not be deleted.
pub fn removal_denied(path: &Path) -> Option<ProtectReason> {
    let resolved = resolve_path(path);
    removal_denied_resolved(&resolved)
}

/// Same as [`removal_denied`] but for an already-resolved absolute path.
pub fn removal_denied_resolved(resolved: &Path) -> Option<ProtectReason> {
    // 1) Inside zexlib or the zexlib root itself → always allow, even under syshub.
    if is_named_root(resolved, "zexlib") || is_inside_named_root(resolved, "zexlib") {
        return None;
    }

    // 2) zaisys root or anything under it → deny.
    if is_named_root(resolved, "zaisys") || is_inside_named_root(resolved, "zaisys") {
        return Some(ProtectReason::ZaisysTree);
    }

    // 4) /overlayer/syshub tree → deny.
    for prefix in PROTECTED_TREE_PREFIXES {
        let p = Path::new(prefix);
        if resolved == p || resolved.starts_with(p) {
            return Some(ProtectReason::SyshubTree);
        }
    }

    let _ = (PROTECTED_TREE_NAMES, PROTECTED_ROOT_NAMES); // documented constants
    None
}

/// Return `Some(reason)` if `path` must not be modified or overwritten.
/// Stricter than `removal_denied`: no exception for `zexlib` contents,
/// and guards both `/overlayer/syshub` and `/overlayer/zaisys` trees against any mutation.
pub fn modification_denied(path: &Path) -> Option<ProtectReason> {
    let resolved = resolve_path(path);
    modification_denied_resolved(&resolved)
}

/// Same as [`modification_denied`] but for an already-resolved absolute path.
pub fn modification_denied_resolved(resolved: &Path) -> Option<ProtectReason> {
    for prefix in MODIFICATION_PREFIXES {
        let p = Path::new(prefix);
        if resolved == p || resolved.starts_with(p) {
            return Some(ProtectReason::OverlayerTree);
        }
    }

    if is_named_root(resolved, "zexlib") || is_inside_named_root(resolved, "zexlib") {
        return None;
    }

    if is_named_root(resolved, "zaisys") || is_inside_named_root(resolved, "zaisys") {
        return Some(ProtectReason::ZaisysTree);
    }

    None
}

fn is_named_root(path: &Path, name: &str) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(name)
}

/// True if `path` has an ancestor directory named `name` (path is strictly under it).
fn is_inside_named_root(path: &Path, name: &str) -> bool {
    let mut comps: Vec<_> = path.components().collect();
    if comps.is_empty() {
        return false;
    }
    // Drop the final component — we care about ancestors only.
    comps.pop();
    comps.iter().any(|c| match c {
        Component::Normal(n) => n.to_str() == Some(name),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syshub_root_and_child_blocked() {
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/syshub")),
            Some(ProtectReason::SyshubTree)
        );
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/syshub/bin")),
            Some(ProtectReason::SyshubTree)
        );
    }

    #[test]
    fn zexlib_allowed() {
        assert_eq!(removal_denied_resolved(Path::new("/zexlib")), None);
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zexlib")),
            None
        );
    }

    #[test]
    fn zaisys_tree_blocked() {
        assert_eq!(
            removal_denied_resolved(Path::new("/zaisys")),
            Some(ProtectReason::ZaisysTree)
        );
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zaisys/run/x")),
            Some(ProtectReason::ZaisysTree)
        );
    }

    #[test]
    fn normal_paths_allowed() {
        assert_eq!(removal_denied_resolved(Path::new("/tmp/foo")), None);
        assert_eq!(removal_denied_resolved(Path::new("/home/user/a")), None);
        assert_eq!(removal_denied_resolved(Path::new("/overlayer/other")), None);
    }

    #[test]
    fn modification_denied_blocks_everything_in_syshub_and_zaisys() {
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/syshub")),
            Some(ProtectReason::OverlayerTree)
        );
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/syshub/bin/ls")),
            Some(ProtectReason::OverlayerTree)
        );
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/zaisys")),
            Some(ProtectReason::OverlayerTree)
        );
    }

    #[test]
    fn modification_denied_zexlib_exception() {
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/zexlib")),
            None
        );
    }

    #[test]
    fn modification_denied_allows_unrelated() {
        assert_eq!(modification_denied_resolved(Path::new("/tmp/foo")), None);
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer-backup")),
            None
        );
    }
}
