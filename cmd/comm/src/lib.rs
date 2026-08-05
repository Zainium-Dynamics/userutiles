//! user comm — compare two sorted files line by line.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

use usercore::Ui;

/// Entry point for the `comm` utility. Parses `std::env::args()`, reads the
/// two file operands, and prints their line-by-line comparison in three
/// tab-indented columns (unique to file 1, unique to file 2, common to
/// both), matching GNU `comm` conventions.
///
/// Both inputs are assumed to already be sorted; `comm` does not sort them.
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("comm");
    let mut suppress = [false; 3];
    let mut zero = false;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: comm [OPTION]... FILE1 FILE2\n\
 Compare two sorted files line by line.\n\
 -1 suppress column 1 (lines unique to FILE1)\n\
 -2 suppress column 2 (lines unique to FILE2)\n\
 -3 suppress column 3 (lines common)\n\
 -z line delimiter is NUL\n"
                );
                return 0;
            }
            "--version" => {
                println!("comm (user_utils) 0.1.0");
                return 0;
            }
            "-1" => suppress[0] = true,
            "-2" => suppress[1] = true,
            "-3" => suppress[2] = true,
            "-z" | "--zero-terminated" => zero = true,
            s if s.starts_with('-') && s != "-" => {
                for c in s.chars().skip(1) {
                    match c {
                        '1' => suppress[0] = true,
                        '2' => suppress[1] = true,
                        '3' => suppress[2] = true,
                        'z' => zero = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            other => files.push(other.to_string()),
        }
    }
    if files.len() != 2 {
        ui.err("two file operands required");
        return 1;
    }
    let a = match read_lines(&files[0], zero) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&format!("{}: {e}", files[0]));
            return 1;
        }
    };
    let b = match read_lines(&files[1], zero) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&format!("{}: {e}", files[1]));
            return 1;
        }
    };
    let mut out = io::stdout().lock();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < a.len() || j < b.len() {
        if j >= b.len() || (i < a.len() && a[i] < b[j]) {
            emit(&mut out, &a[i], 0, &suppress);
            i += 1;
        } else if i >= a.len() || a[i] > b[j] {
            emit(&mut out, &b[j], 1, &suppress);
            j += 1;
        } else {
            emit(&mut out, &a[i], 2, &suppress);
            i += 1;
            j += 1;
        }
    }
    0
}

/// Read `path` (or stdin if `path == "-"`) as a sequence of lines.
///
/// When `zero` is set, lines are split on NUL bytes instead of `\n` and
/// empty segments are dropped (matching GNU `comm -z`'s treatment of a
/// trailing delimiter).
fn read_lines(path: &str, zero: bool) -> io::Result<Vec<String>> {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    let mut lines = Vec::new();
    if zero {
        let mut data = Vec::new();
        let mut r = reader;
        r.read_to_end(&mut data)?;
        for part in data.split(|b| *b == 0) {
            if part.is_empty() {
                continue;
            }
            lines.push(String::from_utf8_lossy(part).into_owned());
        }
    } else {
        for line in reader.lines() {
            lines.push(line?);
        }
    }
    Ok(lines)
}

/// Write one output line in the appropriate column (0, 1, or 2), prefixed
/// with the right number of tabs, or do nothing if that column is
/// suppressed.
fn emit(out: &mut impl Write, line: &str, col: usize, suppress: &[bool; 3]) {
    if suppress[col] {
        return;
    }
    let mut prefix = String::new();
    for c in 0..col {
        if !suppress[c] {
            prefix.push('\t');
        }
    }
    let _ = writeln!(out, "{prefix}{line}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_file(tag: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "user_comm_test_{tag}_{}_{}",
            std::process::id(),
            tag
        ));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn read_lines_splits_on_newline() {
        let f = tmp_file("nl", "a\nb\nc\n");
        let lines = read_lines(f.to_str().unwrap(), false).unwrap();
        assert_eq!(lines, vec!["a", "b", "c"]);
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn read_lines_splits_on_nul_when_zero_terminated() {
        let f = tmp_file("z", "a\0b\0c\0");
        let lines = read_lines(f.to_str().unwrap(), true).unwrap();
        assert_eq!(lines, vec!["a", "b", "c"]);
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn read_lines_missing_file_errors() {
        let missing = std::env::temp_dir().join(format!(
            "user_comm_test_missing_{}_does_not_exist",
            std::process::id()
        ));
        let result = read_lines(missing.to_str().unwrap(), false);
        assert!(result.is_err());
    }

    #[test]
    fn emit_respects_suppression() {
        let mut buf: Vec<u8> = Vec::new();
        emit(&mut buf, "line", 0, &[false, false, false]);
        assert_eq!(String::from_utf8(buf).unwrap(), "line\n");

        let mut buf: Vec<u8> = Vec::new();
        emit(&mut buf, "line", 0, &[true, false, false]);
        assert!(buf.is_empty());
    }

    #[test]
    fn emit_prefixes_tabs_for_column() {
        let mut buf: Vec<u8> = Vec::new();
        emit(&mut buf, "common", 2, &[false, false, false]);
        assert_eq!(String::from_utf8(buf).unwrap(), "\t\tcommon\n");
    }
}
