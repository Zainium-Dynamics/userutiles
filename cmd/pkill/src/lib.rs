//! user pkill: signal processes by name, ptty, uid, and other attributes.

/// Entry point for the `pkill` utility. Parses `std::env::args()` (handled
/// by [`user_pgrep::run_as_pkill`]) and sends a signal (default `SIGTERM`) to
/// every process matching the given criteria.
///
/// Returns 0 if at least one process was signaled, 1 if none matched, or 2
/// on a usage error: see `user_pgrep` for the exact matching and signaling
/// behavior.
pub fn run() -> i32 {
    user_pgrep::run_as_pkill()
}
