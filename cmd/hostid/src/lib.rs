//! user hostid — print the numeric identifier for the current host.

use usercore::Ui;

/// What to do based on the (at most one meaningful) CLI argument to
/// `hostid`.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// Print the help banner and exit 0.
    Help,
    /// Print the version banner and exit 0.
    Version,
    /// Print the host ID (the normal, no-argument case).
    PrintId,
    /// Argument wasn't recognized; report it and exit 1.
    Invalid(String),
}

/// Classify the first CLI argument (`None` if there wasn't one). `hostid`
/// takes at most one flag, so only `args().nth(1)` is ever meaningful.
fn classify_arg(arg: Option<&str>) -> Action {
    match arg {
        None => Action::PrintId,
        Some("-h") | Some("--help") => Action::Help,
        Some("--version") => Action::Version,
        Some(other) => Action::Invalid(other.to_string()),
    }
}

/// Entry point for the `hostid` utility. Parses `std::env::args()` and
/// prints the host's numeric identifier (from `gethostid(3)`) as 8 lowercase
/// hex digits.
///
/// Returns 0 on success (including `--help`/`--version`), 1 on a usage
/// error.
pub fn run() -> i32 {
    let ui = Ui::new("hostid");
    let first = std::env::args().nth(1);
    match classify_arg(first.as_deref()) {
        Action::Help => {
            print!("Usage: hostid\nPrint the numeric identifier for the current host.\n");
            0
        }
        Action::Version => {
            println!("hostid (user_utils) 0.1.0");
            0
        }
        Action::Invalid(other) => {
            ui.err(&format!("invalid option -- '{other}'"));
            1
        }
        Action::PrintId => {
            // SAFETY: `gethostid` takes no arguments and simply reads a
            // host-identifier value (from `/etc/hostid` or a
            // kernel-provided fallback); it cannot fail or cause undefined
            // behavior regardless of process state.
            let id = unsafe { libc::gethostid() } as u32;
            println!("{id:08x}");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_arg_none_prints_id() {
        assert_eq!(classify_arg(None), Action::PrintId);
    }

    #[test]
    fn classify_arg_help_variants() {
        assert_eq!(classify_arg(Some("-h")), Action::Help);
        assert_eq!(classify_arg(Some("--help")), Action::Help);
    }

    #[test]
    fn classify_arg_version() {
        assert_eq!(classify_arg(Some("--version")), Action::Version);
    }

    #[test]
    fn classify_arg_unknown_is_invalid() {
        assert_eq!(
            classify_arg(Some("--bogus")),
            Action::Invalid("--bogus".to_string())
        );
    }
}
