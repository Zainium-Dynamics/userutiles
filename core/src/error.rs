//! Unified error types for ZEX utils.

use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::exit::{EXIT_FAILURE, EXIT_USAGE};

/// Result alias for ZEX operations.
pub type ZexResult<T> = Result<T, ZexError>;

/// Error kinds shared by utilities.
#[derive(Debug)]
pub enum ZexError {
    /// User/CLI usage error.
    Usage(String),
    /// I/O failure optionally tied to a path.
    Io {
        message: String,
        path: Option<PathBuf>,
        source: Option<io::Error>,
    },
    /// Generic runtime failure.
    Runtime(String),
}

impl ZexError {
    pub fn usage(msg: impl Into<String>) -> Self {
        Self::Usage(msg.into())
    }

    pub fn runtime(msg: impl Into<String>) -> Self {
        Self::Runtime(msg.into())
    }

    pub fn io(msg: impl Into<String>, path: Option<PathBuf>, source: Option<io::Error>) -> Self {
        Self::Io {
            message: msg.into(),
            path,
            source,
        }
    }

    pub fn from_io(path: impl Into<PathBuf>, err: io::Error) -> Self {
        let path = path.into();
        Self::Io {
            message: err.to_string(),
            path: Some(path),
            source: Some(err),
        }
    }

    /// Map to process exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => EXIT_USAGE,
            Self::Io { .. } | Self::Runtime(_) => EXIT_FAILURE,
        }
    }
}

impl fmt::Display for ZexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(m) => write!(f, "{m}"),
            Self::Runtime(m) => write!(f, "{m}"),
            Self::Io {
                message,
                path,
                source,
            } => {
                if let Some(p) = path {
                    write!(f, "{}: {message}", p.display())?;
                } else {
                    write!(f, "{message}")?;
                }
                if let Some(src) = source {
                    // Avoid duplicating if message already holds the io text.
                    let s = src.to_string();
                    if !message.contains(&s) {
                        write!(f, ": {s}")?;
                    }
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ZexError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io {
                source: Some(s), ..
            } => Some(s),
            _ => None,
        }
    }
}

impl From<io::Error> for ZexError {
    fn from(err: io::Error) -> Self {
        Self::Io {
            message: err.to_string(),
            path: None,
            source: Some(err),
        }
    }
}
