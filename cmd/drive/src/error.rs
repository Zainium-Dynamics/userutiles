use std::fmt;

#[allow(dead_code)]
#[derive(Debug)]
pub enum DriveError {
    DeviceNotFound(String),
    PermissionDenied(String),
    DeviceBusy(String),
    FilesystemError(String),
    SmartUnavailable(String),
    CloneFailed(String),
    SnapshotFailed(String),
    RepairFailed(String),
    Io(std::io::Error),
    Other(String),
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceNotFound(d) => write!(f, "Device not found: {d}"),
            Self::PermissionDenied(d) => write!(
                f,
                "Permission denied accessing: {d} — try running with sudo"
            ),
            Self::DeviceBusy(d) => write!(f, "Device is busy: {d}"),
            Self::FilesystemError(msg) => write!(f, "Filesystem error: {msg}"),
            Self::SmartUnavailable(d) => write!(f, "SMART data unavailable for: {d}"),
            Self::CloneFailed(msg) => write!(f, "Clone failed: {msg}"),
            Self::SnapshotFailed(msg) => write!(f, "Snapshot failed: {msg}"),
            Self::RepairFailed(msg) => write!(f, "Repair failed: {msg}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DriveError {}

impl From<std::io::Error> for DriveError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied(e.to_string()),
            std::io::ErrorKind::NotFound => Self::DeviceNotFound(e.to_string()),
            _ => Self::Io(e),
        }
    }
}

impl From<anyhow::Error> for DriveError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_device_not_found() {
        let e = DriveError::DeviceNotFound("sdc".into());
        assert!(e.to_string().contains("sdc"));
    }

    #[test]
    fn display_permission_denied() {
        let e = DriveError::PermissionDenied("/dev/sda".into());
        assert!(e.to_string().to_lowercase().contains("permission"));
    }

    #[test]
    fn display_filesystem_error() {
        let e = DriveError::FilesystemError("corrupt superblock".into());
        assert!(e.to_string().contains("corrupt superblock"));
    }

    #[test]
    fn from_io_permission_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let drive_err = DriveError::from(io_err);
        assert!(matches!(drive_err, DriveError::PermissionDenied(_)));
    }

    #[test]
    fn from_io_not_found() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let drive_err = DriveError::from(io_err);
        assert!(matches!(drive_err, DriveError::DeviceNotFound(_)));
    }
}
