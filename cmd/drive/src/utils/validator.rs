use std::path::Path;

/// Returns true if the string looks like a valid block device path or name
/// (no path traversal, no embedded separators, safe character set).
pub fn is_valid_device(device: &str) -> bool {
    let clean = device.trim_start_matches("/dev/");
    // Must be non-empty, contain only safe chars, not include traversal
    !clean.is_empty()
        && !clean.contains("..")
        && !clean.contains('/')
        && clean
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Normalize a bare device name (`sdb1`) or full path (`/dev/sdb1`) to a
/// full `/dev/...` path. Returns `None` if `device` fails [`is_valid_device`]
/// (path traversal, embedded separators, or empty input) so callers never
/// build a shell/exec argument from unsanitized device input.
pub fn normalize_device(device: &str) -> Option<String> {
    if !is_valid_device(device) {
        return None;
    }
    Some(if device.starts_with("/dev/") {
        device.to_string()
    } else {
        format!("/dev/{device}")
    })
}

/// Returns true if the path is a real block device
#[allow(dead_code)]
pub fn device_exists(device: &str) -> bool {
    let path = if device.starts_with("/dev/") {
        device.to_string()
    } else {
        format!("/dev/{device}")
    };
    Path::new(&path).exists()
}

/// Returns true if the filesystem name is in the supported list
pub fn is_supported_fs(fs: &str) -> bool {
    matches!(
        fs.to_lowercase().as_str(),
        "ext4" | "btrfs" | "xfs" | "exfat" | "vfat" | "ntfs" | "f2fs"
    )
}

/// Returns true if `name` is safe to join onto a directory path as a single
/// path component — rejects empty names, `.`/`..`, and any path separator,
/// preventing path traversal via `drive snapshot delete/restore <name>`.
pub fn is_valid_snapshot_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_device_names() {
        assert!(is_valid_device("nvme0n1"));
        assert!(is_valid_device("sda"));
        assert!(is_valid_device("sdb1"));
        assert!(is_valid_device("/dev/nvme0n1p1"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_valid_device("../../etc/passwd"));
        assert!(!is_valid_device("/dev/../etc/shadow"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_valid_device(""));
        assert!(!is_valid_device("/dev/"));
    }

    #[test]
    fn supported_filesystems() {
        for fs in ["ext4", "btrfs", "xfs", "exfat", "vfat", "ntfs", "f2fs"] {
            assert!(is_supported_fs(fs), "{fs} should be supported");
        }
    }

    #[test]
    fn rejects_unknown_fs() {
        assert!(!is_supported_fs("reiserfs"));
        assert!(!is_supported_fs("zfs"));
        assert!(!is_supported_fs(""));
    }

    #[test]
    fn normalize_device_accepts_valid() {
        assert_eq!(normalize_device("sdb1").as_deref(), Some("/dev/sdb1"));
        assert_eq!(
            normalize_device("/dev/nvme0n1p1").as_deref(),
            Some("/dev/nvme0n1p1")
        );
    }

    #[test]
    fn normalize_device_rejects_traversal_and_injection() {
        assert_eq!(normalize_device("../../etc/passwd"), None);
        assert_eq!(normalize_device("sdb1; rm -rf /"), None);
        assert_eq!(normalize_device(""), None);
    }

    #[test]
    fn snapshot_name_accepts_plain_names() {
        assert!(is_valid_snapshot_name("2026-07-22_1200"));
        assert!(is_valid_snapshot_name("pre-upgrade"));
    }

    #[test]
    fn snapshot_name_rejects_traversal() {
        assert!(!is_valid_snapshot_name(""));
        assert!(!is_valid_snapshot_name("."));
        assert!(!is_valid_snapshot_name(".."));
        assert!(!is_valid_snapshot_name("../../etc/passwd"));
        assert!(!is_valid_snapshot_name("a/b"));
        assert!(!is_valid_snapshot_name("a\\b"));
    }
}
