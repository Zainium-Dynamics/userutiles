//! user uniq — report or omit repeated lines.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Options controlling how [`process`] filters adjacent matching lines.
#[derive(Default, Clone, Copy)]
struct Options {
    count: bool,
    repeated: bool,
    unique_only: bool,
    ignore_case: bool,
    skip_fields: usize,
    skip_chars: usize,
}

/// Entry point for the `uniq` utility. Parses `std::env::args()`, then
/// filters adjacent matching lines from the input file (or stdin) to the
/// output file (or stdout).
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("uniq");
    let mut opts = Options::default();
    let mut files: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: uniq [OPTION]... [INPUT [OUTPUT]]\n\
 Filter adjacent matching lines from INPUT (or standard input).\n\n\
 -c, --count prefix lines by the number of occurrences\n\
 -d, --repeated only print duplicate lines, one for each group\n\
 -f, --skip-fields=N avoid comparing the first N fields\n\
 -i, --ignore-case ignore differences in case when comparing\n\
 -s, --skip-chars=N avoid comparing the first N characters\n\
 -u, --unique only print unique lines\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("uniq (user_utils) 0.1.0");
                return 0;
            }
            "-c" | "--count" => opts.count = true,
            "-d" | "--repeated" => opts.repeated = true,
            "-u" | "--unique" => opts.unique_only = true,
            "-i" | "--ignore-case" => opts.ignore_case = true,
            "-f" | "--skip-fields" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option requires an argument -- 'f'");
                    return 1;
                };
                match v.parse() {
                    Ok(n) => opts.skip_fields = n,
                    Err(_) => {
                        ui.err(&format!("invalid number of fields to skip: '{v}'"));
                        return 1;
                    }
                }
            }
            "-s" | "--skip-chars" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option requires an argument -- 's'");
                    return 1;
                };
                match v.parse() {
                    Ok(n) => opts.skip_chars = n,
                    Err(_) => {
                        ui.err(&format!("invalid number of bytes to skip: '{v}'"));
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s != "-" && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'c' => opts.count = true,
                        'd' => opts.repeated = true,
                        'u' => opts.unique_only = true,
                        'i' => opts.ignore_case = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }

    let input = files.first().map(|s| s.as_str()).unwrap_or("-");
    let output = files.get(1).map(|s| s.as_str());

    let reader: Box<dyn BufRead> = if input == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        match File::open(input) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                ui.err(&format!("{input}: {e}"));
                return 1;
            }
        }
    };

    let mut out: Box<dyn Write> = if let Some(path) = output {
        match File::create(path) {
            Ok(f) => Box::new(f),
            Err(e) => {
                ui.err(&format!("{path}: {e}"));
                return 1;
            }
        }
    } else {
        Box::new(io::stdout())
    };

    match process(reader, &mut *out, &opts) {
        Ok(()) => 0,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            ui.err(&format!("{e}"));
            1
        }
    }
}

/// Read lines from `reader`, drop adjacent duplicates per `opts`, and write
/// the surviving lines to `writer`.
fn process(reader: impl BufRead, writer: &mut dyn Write, opts: &Options) -> io::Result<()> {
    let mut prev: Option<String> = None;
    let mut prev_key = String::new();
    let mut n = 0u64;

    for line in reader.lines() {
        let line = line?;
        let key = make_key(&line, opts.skip_fields, opts.skip_chars, opts.ignore_case);
        if let Some(ref p) = prev {
            if key == prev_key {
                n = n.saturating_add(1);
                continue;
            }
            emit(writer, p, n, opts)?;
        }
        prev = Some(line);
        prev_key = key;
        n = 1;
    }
    if let Some(ref p) = prev {
        emit(writer, p, n, opts)?;
    }
    Ok(())
}

/// Write a single (possibly count-prefixed) output line, honoring
/// `-d`/`-u` filtering.
fn emit(out: &mut dyn Write, line: &str, n: u64, opts: &Options) -> io::Result<()> {
    if opts.repeated && n < 2 {
        return Ok(());
    }
    if opts.unique_only && n != 1 {
        return Ok(());
    }
    if opts.count {
        writeln!(out, "{n:7} {line}")
    } else {
        writeln!(out, "{line}")
    }
}

/// Build the comparison key for `line`: skip the first `skip_fields`
/// whitespace-separated fields, then the first `skip_chars` characters of
/// what remains, optionally folding to lowercase for `-i`.
fn make_key(line: &str, skip_fields: usize, skip_chars: usize, ignore_case: bool) -> String {
    let mut s = line;
    // skip fields (whitespace-separated)
    let mut skipped = 0;
    while skipped < skip_fields {
        let rest = s.trim_start();
        if rest.is_empty() {
            s = rest;
            break;
        }
        if let Some(pos) = rest.find(char::is_whitespace) {
            s = rest[pos..].trim_start();
        } else {
            s = "";
            break;
        }
        skipped += 1;
    }
    let s = if skip_chars > 0 {
        s.chars().skip(skip_chars).collect::<String>()
    } else {
        s.to_string()
    };
    if ignore_case {
        s.to_ascii_lowercase()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_process(input: &str, opts: Options) -> String {
        let mut out = Vec::new();
        process(io::Cursor::new(input.as_bytes()), &mut out, &opts).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn make_key_plain() {
        assert_eq!(make_key("hello world", 0, 0, false), "hello world");
    }

    #[test]
    fn make_key_skip_fields() {
        assert_eq!(make_key("a b c", 1, 0, false), "b c");
        assert_eq!(make_key("a b c", 2, 0, false), "c");
        assert_eq!(make_key("a b c", 10, 0, false), "");
    }

    #[test]
    fn make_key_skip_chars() {
        assert_eq!(make_key("hello", 0, 2, false), "llo");
        assert_eq!(make_key("hi", 0, 10, false), "");
    }

    #[test]
    fn make_key_ignore_case() {
        assert_eq!(make_key("HeLLo", 0, 0, true), "hello");
    }

    #[test]
    fn process_default_drops_adjacent_duplicates() {
        let out = run_process("a\na\nb\nb\nb\nc\n", Options::default());
        assert_eq!(out, "a\nb\nc\n");
    }

    #[test]
    fn process_non_adjacent_duplicates_kept() {
        let out = run_process("a\nb\na\n", Options::default());
        assert_eq!(out, "a\nb\na\n");
    }

    #[test]
    fn process_count_prefixes_occurrences() {
        let opts = Options {
            count: true,
            ..Options::default()
        };
        let out = run_process("a\na\nb\n", opts);
        assert_eq!(out, "      2 a\n      1 b\n");
    }

    #[test]
    fn process_repeated_only() {
        let opts = Options {
            repeated: true,
            ..Options::default()
        };
        let out = run_process("a\na\nb\nc\nc\n", opts);
        assert_eq!(out, "a\nc\n");
    }

    #[test]
    fn process_unique_only() {
        let opts = Options {
            unique_only: true,
            ..Options::default()
        };
        let out = run_process("a\na\nb\nc\nc\n", opts);
        assert_eq!(out, "b\n");
    }

    #[test]
    fn process_ignore_case() {
        let opts = Options {
            ignore_case: true,
            ..Options::default()
        };
        let out = run_process("Hello\nhello\nHELLO\n", opts);
        assert_eq!(out, "Hello\n");
    }

    #[test]
    fn process_empty_input() {
        let out = run_process("", Options::default());
        assert_eq!(out, "");
    }
}
