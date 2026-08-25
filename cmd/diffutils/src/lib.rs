//! ZEX diffutils library — based on uutils/diffutils 0.5.0 (MIT/Apache-2.0).
//! Provides `diff` and `cmp` implementations without uucore.

// Vendored, not hand-maintained for lint cleanliness — see scripts/clippy.sh.
// cmd/diff and cmd/cmp path-depend on this crate, so it still gets linted.
#![allow(clippy::incompatible_msrv)]

pub mod cmp;
pub mod context_diff;
pub mod diff;
pub mod ed_diff;
pub mod macros;
pub mod normal_diff;
pub mod params;
pub mod side_diff;
pub mod unified_diff;
pub mod utils;

use std::env;
use std::process::ExitCode;

/// Multicall entry for `diff`.
pub fn run_diff() -> i32 {
    let args = env::args_os().peekable();
    diff::main(args)
}

/// Multicall entry for `cmp`.
pub fn run_cmp() -> i32 {
    let args = env::args_os().peekable();
    cmp::main(args)
}

/// Convenience for tests / ExitCode callers.
pub fn run_diff_exit() -> ExitCode {
    ExitCode::from(run_diff() as u8)
}
