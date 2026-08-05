use std::fmt;

#[derive(Debug)]
pub enum TraceError {
    PermissionDenied(String),
    ProcessNotFound(String),
    PtraceError(String),
    IoError(String),
    ConfigError(String),
    OutputError(String),
    SyscallError(String),
    ProcfsError(String),
    SerializationError(String),
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TraceError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            TraceError::ProcessNotFound(msg) => write!(f, "Process not found: {}", msg),
            TraceError::PtraceError(msg) => write!(f, "Ptrace error: {}", msg),
            TraceError::IoError(msg) => write!(f, "IO error: {}", msg),
            TraceError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            TraceError::OutputError(msg) => write!(f, "Output error: {}", msg),
            TraceError::SyscallError(msg) => write!(f, "Syscall error: {}", msg),
            TraceError::ProcfsError(msg) => write!(f, "Procfs error: {}", msg),
            TraceError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for TraceError {}

impl From<std::io::Error> for TraceError {
    fn from(err: std::io::Error) -> Self {
        TraceError::IoError(err.to_string())
    }
}

impl From<procfs::ProcError> for TraceError {
    fn from(err: procfs::ProcError) -> Self {
        TraceError::ProcfsError(err.to_string())
    }
}

pub type TraceResult<T> = Result<T, TraceError>;
