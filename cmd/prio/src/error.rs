use thiserror::Error;

/// Central error type for `prio`. Every sub-system converts its errors into
/// one of these variants so the top-level handler can produce a uniform,
/// styled terminal message.
#[derive(Error, Debug)]
pub enum PrioError {
    #[error("permission denied")]
    PermissionDenied,

    #[error("process not found: PID {0}")]
    ProcessNotFound(u32),

    #[error("process name not found: '{0}'")]
    ProcessNameNotFound(String),

    #[error("invalid niceness level {0}: must be -20 to +19")]
    InvalidNiceness(i32),

    #[error("invalid CPU level {0}: must be 0-100")]
    InvalidCpuLevel(u32),

    #[error("cgroup error: {0}")]
    CgroupError(String),

    #[error("I/O priority error: {0}")]
    IoPriorityError(String),

    #[error("memory size parse error: '{0}' — use e.g. 4G, 512M")]
    MemoryParseError(String),

    #[error("duration parse error: '{0}' — use e.g. 10m, 2h, 30s")]
    DurationParseError(String),

    #[error("unknown I/O mode '{0}': use realtime, high, normal, or idle")]
    UnknownIoMode(String),

    #[error("system error: {0}")]
    SystemError(String),

    #[error("process spawn failed: {0}")]
    SpawnError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Convenience alias for `Result<T, PrioError>`, used throughout `prio`.
pub type Result<T> = std::result::Result<T, PrioError>;

impl PrioError {
    /// A short human-readable fix suggestion shown beneath the error message.
    pub fn fix_hint(&self) -> Option<String> {
        match self {
            PrioError::PermissionDenied => {
                Some("Re-run with sudo, or grant CAP_SYS_NICE to the binary.".to_string())
            }
            PrioError::CgroupError(_) => {
                Some("Memory limits require root and cgroup v2 support.".to_string())
            }
            PrioError::IoPriorityError(_) => {
                Some("I/O realtime class requires root (CAP_SYS_ADMIN).".to_string())
            }
            _ => None,
        }
    }
}
