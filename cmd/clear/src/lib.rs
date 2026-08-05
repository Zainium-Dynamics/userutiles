//! user clear — clear the terminal screen.
use std::io::{self, Write};

use usercore::Ui;

/// What `clear` should do, as decided by `parse_action` from
/// `std::env::args()`.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// Clear the screen and scrollback (default, no arguments).
    Full,
    /// Clear the visible screen only, keep scrollback (`-x`).
    NoScrollback,
    /// Print the help text and exit 0.
    Help,
    /// Print the version string and exit 0.
    Version,
    /// Unknown option; the `String` is the offending argument.
    Invalid(String),
}

/// Entry point for the `clear` utility. Parses `std::env::args()` and
/// writes the corresponding ANSI escape sequence to standard output.
///
/// Returns 0 on success, 1 if given an unrecognized option.
pub fn run() -> i32 {
    let ui = Ui::new("clear");
    match parse_action(&mut std::env::args().skip(1)) {
        Action::Help => {
            print!("Usage: clear [OPTION]\nClear the terminal screen.\n\n -x do not clear scrollback\n -h, --help display this help and exit\n --version output version information and exit\n");
            0
        }
        Action::Version => {
            println!("clear (user_utils) 0.1.0");
            0
        }
        Action::NoScrollback => {
            let _ = io::stdout().write_all(b"\x1b[H\x1b[2J");
            let _ = io::stdout().flush();
            0
        }
        Action::Full => {
            let _ = io::stdout().write_all(b"\x1b[H\x1b[2J\x1b[3J");
            let _ = io::stdout().flush();
            0
        }
        Action::Invalid(arg) => {
            ui.err(&format!("invalid option -- '{arg}'"));
            1
        }
    }
}

/// Decide the `Action` from the first CLI argument (if any). `clear`
/// takes no positional operands, only an optional single flag, so only
/// the first item of `args` is ever consulted.
fn parse_action(args: &mut impl Iterator<Item = String>) -> Action {
    match args.next() {
        None => Action::Full,
        Some(arg) => match arg.as_str() {
            "-h" | "--help" => Action::Help,
            "--version" => Action::Version,
            "-x" => Action::NoScrollback,
            other => Action::Invalid(other.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Action {
        parse_action(&mut args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_args_is_full_clear() {
        assert_eq!(parse(&[]), Action::Full);
    }

    #[test]
    fn help_flags() {
        assert_eq!(parse(&["-h"]), Action::Help);
        assert_eq!(parse(&["--help"]), Action::Help);
    }

    #[test]
    fn version_flag() {
        assert_eq!(parse(&["--version"]), Action::Version);
    }

    #[test]
    fn no_scrollback_flag() {
        assert_eq!(parse(&["-x"]), Action::NoScrollback);
    }

    #[test]
    fn unknown_flag_is_invalid() {
        assert_eq!(parse(&["--bogus"]), Action::Invalid("--bogus".to_string()));
    }

    #[test]
    fn only_first_argument_is_consulted() {
        // Matches the original behavior: `clear -x --help` clears without
        // scrollback and never looks at `--help`.
        assert_eq!(parse(&["-x", "--help"]), Action::NoScrollback);
    }
}
