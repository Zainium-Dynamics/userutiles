// error.rs — Unified XmvError type.
// Every sub-module returns Result<T, XmvError>. No unwrap() anywhere.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XmvError {
    #[error("I/O error on '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Source does not exist: '{0}'")]
    SourceNotFound(PathBuf),

    #[error("Destination already exists (--no-clobber): '{0}'")]
    NoClobber(PathBuf),

    #[error("Recursive flag required to move directory: '{0}'")]
    IsDirectory(PathBuf),

    #[error("renameat2(RENAME_EXCHANGE) failed on '{0}': files are not on the same filesystem")]
    ExchangeCrossDevice(PathBuf),

    #[error("renameat2 not supported on this kernel (requires Linux ≥ 3.15)")]
    Renameat2Unsupported,

    #[error("Checksum mismatch after cross-device copy for '{path}': src={src_hash:x} dest={dest_hash:x}")]
    ChecksumMismatch {
        path: PathBuf,
        src_hash: u128,
        dest_hash: u128,
    },

    #[error("Undo journal I/O error: {0}")]
    Journal(String),

    #[error("Trash directory error: {0}")]
    Trash(String),

    #[error("Unsupported platform — xmv requires Linux or Redox OS")]
    UnsupportedPlatform,

    #[error("Thread error")]
    ThreadJoin,
}

/// Convenience: wrap std::io::Error with a path context.
pub fn io_err(path: impl Into<PathBuf>, source: std::io::Error) -> XmvError {
    XmvError::Io {
        path: path.into(),
        source,
    }
}

pub type Result<T> = std::result::Result<T, XmvError>;
