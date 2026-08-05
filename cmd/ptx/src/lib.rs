//! user ptx — permuted index (simplified GNU ptx).
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `ptx` utility. Parses `std::env::args()` and prints
/// a permuted (KWIC-style) index: for every whitespace-separated word in
/// each `FILE` (or stdin, if none are given), a line showing that word with
/// its surrounding context, sorted case-insensitively by the keyword.
///
/// Returns 0 on success, 1 on a usage error or if any file could not be
/// opened or read.
pub fn run() -> i32 {
    let ui = Ui::new("ptx");
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: ptx [FILE]...\nOutput a permuted index of words from FILE(s).\n");
                return 0;
            }
            "--version" => {
                println!("ptx (user_utils) 0.1.0");
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
    let mut refs: Vec<(String, String, String)> = Vec::new(); // left, word, right
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
                    ui.err(&format!("{e}"));
                    return 1;
                }
            };
            refs.extend(permute_line(&line));
        }
    }
    refs.sort_by_key(|(_, w, _)| w.to_ascii_lowercase());
    let mut out = io::stdout().lock();
    for (l, w, r) in refs {
        let _ = writeln!(out, "{l:>30} {w} {r}");
    }
    0
}

/// Produce one `(left_context, word, right_context)` triple for each
/// whitespace-separated word in `line`, so a permuted-index caller can sort
/// and print every word of `line` as a keyword in turn.
fn permute_line(line: &str) -> Vec<(String, String, String)> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let mut out = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        let left = words[..i].join(" ");
        let right = words[i + 1..].join(" ");
        out.push((left, (*w).to_string(), right));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permute_line_empty_is_empty() {
        assert!(permute_line("").is_empty());
        assert!(permute_line("   ").is_empty());
    }

    #[test]
    fn permute_line_single_word_has_no_context() {
        let r = permute_line("hello");
        assert_eq!(r, vec![("".to_string(), "hello".to_string(), "".to_string())]);
    }

    #[test]
    fn permute_line_multiple_words_builds_left_right_context() {
        let r = permute_line("the quick fox");
        assert_eq!(
            r,
            vec![
                ("".to_string(), "the".to_string(), "quick fox".to_string()),
                ("the".to_string(), "quick".to_string(), "fox".to_string()),
                ("the quick".to_string(), "fox".to_string(), "".to_string()),
            ]
        );
    }
}
