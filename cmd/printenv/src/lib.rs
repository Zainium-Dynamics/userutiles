//! user printenv — print all or part of environment.
use std::env;

use usercore::Ui;

/// Entry point for the `printenv` utility. Parses `std::env::args()` and
/// prints the value of each named `VARIABLE`, or the entire environment
/// (as `NAME=value` pairs) if none are given.
///
/// Returns 0 on success, 1 if any requested variable is unset (matching
/// GNU `printenv`), or on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("printenv");
    let mut zero = false;
    let mut names: Vec<String> = Vec::new();
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: printenv [OPTION]... [VARIABLE]...\n -0, --null end each output line with NUL\n");
                return 0;
            }
            "--version" => {
                println!("printenv (user_utils) 0.1.0");
                return 0;
            }
            "-0" | "--null" => zero = true,
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => names.push(other.to_string()),
        }
    }
    let end = if zero { "\0" } else { "\n" };
    if names.is_empty() {
        for (k, v) in env::vars() {
            print!("{k}={v}{end}");
        }
        return 0;
    }
    let mut status = 0;
    for n in names {
        match env::var(&n) {
            Ok(v) => print!("{v}{end}"),
            Err(_) => status = 1,
        }
    }
    status
}

// `run()` reads `std::env::args()` and the real process environment, so
// CLI-level behavior (missing var -> exit 1, `-0` NUL separator, invalid
// option, etc.) is covered by the integration tests in `tests/cli.rs`
// instead of unit tests here.
