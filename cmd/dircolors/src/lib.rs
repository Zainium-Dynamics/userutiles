//! user dircolors — output commands to set LS_COLORS.
use std::fs;
use std::io::{self, Write};

use usercore::Ui;

const DEFAULT_DB: &str = r#"
# user_utils default dircolors database
NORMAL 00
FILE 00
DIR 01;34
LINK 01;36
FIFO 40;33
SOCK 01;35
DOOR 01;35
BLK 40;33;01
CHR 40;33;01
ORPHAN 40;31;01
EXEC 01;32
*tar 01;31
*tgz 01;31
*zip 01;31
*gz 01;31
*xz 01;31
*zst 01;31
*jpg 01;35
*jpeg 01;35
*png 01;35
*gif 01;35
*mp3 00;36
*mp4 00;36
"#;

/// Entry point for the `dircolors` utility. Parses `std::env::args()` and
/// either prints the built-in database (`-p`) or emits a shell command
/// (`export LS_COLORS=...` for Bourne shells, `setenv LS_COLORS ...` for
/// `-c`/csh) built from `FILE`, or the built-in defaults if no file is
/// given.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("dircolors");
    let mut print_database = false;
    let mut csh = false;
    let mut file: Option<String> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: dircolors [OPTION]... [FILE]\nOutput commands to set LS_COLORS.\n -b, --sh, --bourne-shell output Bourne shell code (default)\n -c, --csh, --c-shell output C shell code\n -p, --print-database print defaults\n");
                return 0;
            }
            "--version" => {
                println!("dircolors (user_utils) 0.1.0");
                return 0;
            }
            "-b" | "--sh" | "--bourne-shell" => csh = false,
            "-c" | "--csh" | "--c-shell" => csh = true,
            "-p" | "--print-database" => print_database = true,
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => file = Some(other.to_string()),
        }
    }
    if print_database {
        print!("{DEFAULT_DB}");
        return 0;
    }
    let db = if let Some(f) = file {
        match fs::read_to_string(&f) {
            Ok(s) => s,
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                return 1;
            }
        }
    } else {
        DEFAULT_DB.to_string()
    };
    let value = parse_db(&db);
    let mut out = io::stdout().lock();
    if csh {
        let _ = writeln!(out, "setenv LS_COLORS '{value}'");
    } else {
        let _ = writeln!(out, "LS_COLORS='{value}'; export LS_COLORS");
    }
    0
}

/// Convert a dircolors-format database (`KEYWORD SGR` / `*.ext=SGR` lines,
/// `#`-comments, blank lines ignored) into a colon-joined `LS_COLORS`
/// value string, e.g. `"di=01;34:*.tar=01;31"`.
///
/// Unrecognized keywords are silently skipped, matching GNU `dircolors`'
/// lenient parsing.
fn parse_db(db: &str) -> String {
    let mut parts = Vec::new();
    for line in db.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let Some(key) = it.next() else { continue };
        let Some(val) = it.next() else { continue };
        let k = match key {
            "NORMAL" | "NORM" => "no",
            "FILE" => "fi",
            "DIR" => "di",
            "LINK" | "LNK" => "ln",
            "FIFO" | "PIPE" => "pi",
            "SOCK" => "so",
            "DOOR" => "do",
            "BLK" => "bd",
            "CHR" => "cd",
            "ORPHAN" => "or",
            "EXEC" => "ex",
            "SETUID" => "su",
            "SETGID" => "sg",
            "STICKY" => "st",
            "OTHER_WRITABLE" => "ow",
            "STICKY_OTHER_WRITABLE" => "tw",
            s if s.starts_with('*') => s,
            _ => continue,
        };
        parts.push(format!("{k}={val}"));
    }
    parts.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_db_maps_known_keywords() {
        let db = "DIR 01;34\nEXEC 01;32\n";
        assert_eq!(parse_db(db), "di=01;34:ex=01;32");
    }

    #[test]
    fn parse_db_passes_through_glob_entries() {
        // Input db lines are whitespace-separated (pattern, then SGR
        // code); the joined LS_COLORS value re-quotes them with '='.
        let db = "*tar 01;31\n*jpg 01;35\n";
        assert_eq!(parse_db(db), "*tar=01;31:*jpg=01;35");
    }

    #[test]
    fn parse_db_skips_comments_blank_lines_and_unknown_keywords() {
        let db = "# comment\n\nBOGUS 00\nDIR 01;34\n";
        assert_eq!(parse_db(db), "di=01;34");
    }

    #[test]
    fn parse_db_empty_input_yields_empty_string() {
        assert_eq!(parse_db(""), "");
    }

    #[test]
    fn parse_db_default_database_is_nonempty() {
        let value = parse_db(DEFAULT_DB);
        assert!(value.contains("di=01;34"));
        assert!(value.contains("*tar=01;31"));
    }
}
