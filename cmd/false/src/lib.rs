//! user false — always fail.

/// Entry point for the `false` utility. `--help` and `--version` are
/// honored (printing to stdout and exiting 0); any other argument, or no
/// argument at all, is ignored.
///
/// Always returns 1, except for `--help`/`--version` which return 0.
pub fn run() -> i32 {
    let first = std::env::args_os()
        .nth(1)
        .map(|a| a.to_string_lossy().into_owned());
    dispatch(first.as_deref())
}

/// Decide the exit code and any banner text for the first CLI argument
/// (or `None` if there wasn't one). Pulled out of [`run`] so the dispatch
/// logic can be exercised without touching real process arguments.
fn dispatch(first_arg: Option<&str>) -> i32 {
    match first_arg {
        Some("--help") => {
            print!("Usage: false [ignored command line arguments]\nor: false OPTION\nExit with a status code indicating failure.\n\n --help display this help and exit\n --version output version information and exit\n");
            0
        }
        Some("--version") => {
            println!("false (user_utils) 0.1.0");
            0
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_no_args_fails() {
        assert_eq!(dispatch(None), 1);
    }

    #[test]
    fn dispatch_ignores_unrecognized_args() {
        assert_eq!(dispatch(Some("anything")), 1);
        assert_eq!(dispatch(Some("-x")), 1);
    }

    #[test]
    fn dispatch_help_succeeds() {
        assert_eq!(dispatch(Some("--help")), 0);
    }

    #[test]
    fn dispatch_version_succeeds() {
        assert_eq!(dispatch(Some("--version")), 0);
    }
}
