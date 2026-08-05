//! user fmt — reformat paragraphs to a given width.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `fmt` utility. Parses `std::env::args()` and
/// rewraps each paragraph (a run of non-blank lines, separated by blank
/// lines) of each `FILE` (or stdin, for `-` or no files) to `width`
/// columns.
///
/// `-c`/`--crown-margin` and `-t`/`--tagged-paragraph` are accepted for
/// CLI compatibility but not yet honored — indentation is not currently
/// preserved specially for the first line(s) of a paragraph.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("fmt");
    let mut width = 75usize;
    let mut crown = false;
    let mut tagged = false;
    let mut split_only = false;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: fmt [-w WIDTH] [FILE]...\nReformat each paragraph to WIDTH columns (default 75).\n -w, --width=WIDTH maximum line width\n -c, --crown-margin preserve first two lines indent\n -t, --tagged-paragraph indentation of first line different\n -s, --split-only split long lines, do not refill\n");
                return 0;
            }
            "--version" => {
                println!("fmt (user_utils) 0.1.0");
                return 0;
            }
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
            "-c" | "--crown-margin" => crown = true,
            "-t" | "--tagged-paragraph" => tagged = true,
            "-s" | "--split-only" => split_only = true,
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }
    let _ = (crown, tagged); // accepted but not yet honored, see doc comment
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
        if let Err(e) = format_stream(reader, &mut out, width, split_only) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                ui.err(&format!("{e}"));
                status = 1;
            }
        }
    }
    status
}

/// Read lines from `reader`, splitting them into paragraphs at blank
/// lines, and write each reformatted paragraph (plus the blank line that
/// ended it) to `out`.
fn format_stream(
    reader: Box<dyn BufRead>,
    out: &mut impl Write,
    width: usize,
    split_only: bool,
) -> io::Result<()> {
    let mut para: Vec<String> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            flush_para(&mut para, out, width, split_only)?;
            writeln!(out)?;
        } else {
            para.push(line);
        }
    }
    flush_para(&mut para, out, width, split_only)
}

/// Reformat and write the buffered paragraph `para` to `out`, then clear
/// it. In `split_only` mode, each input line is wrapped independently
/// (long lines broken at whitespace, short lines left as-is) rather than
/// being re-flowed as a single run of words.
fn flush_para(
    para: &mut Vec<String>,
    out: &mut dyn Write,
    width: usize,
    split_only: bool,
) -> io::Result<()> {
    if para.is_empty() {
        return Ok(());
    }
    if split_only {
        for line in para.drain(..) {
            if line.chars().count() <= width {
                writeln!(out, "{line}")?;
            } else {
                for wrapped in wrap_words(line.split_whitespace(), width) {
                    writeln!(out, "{wrapped}")?;
                }
            }
        }
    } else {
        let words = para.drain(..).flat_map(|l| {
            l.split_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        });
        for wrapped in wrap_words(words, width) {
            writeln!(out, "{wrapped}")?;
        }
    }
    Ok(())
}

/// Greedily pack `words` into lines of at most `width` columns (never
/// splitting a single word, however long), returning the finished lines.
fn wrap_words<I, S>(words: I, width: usize) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut lines = Vec::new();
    let mut col = 0usize;
    let mut line = String::new();
    for w in words {
        let w = w.as_ref();
        let wl = w.chars().count();
        if col > 0 && col + 1 + wl > width {
            lines.push(std::mem::take(&mut line));
            col = 0;
        }
        if col > 0 {
            line.push(' ');
            col += 1;
        }
        line.push_str(w);
        col += wl;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flush(words: &[&str], width: usize, split_only: bool) -> String {
        let mut para: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        flush_para(&mut para, &mut out, width, split_only).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn flush_para_empty_is_noop() {
        assert_eq!(flush(&[], 75, false), "");
    }

    #[test]
    fn flush_para_wraps_short_words_to_width() {
        let result = flush(&["one two three four"], 9, false);
        assert_eq!(result, "one two\nthree\nfour\n");
    }

    #[test]
    fn flush_para_never_splits_a_long_word() {
        let result = flush(&["supercalifragilistic short"], 5, false);
        assert_eq!(result, "supercalifragilistic\nshort\n");
    }

    #[test]
    fn flush_para_split_only_preserves_short_lines_as_is() {
        let result = flush(&["a  b   c"], 75, true);
        // split_only keeps the original line verbatim when it already fits.
        assert_eq!(result, "a  b   c\n");
    }

    #[test]
    fn flush_para_split_only_wraps_long_lines() {
        let result = flush(&["one two three four"], 9, true);
        assert_eq!(result, "one two\nthree\nfour\n");
    }

    #[test]
    fn format_stream_separates_paragraphs_with_blank_line() {
        let input = "hello world\n\nsecond para\n";
        let reader: Box<dyn BufRead> = Box::new(BufReader::new(input.as_bytes()));
        let mut out = Vec::new();
        format_stream(reader, &mut out, 75, false).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "hello world\n\nsecond para\n"
        );
    }

    #[test]
    fn format_stream_empty_input_produces_no_output() {
        let reader: Box<dyn BufRead> = Box::new(BufReader::new("".as_bytes()));
        let mut out = Vec::new();
        format_stream(reader, &mut out, 75, false).unwrap();
        assert!(out.is_empty());
    }
}
