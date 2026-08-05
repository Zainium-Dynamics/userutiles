//! user wc — print newline, word, and byte counts for each file.
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use usercore::Ui;

/// Accumulated newline/word/byte/char/max-line-length counts for one input.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct Counts {
    lines: u64,
    words: u64,
    bytes: u64,
    chars: u64,
    max_line: u64,
}

/// Which columns to print, resolved from CLI flags (all of `-c -m -l -w
/// -L`, or lines/words/bytes by default when none are given).
#[derive(Default, Clone, Copy)]
struct Show {
    lines: bool,
    words: bool,
    bytes: bool,
    chars: bool,
    max_line: bool,
}

/// Entry point for the `wc` utility. Parses `std::env::args()` and prints
/// newline/word/byte (and optionally char / max-line-length) counts for
/// each named file, or stdin if none are given.
///
/// Returns 0 on success, 1 if any file could not be read.
pub fn run() -> i32 {
    let ui = Ui::new("wc");
    let mut show = Show::default();
    let mut any = false;
    let mut files: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: wc [OPTION]... [FILE]...\n\
 Print newline, word, and byte counts for each FILE.\n\n\
 -c, --bytes print the byte counts\n\
 -m, --chars print the character counts\n\
 -l, --lines print the newline counts\n\
 -L, --max-line-length print the maximum display width\n\
 -w, --words print the word counts\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("wc (user_utils) 0.1.0");
                return 0;
            }
            "-c" | "--bytes" => {
                show.bytes = true;
                any = true;
            }
            "-m" | "--chars" => {
                show.chars = true;
                any = true;
            }
            "-l" | "--lines" => {
                show.lines = true;
                any = true;
            }
            "-w" | "--words" => {
                show.words = true;
                any = true;
            }
            "-L" | "--max-line-length" => {
                show.max_line = true;
                any = true;
            }
            s if s.starts_with('-') && s != "-" && !s.starts_with("--") => {
                for ch in s.chars().skip(1) {
                    any = true;
                    match ch {
                        'c' => show.bytes = true,
                        'm' => show.chars = true,
                        'l' => show.lines = true,
                        'w' => show.words = true,
                        'L' => show.max_line = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{ch}'"));
                            return 1;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if !any {
        show.lines = true;
        show.words = true;
        show.bytes = true;
    }
    if files.is_empty() {
        files.push("-".into());
    }

    let mut total = Counts::default();
    let mut status = 0;
    let multi = files.len() > 1;
    let show_name = files[0] != "-" || multi;

    for f in &files {
        match count_file(f) {
            Ok(c) => {
                print_counts(c, show, f, show_name);
                total.lines += c.lines;
                total.words += c.words;
                total.bytes += c.bytes;
                total.chars += c.chars;
                total.max_line = total.max_line.max(c.max_line);
            }
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }
    if multi {
        print_counts(total, show, "total", true);
    }
    status
}

/// Read `path` (or stdin, for `"-"`) fully and compute its [`Counts`].
fn count_file(path: &str) -> io::Result<Counts> {
    let mut reader: Box<dyn Read> = if path == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(Path::new(path))?)
    };
    let mut c = Counts::default();
    let mut buf = [0u8; 64 * 1024];
    let mut in_word = false;
    let mut line_len = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for &b in &buf[..n] {
            c.bytes += 1;
            // char count: count UTF-8 leading bytes approx + ASCII
            if b & 0xC0 != 0x80 {
                c.chars += 1;
            }
            if b == b'\n' {
                c.lines += 1;
                c.max_line = c.max_line.max(line_len);
                line_len = 0;
                in_word = false;
            } else {
                if b == b'\t' {
                    line_len += 8 - (line_len % 8);
                } else if b >= 0x20 {
                    line_len += 1;
                }
                if b.is_ascii_whitespace() {
                    in_word = false;
                } else if !in_word {
                    in_word = true;
                    c.words += 1;
                }
            }
        }
    }
    c.max_line = c.max_line.max(line_len);
    Ok(c)
}

/// Print one result row: the requested columns (in `-l -w -c -m -L`
/// order), then `name` if `show_name` is set (and `name` isn't the stdin
/// placeholder `"-"`).
fn print_counts(c: Counts, show: Show, name: &str, show_name: bool) {
    let mut parts = Vec::new();
    if show.lines {
        parts.push(format!("{:8}", c.lines));
    }
    if show.words {
        parts.push(format!("{:8}", c.words));
    }
    if show.chars {
        parts.push(format!("{:8}", c.chars));
    }
    if show.bytes {
        parts.push(format!("{:8}", c.bytes));
    }
    if show.max_line {
        parts.push(format!("{:8}", c.max_line));
    }
    if show_name && name != "-" {
        println!("{} {name}", parts.join(" "));
    } else {
        println!("{}", parts.join(" "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_file(name: &str, content: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("user_wc_test_{}_{name}", std::process::id()));
        let mut f = File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    #[test]
    fn count_file_basic() {
        let p = tmp_file("basic", b"hello world\nfoo\n");
        let c = count_file(p.to_str().unwrap()).unwrap();
        assert_eq!(c.lines, 2);
        assert_eq!(c.words, 3);
        assert_eq!(c.bytes, 16);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn count_file_empty() {
        let p = tmp_file("empty", b"");
        let c = count_file(p.to_str().unwrap()).unwrap();
        assert_eq!(c, Counts::default());
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn count_file_no_trailing_newline_still_counts_word() {
        let p = tmp_file("no_nl", b"abc");
        let c = count_file(p.to_str().unwrap()).unwrap();
        assert_eq!(c.lines, 0);
        assert_eq!(c.words, 1);
        assert_eq!(c.bytes, 3);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn count_file_missing_errors() {
        let missing = format!("/nonexistent_user_wc_test_{}", std::process::id());
        assert!(count_file(&missing).is_err());
    }

    #[test]
    fn count_file_max_line_tracks_tabs() {
        // A tab advances to the next multiple of 8.
        let p = tmp_file("tabs", b"a\tb\n");
        let c = count_file(p.to_str().unwrap()).unwrap();
        assert_eq!(c.max_line, 9); // 'a' -> col 1, tab -> col 8, 'b' -> col 9
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn count_file_utf8_chars_vs_bytes() {
        let p = tmp_file("utf8", "héllo\n".as_bytes());
        let c = count_file(p.to_str().unwrap()).unwrap();
        assert_eq!(c.bytes, 7); // 'é' is 2 bytes in UTF-8
        assert_eq!(c.chars, 6);
    }
}
