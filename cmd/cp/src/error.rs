// error.rs — Unified CpError type.
// Every sub-module returns Result<T, CpError>. No unwrap() anywhere.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CpError {
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

    #[error("-r not specified; omitting directory '{0}'")]
    IsDirectory(PathBuf),

    #[error("target '{0}' is not a directory")]
    NotADirectory(PathBuf),

    #[error("'{0}' and '{1}' are the same file")]
    SameFile(PathBuf, PathBuf),

    #[error("possible symlink loop detected at '{0}'")]
    SymlinkLoop(PathBuf),

    #[error("reflink requested (--reflink=always) but not supported for '{0}'")]
    ReflinkUnsupported(PathBuf),

    #[error("Checksum mismatch after copy for '{path}': src={src_hash:x} dest={dest_hash:x}")]
    ChecksumMismatch {
        path: PathBuf,
        src_hash: u128,
        dest_hash: u128,
    },

    #[error("missing file operand")]
    MissingOperand,

    #[error("Thread pool error")]
    ThreadJoin,
}

/// Convenience: wrap std::io::Error with a path context.
pub fn io_err(path: impl Into<PathBuf>, source: std::io::Error) -> CpError {
    CpError::Io {
        path: path.into(),
        source,
    }
}

pub type Result<T> = std::result::Result<T, CpError>;
