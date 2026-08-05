use std::fmt;

#[derive(Debug)]
pub enum TriggerError {
    AppNotFound {
        app: String,
        suggestions: Vec<String>,
    },
    FileNotFound {
        path: String,
    },
    #[allow(dead_code)]
    PermissionDenied {
        target: String,
    },
    ExecutionFailed {
        target: String,
        reason: String,
    },
    ConfigError {
        reason: String,
    },
    RootExecutionForbidden {
        app: String,
        command: String,
    },
    IoError {
        reason: String,
    },
    Utf8Error {
        reason: String,
    },
}

impl fmt::Display for TriggerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TriggerError::AppNotFound { app, suggestions } => {
                writeln!(f, "  ✗ Application '{}' not found.", app)?;
                if !suggestions.is_empty() {
                    writeln!(f, "\n Smart suggestions:")?;
                    for sug in suggestions {
                        writeln!(f, "   • Did you mean: {} ?", sug)?;
                    }
                }
                Ok(())
            }
            TriggerError::FileNotFound { path } => {
                write!(f, " ✗ File '{}' not found.", path)
            }
            TriggerError::PermissionDenied { target } => {
                write!(f, " ✗ Permission denied: Cannot execute '{}'.", target)
            }
            TriggerError::ExecutionFailed { target, reason } => {
                write!(f, "Error: Failed to execute '{}': {}", target, reason)
            }
            TriggerError::ConfigError { reason } => {
                write!(f, "Configuration error: {}", reason)
            }
            TriggerError::RootExecutionForbidden { app, command } => {
                write!(f, "Error: Cannot safely run GUI app '{}' as root without proper flags.\n\nSuggestion: {}", app, command)
            }
            TriggerError::IoError { reason } => {
                write!(f, "I/O Error: {}", reason)
            }
            TriggerError::Utf8Error { reason } => {
                write!(f, "UTF-8 Error: {}", reason)
            }
        }
    }
}

impl std::error::Error for TriggerError {}

impl From<std::io::Error> for TriggerError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError {
            reason: value.to_string(),
        }
    }
}

impl From<std::string::FromUtf8Error> for TriggerError {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Self::Utf8Error {
            reason: value.to_string(),
        }
    }
}

impl TriggerError {
    pub fn exit_code(&self) -> u8 {
        match self {
            TriggerError::AppNotFound { .. } => 2,
            TriggerError::FileNotFound { .. } => 3,
            TriggerError::PermissionDenied { .. } => 4,
            TriggerError::ExecutionFailed { .. } => 5,
            TriggerError::ConfigError { .. } => 6,
            TriggerError::RootExecutionForbidden { .. } => 7,
            TriggerError::IoError { .. } => 8,
            TriggerError::Utf8Error { .. } => 9,
        }
    }
}

pub type Result<T> = std::result::Result<T, TriggerError>;
