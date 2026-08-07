//! user tac — write each file to stdout, last line first.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `tac` utility. Parses `std::env::args()` and writes
/// each file's lines to stdout in reverse order (last line first).
///
/// Returns 0 on success, 1 if a file could not be opened or read.
pub fn run() -> i32 {
    let ui = Ui::new("tac");
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("tac (user_utils) 0.1.0");
                return 0;
            }
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
    let mut status = 0;
    for f in files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            match File::open(&f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    status = 1;
                    continue;
                }
            }
        };
        match read_lines_reversed(reader) {
            Ok(lines) => {
                for line in lines {
                    if let Err(e) = out.write_all(&line) {
                        if e.kind() == io::ErrorKind::BrokenPipe {
                            return 0;
                        }
                        ui.err(&format!("{f}: {e}"));
                        return 1;
                    }
                }
            }
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }
    status
}

fn print_help() {
    print!(
        "Usage: tac [OPTION]... [FILE]...\n\
Write each FILE to standard output, last line first.\n\n\
      --help      display this help and exit\n\
      --version   output version information and exit\n"
    );
}

/// Read every line (bytes up to and including the trailing `\n`, if any)
/// from `reader` and return them in reverse order. Operates on raw bytes
/// rather than `BufRead::lines()` so it neither assumes UTF-8 content nor
/// silently drops lines that fail UTF-8 validation.
fn read_lines_reversed(mut reader: Box<dyn BufRead>) -> io::Result<Vec<Vec<u8>>> {
    let mut lines = Vec::new();
    loop {
        let mut buf = Vec::new();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        lines.push(buf);
    }
    lines.reverse();
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reversed(input: &[u8]) -> Vec<Vec<u8>> {
        let reader: Box<dyn BufRead> = Box::new(io::Cursor::new(input.to_vec()));
        read_lines_reversed(reader).unwrap()
    }

    #[test]
    fn reverses_simple_lines() {
        let lines = reversed(b"a\nb\nc\n");
        assert_eq!(
            lines,
            vec![b"c\n".to_vec(), b"b\n".to_vec(), b"a\n".to_vec()]
        );
    }

    #[test]
    fn preserves_missing_trailing_newline() {
        let lines = reversed(b"a\nb\nc");
        assert_eq!(lines, vec![b"c".to_vec(), b"b\n".to_vec(), b"a\n".to_vec()]);
    }

    #[test]
    fn empty_input_yields_no_lines() {
        assert!(reversed(b"").is_empty());
    }

    #[test]
    fn handles_non_utf8_bytes_without_dropping_content() {
        // Regression: the previous `BufRead::lines()` + `unwrap_or_default()`
        // implementation silently replaced any line that failed UTF-8
        // validation with an empty string.
        let input = [b'a', 0xff, 0xfe, b'\n', b'b', b'\n'];
        let lines = reversed(&input);
        assert_eq!(lines, vec![b"b\n".to_vec(), vec![b'a', 0xff, 0xfe, b'\n']]);
    }
}
