//! user cmp — compare two files byte by byte.
//!
//! Thin wrapper: all argument parsing and comparison logic lives in the
//! vendored `user_diffutils` crate.

/// Entry point for the `cmp` utility. Delegates entirely to
/// `user_diffutils::run_cmp`, which reads `std::env::args()` itself.
///
/// Returns the process exit code (0 on files-equal / success, non-zero on
/// difference or error), matching GNU `cmp` conventions.
pub fn run() -> i32 {
    user_diffutils::run_cmp()
}
