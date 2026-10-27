//! user uniq — report or omit repeated lines.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Options controlling how [`process`] filters adjacent matching lines.
#[derive(Default, Clone)]
struct Options {
    count: bool,
    repeated: bool,
    unique_only: bool,
    ignore_case: bool,
    skip_fields: usize,
    skip_chars: usize,
    /// -w: compare at most this many characters
    check_chars: Option<usize>,
    /// -D / --all-repeated[=METHOD]: none, prepend, separate
    all_repeated: Option<String>,
    /// --group[=METHOD]: separate, prepend, append, both
    group: Option<String>,
    zero: bool,
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
            "-z" | "--zero-terminated" => opts.zero = true,
            "-D" | "--all-repeated" => opts.all_repeated = Some("none".into()),
            "--group" => opts.group = Some("separate".into()),
            s if s.starts_with("--all-repeated=") => opts.all_repeated = Some(s[15..].to_string()),
            s if s.starts_with("--group=") => opts.group = Some(s[8..].to_string()),
            s if s.starts_with("--skip-fields=") || s.starts_with("--skip-chars=") || s.starts_with("--check-chars=") => {
                let (k, v) = s.split_once('=').unwrap_or((s, ""));
                let Ok(n) = v.parse::<usize>() else {
                    ui.err(&format!("invalid number: '{v}'"));
                    return 1;
                };
                match k {
                    "--skip-fields" => opts.skip_fields = n,
                    "--skip-chars" => opts.skip_chars = n,
                    _ => opts.check_chars = Some(n),
                }
            }
            "-w" | "--check-chars" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse().ok()) {
                    Some(n) => opts.check_chars = Some(n),
                    None => {
                        ui.err("invalid number of bytes to compare");
                        return 1;
                    }
                }
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
                let chars: Vec<char> = s.chars().skip(1).collect();
                let mut k = 0;
                while k < chars.len() {
                    match chars[k] {
                        'c' => opts.count = true,
                        'd' => opts.repeated = true,
                        'u' => opts.unique_only = true,
                        'i' => opts.ignore_case = true,
                        'z' => opts.zero = true,
                        'D' => opts.all_repeated = Some("none".into()),
                        c @ ('f' | 's' | 'w') => {
                            let inline: String = chars[k + 1..].iter().collect();
                            let v = if inline.is_empty() {
                                i += 1;
                                args.get(i).cloned().unwrap_or_default()
                            } else {
                                inline
                            };
                            let Ok(n) = v.parse::<usize>() else {
                                ui.err(&format!("invalid number: '{v}'"));
                                return 1;
                            };
                            match c {
                                'f' => opts.skip_fields = n,
                                's' => opts.skip_chars = n,
                                _ => opts.check_chars = Some(n),
                            }
                            break;
                        }
                        c => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                    k += 1;
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
/// Group adjacent lines with equal keys and write what the options ask for.
fn process(mut reader: impl BufRead, writer: &mut dyn Write, opts: &Options) -> io::Result<()> {
    let term = if opts.zero { 0u8 } else { b'\n' };
    let mut groups_seen = 0usize;
    let mut group: Vec<String> = Vec::new();
    let mut key = String::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(term, &mut buf)?;
        let line = if n == 0 {
            None
        } else {
            if buf.last() == Some(&term) {
                buf.pop();
            }
            Some(String::from_utf8_lossy(&buf).into_owned())
        };
        let k = line.as_deref().map(|l| make_key(l, opts));
        if group.is_empty() || k.as_ref() != Some(&key) {
            if !group.is_empty() {
                emit_group(writer, &group, opts, &mut groups_seen, term)?;
            }
            group.clear();
            if let Some(k) = k {
                key = k;
            }
        }
        match line {
            Some(l) => group.push(l),
            None => break,
        }
    }
    if opts.group.as_deref().is_some_and(|m| m == "append" || m == "both") && groups_seen > 0 {
        writer.write_all(&[term])?;
    }
    Ok(())
}

/// One run of equal lines.
fn emit_group(out: &mut dyn Write, group: &[String], opts: &Options, seen: &mut usize, term: u8) -> io::Result<()> {
    let line = |out: &mut dyn Write, l: &str| -> io::Result<()> {
        out.write_all(l.as_bytes())?;
        out.write_all(&[term])
    };
    if let Some(method) = &opts.group {
        let sep_before = match method.as_str() {
            "prepend" | "both" => true,
            "separate" | "append" => *seen > 0,
            _ => false,
        };
        if sep_before {
            out.write_all(&[term])?;
        }
        for l in group {
            line(out, l)?;
        }
        *seen += 1;
        return Ok(());
    }
    if let Some(method) = &opts.all_repeated {
        if group.len() < 2 {
            return Ok(());
        }
        if (method == "prepend") || (method == "separate" && *seen > 0) {
            out.write_all(&[term])?;
        }
        for l in group {
            line(out, l)?;
        }
        *seen += 1;
        return Ok(());
    }
    let n = group.len();
    if (opts.repeated && n < 2) || (opts.unique_only && n != 1) {
        return Ok(());
    }
    if opts.count {
        out.write_all(format!("{n:7} ").as_bytes())?;
    }
    line(out, &group[0])
}

/// The part of `line` compared: after `-f` fields (each blanks then non-blanks, the
/// blanks before the next field kept) and `-s` characters, at most `-w` characters,
/// lowercased for `-i`.
fn make_key(line: &str, opts: &Options) -> String {
    let blank = |c: char| c == ' ' || c == '\t';
    let mut s = line;
    for _ in 0..opts.skip_fields {
        let t = s.trim_start_matches(blank);
        s = t.find(blank).map_or("", |p| &t[p..]);
    }
    let mut k: String = s.chars().skip(opts.skip_chars).collect();
    if let Some(w) = opts.check_chars {
        k = k.chars().take(w).collect();
    }
    if opts.ignore_case {
        k.to_lowercase()
    } else {
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(line: &str, skip_fields: usize, skip_chars: usize, ignore_case: bool) -> String {
        make_key(line, &Options { skip_fields, skip_chars, ignore_case, ..Default::default() })
    }

    fn run_process(input: &str, opts: Options) -> String {
        let mut out = Vec::new();
        process(io::Cursor::new(input.as_bytes()), &mut out, &opts).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn make_key_plain() {
        assert_eq!(key("hello world", 0, 0, false), "hello world");
    }

    #[test]
    fn make_key_skip_fields() {
        assert_eq!(key("a b c", 1, 0, false), " b c");
        assert_eq!(key("a b c", 2, 0, false), " c");
        assert_eq!(key("a b c", 10, 0, false), "");
    }

    #[test]
    fn make_key_skip_chars() {
        assert_eq!(key("hello", 0, 2, false), "llo");
        assert_eq!(key("hi", 0, 10, false), "");
    }

    #[test]
    fn make_key_ignore_case() {
        assert_eq!(key("HeLLo", 0, 0, true), "hello");
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
