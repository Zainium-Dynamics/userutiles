//! usercore: shared foundation for ZEX utils (Zainium OS coreutils replacement).

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

#[cfg(not(target_os = "linux"))]
compile_error!("usercore targets Zainium OS only.");

pub mod args;
pub mod blkpg;
pub mod blkprobe;
pub mod digest;
pub mod error;
pub mod exit;
pub mod nss;
pub mod pathx;
pub mod posix_regex;
pub mod protect;
pub mod ptable;
pub mod ui;
pub mod zainium;

pub use error::{ZexError, ZexResult};
pub use exit::{EXIT_FAILURE, EXIT_SUCCESS, EXIT_USAGE};
pub use ui::Ui;
pub use zainium::{DEFAULT_PATH, INSTALL_PREFIX, SYSHUB_BIN, SYSHUB_SBIN};
