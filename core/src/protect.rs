//! Zainium protected paths — never removable or modifiable via user_utils tools (even as root).
//!
//! Rules:
//! - `/overlayer/syshub` and everything under it: **cannot be modified or deleted**
//! - `zaisys` itself (the directory, not its contents): **cannot be deleted**
//! - `zaisys/kernel` and everything under it: **cannot be modified or deleted**
//! - everything else under `zaisys`: writable (carve's install-only files live here)
//! - `zexlib` (both root and contents): **fully modifiable and deletable**

use std::fs;
use std::path::{Component, Path, PathBuf};

pub const PROTECTED_TREE_PREFIXES: &[&str] = &["/overlayer/syshub"];

pub const MODIFICATION_PREFIXES: &[&str] = &["/overlayer/syshub"];

pub const ZAISYS_KERNEL_PREFIXES: &[&str] =
    &["/overlayer/zaisys/kernel", "/overlayer/zaisys/limine"];

pub const MUTABLE_STATE_PREFIXES: &[&str] = &["/overlayer/syshub/etc", "/overlayer/syshub/var"];

pub const PROTECTED_TREE_NAMES: &[&str] = &[];

pub const PROTECTED_ROOT_NAMES: &[&str] = &["zexlib", "zaisys"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectReason {
    SyshubTree,
    ZaisysRoot,
    ZaisysKernel,
    OverlayerTree,
}

impl ProtectReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::SyshubTree => {
                "protected path: /overlayer/syshub cannot be removed (even as root)"
            }
            Self::ZaisysRoot => "protected path: zaisys itself cannot be removed (even as root)",
            Self::ZaisysKernel => {
                "protected path: zaisys/kernel cannot be modified or removed (even as root)"
            }
            Self::OverlayerTree => {
                "protected path: core OS layers cannot be modified (even as root)"
            }
        }
    }
}

pub fn resolve_path(path: &Path) -> PathBuf {
    if let Ok(c) = fs::canonicalize(path) {
        return c;
    }
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    normalize_logical(&abs)
}

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

pub fn removal_denied(path: &Path) -> Option<ProtectReason> {
    let resolved = resolve_path(path);
    removal_denied_resolved(&resolved)
}

pub fn removal_denied_resolved(resolved: &Path) -> Option<ProtectReason> {
    if is_named_root(resolved, "zexlib") || is_inside_named_root(resolved, "zexlib") {
        return None;
    }

    for prefix in ZAISYS_KERNEL_PREFIXES {
        let p = Path::new(prefix);
        if resolved == p || resolved.starts_with(p) {
            return Some(ProtectReason::ZaisysKernel);
        }
    }

    if is_named_root(resolved, "zaisys") {
        return Some(ProtectReason::ZaisysRoot);
    }

    for prefix in PROTECTED_TREE_PREFIXES {
        let p = Path::new(prefix);
        if resolved == p || resolved.starts_with(p) {
            return Some(ProtectReason::SyshubTree);
        }
    }

    let _ = (PROTECTED_TREE_NAMES, PROTECTED_ROOT_NAMES);
    None
}

pub fn modification_denied(path: &Path) -> Option<ProtectReason> {
    let resolved = resolve_path(path);
    modification_denied_resolved(&resolved)
}

pub fn modification_denied_resolved(resolved: &Path) -> Option<ProtectReason> {
    for prefix in MUTABLE_STATE_PREFIXES {
        let p = Path::new(prefix);
        if resolved == p || resolved.starts_with(p) {
            return None;
        }
    }

    for prefix in MODIFICATION_PREFIXES {
        let p = Path::new(prefix);
        if resolved == p || resolved.starts_with(p) {
            return Some(ProtectReason::OverlayerTree);
        }
    }

    if is_named_root(resolved, "zexlib") || is_inside_named_root(resolved, "zexlib") {
        return None;
    }

    for prefix in ZAISYS_KERNEL_PREFIXES {
        let p = Path::new(prefix);
        if resolved == p || resolved.starts_with(p) {
            return Some(ProtectReason::ZaisysKernel);
        }
    }

    if is_named_root(resolved, "zaisys") {
        return Some(ProtectReason::ZaisysRoot);
    }

    None
}

fn is_named_root(path: &Path, name: &str) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(name)
}

fn is_inside_named_root(path: &Path, name: &str) -> bool {
    let mut comps: Vec<_> = path.components().collect();
    if comps.is_empty() {
        return false;
    }
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
    fn zaisys_root_blocked_but_contents_allowed() {
        assert_eq!(
            removal_denied_resolved(Path::new("/zaisys")),
            Some(ProtectReason::ZaisysRoot)
        );
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zaisys")),
            Some(ProtectReason::ZaisysRoot)
        );
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zaisys/bin/carve")),
            None
        );
        assert_eq!(
            removal_denied_resolved(Path::new(
                "/overlayer/zaisys/lib/systemd/system/carve-install.service"
            )),
            None
        );
    }

    #[test]
    fn zaisys_kernel_blocked() {
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zaisys/kernel")),
            Some(ProtectReason::ZaisysKernel)
        );
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zaisys/kernel/quantra-ramfs.img")),
            Some(ProtectReason::ZaisysKernel)
        );
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zaisys/kernel/linux-evl")),
            Some(ProtectReason::ZaisysKernel)
        );
    }

    #[test]
    fn zaisys_limine_blocked() {
        assert_eq!(
            removal_denied_resolved(Path::new("/overlayer/zaisys/limine/limine.conf")),
            Some(ProtectReason::ZaisysKernel)
        );
    }

    #[test]
    fn normal_paths_allowed() {
        assert_eq!(removal_denied_resolved(Path::new("/tmp/foo")), None);
        assert_eq!(removal_denied_resolved(Path::new("/home/user/a")), None);
        assert_eq!(removal_denied_resolved(Path::new("/overlayer/other")), None);
    }

    #[test]
    fn modification_denied_blocks_syshub_and_zaisys_kernel_only() {
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/syshub")),
            Some(ProtectReason::OverlayerTree)
        );
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/syshub/bin/ls")),
            Some(ProtectReason::OverlayerTree)
        );
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/zaisys/kernel/linux-evl")),
            Some(ProtectReason::ZaisysKernel)
        );
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/zaisys")),
            Some(ProtectReason::ZaisysRoot)
        );
        assert_eq!(
            modification_denied_resolved(Path::new("/overlayer/zaisys/bin/carve")),
            None
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
