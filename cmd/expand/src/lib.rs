//! user expand — convert tabs to spaces.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `expand` utility. Parses `std::env::args()` and
/// writes each `FILE` (or stdin, for `-` or no files) to stdout with tab
/// characters expanded to spaces at every `tabstop`-th column.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("expand");
    let mut tabstop = 8usize;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: expand [OPTION]... [FILE]...\nConvert tabs in each FILE to spaces.\n -t, --tabs=N have tabs N characters apart (default 8)\n");
                return 0;
            }
            "--version" => {
                println!("expand (user_utils) 0.1.0");
                return 0;
            }
            "-t" | "--tabs" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse::<usize>().ok()) {
                    Some(n) if n >= 1 => tabstop = n,
                    _ => {
                        ui.err("invalid tab size");
                        return 1;
                    }
                }
            }
            s if s.starts_with("-t") && s.len() > 2 => match s[2..].parse::<usize>() {
                Ok(n) if n >= 1 => tabstop = n,
                _ => {
                    ui.err(&format!("invalid tab size: '{}'", &s[2..]));
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
                    ui.err(&format!("{e}"));
                    return 1;
                }
            };
            if let Err(e) = writeln!(out, "{}", expand_line(&line, tabstop)) {
                if e.kind() != io::ErrorKind::BrokenPipe {
                    ui.err(&format!("{e}"));
                    return 1;
                }
                return 0;
            }
        }
    }
    0
}

/// Expand tab characters in `line` to spaces, so the next non-tab column
/// lands on a multiple of `tabstop` (matching GNU `expand`'s single
/// fixed-tabstop behavior). `tabstop` must be at least 1.
fn expand_line(line: &str, tabstop: usize) -> String {
    let mut result = String::with_capacity(line.len());
    let mut col = 0usize;
    for ch in line.chars() {
        if ch == '\t' {
            let spaces = tabstop - (col % tabstop);
            result.extend(std::iter::repeat(' ').take(spaces));
            col += spaces;
        } else {
            result.push(ch);
            col += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_line_no_tabs_unchanged() {
        assert_eq!(expand_line("hello", 8), "hello");
    }

    #[test]
    fn expand_line_single_tab_at_start() {
        assert_eq!(expand_line("\tx", 8), "        x");
    }

    #[test]
    fn expand_line_tab_after_partial_column() {
        assert_eq!(expand_line("ab\tc", 4), "ab  c");
    }

    #[test]
    fn expand_line_multiple_tabs() {
        assert_eq!(expand_line("\t\t", 4), "        ");
    }

    #[test]
    fn expand_line_custom_tabstop_of_one() {
        assert_eq!(expand_line("a\tb", 1), "a b");
    }

    #[test]
    fn expand_line_empty_input() {
        assert_eq!(expand_line("", 8), "");
    }
}
