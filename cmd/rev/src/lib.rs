//! user rev — reverse lines characterwise.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

use usercore::Ui;

/// Reverse a single line's bytes for output.
///
/// If `line` (excluding the trailing separator) is valid UTF-8, the line is
/// reversed by Unicode scalar value (`char`) so multi-byte sequences stay
/// intact. Otherwise it falls back to a raw byte-wise reversal so binary
/// input round-trips without panicking or corrupting unrelated bytes.
pub fn reverse_line(line: &[u8]) -> Vec<u8> {
    match std::str::from_utf8(line) {
        Ok(s) => s.chars().rev().collect::<String>().into_bytes(),
        Err(_) => line.iter().rev().copied().collect(),
    }
}

/// Read `stream` and write each line (split on `sep`), reversed, to `out`.
///
/// The final unterminated fragment (if any) is reversed and written without
/// a trailing separator, matching classic `rev` behavior on files that lack
/// a final newline.
pub fn rev_stream(stream: impl Read, sep: u8, out: &mut impl Write) -> io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::with_capacity(4096);
    loop {
        buf.clear();
        let n = reader.read_until(sep, &mut buf)?;
        if n == 0 {
            break;
        }
        if buf.last().copied() == Some(sep) {
            buf.pop();
            let mut reversed = reverse_line(&buf);
            reversed.push(sep);
            out.write_all(&reversed)?;
        } else {
            let reversed = reverse_line(&buf);
            out.write_all(&reversed)?;
            break;
        }
    }
    Ok(())
}

const HELP: &str = "Usage: rev [OPTION]... [FILE]...\n\
Reverse lines characterwise.\n\n\
With no FILE, or when FILE is -, read standard input.\n\n\
  -0, --zero     line delimiter is NUL, not newline\n\
  -h, --help     display this help and exit\n\
      --version  output version information and exit\n";

/// Entry point: parse arguments, reverse each input file's lines, print result.
pub fn run() -> i32 {
    let ui = Ui::new("rev");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut zero = false;
    let mut files: Vec<String> = Vec::new();

    for arg in &args {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("rev (user_utils) 0.1.0");
                return 0;
            }
            "-0" | "--zero" => zero = true,
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 2;
            }
            other => files.push(other.to_string()),
        }
    }

    let sep = if zero { b'\0' } else { b'\n' };
    let mut stdout = io::stdout().lock();
    let mut status = 0;

    if files.is_empty() {
        let stdin = io::stdin();
        if let Err(e) = rev_stream(stdin.lock(), sep, &mut stdout) {
            ui.err(&format!("error reading standard input: {e}"));
            status = 1;
        }
    } else {
        for path in &files {
            if path == "-" {
                let stdin = io::stdin();
                if let Err(e) = rev_stream(stdin.lock(), sep, &mut stdout) {
                    ui.err(&format!("error reading standard input: {e}"));
                    status = 1;
                }
                continue;
            }
            match File::open(path) {
                Ok(file) => {
                    if let Err(e) = rev_stream(file, sep, &mut stdout) {
                        ui.err(&format!("cannot read {path}: {e}"));
                        status = 1;
                    }
                }
                Err(e) => {
                    ui.err(&format!("cannot open {path}: {e}"));
                    status = 1;
                }
            }
        }
    }

    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rev_str(s: &str, sep: u8) -> String {
        let mut out = Vec::new();
        rev_stream(s.as_bytes(), sep, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn reverses_simple_line() {
        assert_eq!(rev_str("hello\n", b'\n'), "olleh\n");
    }

    #[test]
    fn reverses_multiple_lines() {
        assert_eq!(rev_str("abc\ndef\n", b'\n'), "cba\nfed\n");
    }

    #[test]
    fn empty_line_stays_empty() {
        assert_eq!(rev_str("\n", b'\n'), "\n");
    }

    #[test]
    fn empty_input_produces_nothing() {
        assert_eq!(rev_str("", b'\n'), "");
    }

    #[test]
    fn no_trailing_newline_is_preserved_as_such() {
        assert_eq!(rev_str("abc", b'\n'), "cba");
    }

    #[test]
    fn unicode_multibyte_chars_reverse_correctly() {
        // "héllo wörld" — should reverse by codepoint, not byte, so
        // multi-byte UTF-8 sequences (é, ö) stay intact rather than
        // becoming invalid byte garbage.
        let input = "héllo wörld\n";
        let out = rev_str(input, b'\n');
        assert_eq!(out, "dlröw olléh\n");
        // Must still be valid UTF-8.
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn zero_terminated_lines() {
        assert_eq!(rev_str("abc\0def\0", b'\0'), "cba\0fed\0");
    }

    #[test]
    fn reverse_line_helper_handles_ascii() {
        assert_eq!(reverse_line(b"abcd"), b"dcba".to_vec());
    }

    #[test]
    fn reverse_line_helper_handles_invalid_utf8_by_reversing_bytes() {
        let bytes = [0xff, 0x41, 0x42, 0xfe];
        let out = reverse_line(&bytes);
        assert_eq!(out, vec![0xfe, 0x42, 0x41, 0xff]);
    }
}
