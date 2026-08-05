//! user readlink — print value of a symbolic link or canonical path.
use std::fs;
use std::path::PathBuf;

use usercore::Ui;

/// Entry point for the `readlink` utility. Parses `std::env::args()` and,
/// for each `FILE`, either prints the target of the symbolic link `FILE`
/// (the default) or its canonicalized path (`-f`/`-e`/`-m`).
///
/// Returns 0 on success, 1 if any `FILE` could not be read (and `-q` was
/// not given to suppress the error), or on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("readlink");
    let mut canonicalize = false;
    let mut zero = false;
    let mut no_newline = false;
    let mut quiet = false;
    let mut paths: Vec<PathBuf> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: readlink [OPTION]... FILE...\n\
 -f, --canonicalize canonicalize by following every symlink\n\
 -e, --canonicalize-existing all components must exist\n\
 -m, --canonicalize-missing no path components need exist\n\
 -n, --no-newline do not output the trailing delimiter\n\
 -q, --quiet, --silent suppress most error messages\n\
 -z, --zero end each output line with NUL\n"
                );
                return 0;
            }
            "--version" => {
                println!("readlink (user_utils) 0.1.0");
                return 0;
            }
            "-f"
            | "--canonicalize"
            | "-e"
            | "--canonicalize-existing"
            | "-m"
            | "--canonicalize-missing" => canonicalize = true,
            "-n" | "--no-newline" => no_newline = true,
            "-q" | "--quiet" | "--silent" | "-s" => quiet = true,
            "-z" | "--zero" => zero = true,
            s if s.starts_with('-') && s != "-" => {
                for c in s.chars().skip(1) {
                    match c {
                        'f' | 'e' | 'm' => canonicalize = true,
                        'n' => no_newline = true,
                        'q' | 's' => quiet = true,
                        'z' => zero = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            other => paths.push(PathBuf::from(other)),
        }
    }
    if paths.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let end = if zero {
        "\0"
    } else if no_newline && paths.len() == 1 {
        ""
    } else {
        "\n"
    };
    let mut status = 0;
    for p in &paths {
        let res = if canonicalize {
            fs::canonicalize(p)
        } else {
            fs::read_link(p)
        };
        match res {
            Ok(t) => print!("{}{end}", t.display()),
            Err(e) => {
                if !quiet {
                    ui.err(&format!("{}: {e}", p.display()));
                }
                status = 1;
            }
        }
    }
    status
}

// `run()`'s behavior (symlink resolution, `-f`/-`e`/`-m` canonicalization,
// `-n`/`-z` delimiter selection, `-q` error suppression) is exercised
// end-to-end against the real filesystem by the integration tests in
// `tests/cli.rs`.
