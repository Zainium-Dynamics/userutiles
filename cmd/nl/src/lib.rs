//! user nl — number lines of files.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `nl` utility. Parses `std::env::args()` as
/// `[OPTION]... [FILE]...` (reading stdin if none are given) and writes
/// each FILE to stdout with line numbers added to non-empty lines.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("nl");
    let mut body_num = true;
    let mut width = 6usize;
    let mut sep = "\t".to_string();
    let mut start = 1i64;
    let mut incr = 1i64;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: nl [OPTION]... [FILE]...\nWrite each FILE to standard output, with line numbers added.\n -b a number all lines\n -b t number only nonempty lines (default)\n -n ln|rn|rz number format\n -w N number width (default 6)\n -s SEP separator (default TAB)\n -v N first line number\n -i N line number increment\n");
                return 0;
            }
            "--version" => {
                println!("nl (user_utils) 0.1.0");
                return 0;
            }
            "-ba" => body_num = true, // all — handled simply as number all nonempty + empty with a
            "-bt" => body_num = true,
            s if s.starts_with("-w") => match s[2..].parse::<usize>() {
                Ok(n) if n > 0 => width = n,
                _ => {
                    ui.err(&format!("invalid line number field width: '{}'", &s[2..]));
                    return 1;
                }
            },
            s if s.starts_with("-s") => sep = s[2..].to_string(),
            s if s.starts_with("-v") => match s[2..].parse::<i64>() {
                Ok(n) => start = n,
                Err(_) => {
                    ui.err(&format!("invalid starting line number: '{}'", &s[2..]));
                    return 1;
                }
            },
            s if s.starts_with("-i") => match s[2..].parse::<i64>() {
                Ok(n) => incr = n,
                Err(_) => {
                    ui.err(&format!("invalid line number increment: '{}'", &s[2..]));
                    return 1;
                }
            },
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }
    let mut out = io::stdout().lock();
    for f in files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            match File::open(&f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    return 1;
                }
            }
        };
        let lines: io::Result<Vec<String>> = reader.lines().collect();
        let lines = match lines {
            Ok(l) => l,
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                return 1;
            }
        };
        if let Err(e) = number_lines(&lines, body_num, width, &sep, start, incr, &mut out) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                ui.err(&format!("{e}"));
                return 1;
            }
            return 0;
        }
    }
    0
}

/// Write `lines` to `out`, prefixing each non-empty line with a
/// right-aligned line number (padded to `width`, followed by `sep`),
/// starting at `start` and increasing by `incr` after each numbered line.
/// When `body_num` is false, empty lines are still written but never
/// numbered; when true, empty lines get a blank (unnumbered) number field
/// for column alignment, matching GNU `nl`'s default "-t" behaviour.
fn number_lines(
    lines: &[String],
    body_num: bool,
    width: usize,
    sep: &str,
    start: i64,
    incr: i64,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut num = start;
    for line in lines {
        if line.is_empty() {
            if body_num {
                writeln!(out, "{:>width$}{sep}", "", width = width)?;
            } else {
                writeln!(out)?;
            }
        } else {
            writeln!(out, "{num:>width$}{sep}{line}", width = width)?;
            num += incr;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_numbering(lines: &[&str], body_num: bool, width: usize, sep: &str) -> String {
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        number_lines(&owned, body_num, width, sep, 1, 1, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn numbers_nonempty_lines_sequentially() {
        let out = run_numbering(&["a", "b", "c"], true, 2, "\t");
        assert_eq!(out, " 1\ta\n 2\tb\n 3\tc\n");
    }

    #[test]
    fn empty_lines_get_blank_field_when_body_num_true() {
        let out = run_numbering(&["a", "", "b"], true, 2, "\t");
        assert_eq!(out, " 1\ta\n  \t\n 2\tb\n");
    }

    #[test]
    fn empty_lines_are_unnumbered_and_bare_when_body_num_false() {
        let out = run_numbering(&["a", "", "b"], false, 2, "\t");
        assert_eq!(out, " 1\ta\n\n 2\tb\n");
    }

    #[test]
    fn empty_input_produces_no_output() {
        let out = run_numbering(&[], true, 6, "\t");
        assert_eq!(out, "");
    }

    #[test]
    fn custom_start_and_increment() {
        let owned: Vec<String> = vec!["x".into(), "y".into()];
        let mut out = Vec::new();
        number_lines(&owned, true, 3, ": ", 10, 5, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), " 10: x\n 15: y\n");
    }

    #[test]
    fn negative_increment_counts_down() {
        let owned: Vec<String> = vec!["x".into(), "y".into(), "z".into()];
        let mut out = Vec::new();
        number_lines(&owned, true, 2, "\t", 3, -1, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), " 3\tx\n 2\ty\n 1\tz\n");
    }
}
