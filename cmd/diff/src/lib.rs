//! user diff: compare files line by line.

/// Entry point for the `diff` utility. Delegates entirely to
/// `user_diffutils::run_diff`, which reads `std::env::args()` itself.
///
/// Returns the process exit code (0 if inputs are identical, 1 if they
/// differ, 2 on error), matching GNU `diff` conventions.
pub fn run() -> i32 {
    user_diffutils::run_diff()
}
