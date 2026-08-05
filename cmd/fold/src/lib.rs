//! user fold — wrap lines.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `fold` utility. Parses `std::env::args()` and
/// writes each `FILE` (or stdin, for `-` or no files) to stdout with every
/// line wrapped to at most `width` columns (or bytes, with `-b`).
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("fold");
    let mut width = 80usize;
    let mut bytes = false;
    let mut spaces = false;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: fold [OPTION]... [FILE]...\nWrap input lines in each FILE.\n -b, --bytes count bytes rather than columns\n -s, --spaces break at spaces\n -w, --width=WIDTH use WIDTH columns instead of 80\n");
                return 0;
            }
            "--version" => {
                println!("fold (user_utils) 0.1.0");
                return 0;
            }
            "-b" | "--bytes" => bytes = true,
            "-s" | "--spaces" => spaces = true,
            "-w" | "--width" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse::<usize>().ok()) {
                    Some(n) if n >= 1 => width = n,
                    _ => {
                        ui.err("invalid width");
                        return 1;
                    }
                }
            }
            s if s.starts_with("-w") && s.len() > 2 => match s[2..].parse::<usize>() {
                Ok(n) if n >= 1 => width = n,
                _ => {
                    ui.err(&format!("invalid width: '{}'", &s[2..]));
                    return 1;
                }
            },
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
        i += 1;
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
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    return 1;
                }
            };
            fold_line(&line, width, bytes, spaces, &mut out);
        }
    }
    0
}

/// Wrap a single input `line` (no trailing newline) to `width` columns (or
/// bytes, if `bytes` is set), writing each wrapped segment followed by a
/// newline to `out`. When `spaces` is set, a wrap point is pulled back to
/// the last whitespace character in the segment when one exists (so words
/// aren't split mid-word), matching `-s`.
fn fold_line(line: &str, width: usize, bytes: bool, spaces: bool, out: &mut impl Write) {
    if line.is_empty() {
        let _ = writeln!(out);
        return;
    }
    if bytes {
        let b = line.as_bytes();
        for chunk in b.chunks(width) {
            let _ = out.write_all(chunk);
            let _ = writeln!(out);
        }
        return;
    }
    let chars: Vec<char> = line.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + width).min(chars.len());
        if spaces && end < chars.len() {
            if let Some(rel) = chars[start..end].iter().rposition(|c| c.is_whitespace()) {
                if rel > 0 {
                    end = start + rel + 1;
                }
            }
        }
        let s: String = chars[start..end].iter().collect();
        let _ = writeln!(out, "{s}");
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(line: &str, width: usize, bytes: bool, spaces: bool) -> String {
        let mut out = Vec::new();
        fold_line(line, width, bytes, spaces, &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn fold_line_empty_yields_blank_line() {
        assert_eq!(fold("", 10, false, false), "\n");
    }

    #[test]
    fn fold_line_shorter_than_width_unchanged() {
        assert_eq!(fold("hi", 10, false, false), "hi\n");
    }

    #[test]
    fn fold_line_wraps_at_width_by_chars() {
        assert_eq!(fold("abcdefgh", 3, false, false), "abc\ndef\ngh\n");
    }

    #[test]
    fn fold_line_wraps_by_bytes() {
        assert_eq!(fold("abcdefgh", 3, true, false), "abc\ndef\ngh\n");
    }

    #[test]
    fn fold_line_spaces_breaks_at_whitespace() {
        // "abc def" with width 5: without -s it'd cut mid-word ("abc d" /
        // "ef"); with -s it should break after "abc ".
        assert_eq!(fold("abc def", 5, false, true), "abc \ndef\n");
    }

    #[test]
    fn fold_line_spaces_with_no_whitespace_falls_back_to_hard_wrap() {
        assert_eq!(fold("abcdefgh", 3, false, true), "abc\ndef\ngh\n");
    }
}
