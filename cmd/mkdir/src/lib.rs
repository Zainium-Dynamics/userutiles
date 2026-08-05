//! user mkdir — guidance stub; Zainium uses `struct` instead of GNU mkdir.
//!
//! This is intentionally *not* a real `mkdir` implementation. Per this
//! workspace's "Zainium naming" convention (see the top-level README),
//! directory/file creation on Zainium OS goes through the in-house
//! `struct` tool (which subsumes both `mkdir -p` and `touch` workflows,
//! with unambiguous create-dir-vs-create-file rules), not GNU `mkdir`.
//! This `mkdir` stub exists only so that muscle-memory invocations of `mkdir`
//! fail loudly with guidance toward `struct`, rather than silently doing
//! nothing or not existing at all.

/// Entry point for the `mkdir` stub. Ignores all arguments (including
/// `--help`) and unconditionally prints guidance pointing the user at
/// `struct`, the Zainium-native replacement for `mkdir -p`/`touch`.
///
/// Always returns 2 (GNU `mkdir`'s usage-error exit code), since this
/// binary can never actually satisfy the request it was invoked for.
pub fn run() -> i32 {
    eprintln!("mkdir: on Zainium OS use `struct` instead of `mkdir`.");
    eprintln!();
    eprintln!(" struct replaces both mkdir -p and touch workflows:");
    eprintln!(" struct src/services/auth # create directory tree (like mkdir -p)");
    eprintln!(" struct path/to/file.txt # create file (path contains '.')");
    eprintln!(" struct -t path/name # force file even without '.'");
    eprintln!();
    eprintln!(" Run `struct --help` for full usage.");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_returns_usage_error_exit_code() {
        // Guidance is written to stderr via eprintln!, which this test
        // doesn't capture; it only asserts the documented exit code.
        assert_eq!(run(), 2);
    }
}
