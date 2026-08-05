//! user join — join lines of two files on a common field.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `join` utility. Parses `std::env::args()` and
/// prints, for each pair of lines from FILE1 and FILE2 whose join fields
/// (`-1`/`-2`/`-j`, default field 1) are equal, a combined line: the join
/// field followed by the remaining fields of both lines.
///
/// Uses an O(n·m) nested scan rather than a sorted merge, so unlike GNU
/// `join` it does not require its inputs to be pre-sorted — at the cost
/// of being unsuitable for very large files.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("join");
    let mut field1 = 1usize;
    let mut field2 = 1usize;
    let mut delim = None::<char>;
    let mut files: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: join [OPTION]... FILE1 FILE2\n\
 Join lines of two files on a common field.\n\
 -1 FIELD join on this FIELD of file 1\n\
 -2 FIELD join on this FIELD of file 2\n\
 -t CHAR use CHAR as input and output field separator\n\
 -j FIELD equivalent to '-1 FIELD -2 FIELD'\n\
 Both files should be sorted on the join field.\n"
                );
                return 0;
            }
            "--version" => {
                println!("join (user_utils) 0.1.0");
                return 0;
            }
            "-1" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("");
                let Some(f) = parse_field(spec) else {
                    ui.err(&format!("invalid field number '{spec}'"));
                    return 1;
                };
                field1 = f;
            }
            "-2" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("");
                let Some(f) = parse_field(spec) else {
                    ui.err(&format!("invalid field number '{spec}'"));
                    return 1;
                };
                field2 = f;
            }
            "-j" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("");
                let Some(f) = parse_field(spec) else {
                    ui.err(&format!("invalid field number '{spec}'"));
                    return 1;
                };
                field1 = f;
                field2 = f;
            }
            "-t" => {
                i += 1;
                delim = args.get(i).and_then(|s| s.chars().next());
            }
            s if s.starts_with("-1") && s.len() > 2 => {
                let Some(f) = parse_field(&s[2..]) else {
                    ui.err(&format!("invalid field number '{}'", &s[2..]));
                    return 1;
                };
                field1 = f;
            }
            s if s.starts_with("-2") && s.len() > 2 => {
                let Some(f) = parse_field(&s[2..]) else {
                    ui.err(&format!("invalid field number '{}'", &s[2..]));
                    return 1;
                };
                field2 = f;
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }
    if files.len() != 2 {
        ui.err("two file operands required");
        return 1;
    }

    let a = match read_file(&files[0]) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&format!("{}: {e}", files[0]));
            return 1;
        }
    };
    let b = match read_file(&files[1]) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&format!("{}: {e}", files[1]));
            return 1;
        }
    };

    let sep = delim.unwrap_or(' ');
    let mut out = io::stdout().lock();
    // Nested scan (correct for unsorted small files; works for sorted too)
    for line1 in &a {
        let f1 = field(line1, field1, delim);
        for line2 in &b {
            let f2 = field(line2, field2, delim);
            if f1 == f2 && !f1.is_empty() {
                let rest1 = other_fields(line1, field1, delim);
                let rest2 = other_fields(line2, field2, delim);
                let mut parts = vec![f1.clone()];
                parts.extend(rest1);
                parts.extend(rest2);
                let joined = if delim.is_some() {
                    parts.join(&sep.to_string())
                } else {
                    parts.join(" ")
                };
                let _ = writeln!(out, "{joined}");
            }
        }
    }
    0
}

/// Parse a 1-based field-number argument (for `-1`/`-2`/`-j`). Rejects
/// `0` as well as non-numeric input, since [`field`] indexes with `n - 1`
/// and would otherwise underflow.
fn parse_field(s: &str) -> Option<usize> {
    match s.parse::<usize>() {
        Ok(0) | Err(_) => None,
        Ok(n) => Some(n),
    }
}

/// Read `path` into a `Vec` of lines; `"-"` reads from stdin.
fn read_file(path: &str) -> io::Result<Vec<String>> {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    reader.lines().collect()
}

/// Split `line` into fields: by `delim` if given, else on runs of
/// whitespace (GNU `join`'s default).
fn split_fields(line: &str, delim: Option<char>) -> Vec<&str> {
    match delim {
        Some(d) => line.split(d).collect(),
        None => line.split_whitespace().collect(),
    }
}

/// Return the 1-based `n`th field of `line` (empty string if out of
/// range). Callers must ensure `n >= 1` (see [`parse_field`]).
fn field(line: &str, n: usize, delim: Option<char>) -> String {
    split_fields(line, delim)
        .get(n - 1)
        .copied()
        .unwrap_or("")
        .to_string()
}

/// Return every field of `line` except the 1-based `n`th (the join
/// field), preserving order.
fn other_fields(line: &str, n: usize, delim: Option<char>) -> Vec<String> {
    split_fields(line, delim)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i + 1 != n)
        .map(|(_, s)| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_field_accepts_positive_numbers() {
        assert_eq!(parse_field("1"), Some(1));
        assert_eq!(parse_field("42"), Some(42));
    }

    #[test]
    fn parse_field_rejects_zero_and_garbage() {
        assert_eq!(parse_field("0"), None);
        assert_eq!(parse_field("abc"), None);
        assert_eq!(parse_field(""), None);
        assert_eq!(parse_field("-1"), None);
    }

    #[test]
    fn split_fields_whitespace_default() {
        assert_eq!(split_fields("a  b c", None), vec!["a", "b", "c"]);
    }

    #[test]
    fn split_fields_custom_delim() {
        assert_eq!(split_fields("a:b:c", Some(':')), vec!["a", "b", "c"]);
    }

    #[test]
    fn field_extracts_1_based_index() {
        assert_eq!(field("a b c", 2, None), "b");
        assert_eq!(field("a b c", 1, None), "a");
    }

    #[test]
    fn field_out_of_range_is_empty() {
        assert_eq!(field("a b", 5, None), "");
    }

    #[test]
    fn other_fields_excludes_join_field() {
        assert_eq!(other_fields("a b c", 2, None), vec!["a", "c"]);
    }

    #[test]
    fn read_file_missing_path_errors() {
        let missing = format!("/nonexistent_user_join_test_{}", std::process::id());
        assert!(read_file(&missing).is_err());
    }

    #[test]
    fn read_file_reads_lines() {
        let dir = std::env::temp_dir().join(format!("user_join_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.txt");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let lines = read_file(path.to_str().unwrap()).unwrap();
        assert_eq!(lines, vec!["one", "two", "three"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
