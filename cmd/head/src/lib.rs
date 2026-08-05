//! user head — output the first part of files.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

use usercore::Ui;

/// Entry point for the `head` utility. Parses `std::env::args()` and
/// writes the first `NUM` lines (default 10, `-n`) or bytes (`-c`) of each
/// `FILE` (or stdin, for `-` or no files) to stdout.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("head");
    let mut lines: Option<u64> = Some(10);
    let mut bytes: Option<u64> = None;
    let mut quiet = false;
    let mut verbose = false;
    let mut files: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: head [OPTION]... [FILE]...\n\
 Print the first 10 lines of each FILE to standard output.\n\n\
 -c, --bytes=[-]NUM print the first NUM bytes\n\
 -n, --lines=[-]NUM print the first NUM lines\n\
 -q, --quiet, --silent never print headers\n\
 -v, --verbose always print headers\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("head (user_utils) 0.1.0");
                return 0;
            }
            "-q" | "--quiet" | "--silent" => quiet = true,
            "-v" | "--verbose" => verbose = true,
            "-n" | "--lines" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'n'");
                    return 1;
                };
                match parse_count(arg) {
                    Ok(n) => {
                        lines = Some(n);
                        bytes = None;
                    }
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            "-c" | "--bytes" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'c'");
                    return 1;
                };
                match parse_count(arg) {
                    Ok(n) => {
                        bytes = Some(n);
                        lines = None;
                    }
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            s if s.starts_with("-n") && s.len() > 2 => match parse_count(&s[2..]) {
                Ok(n) => {
                    lines = Some(n);
                    bytes = None;
                }
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with("-c") && s.len() > 2 => match parse_count(&s[2..]) {
                Ok(n) => {
                    bytes = Some(n);
                    lines = None;
                }
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with('-') && s.len() > 1 && s.as_bytes()[1].is_ascii_digit() => {
                // historic: -NUM
                match parse_count(&s[1..]) {
                    Ok(n) => {
                        lines = Some(n);
                        bytes = None;
                    }
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            "--" => {
                files.extend(args[i + 1..].iter().cloned());
                break;
            }
            s if s.starts_with('-') => {
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

    let multi = files.len() > 1;
    let show_header = verbose || (multi && !quiet);
    let mut status = 0;
    let mut out = io::stdout().lock();

    for (idx, f) in files.iter().enumerate() {
        if show_header {
            if idx > 0 {
                let _ = writeln!(out);
            }
            let _ = writeln!(out, "==> {f} <==");
        }
        let r = if let Some(b) = bytes {
            head_bytes(f, b, &mut out)
        } else {
            head_lines(f, lines.unwrap_or(10), &mut out)
        };
        if let Err(e) = r {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            ui.err(&format!("{f}: {e}"));
            status = 1;
        }
    }
    status
}

/// Parse a `head` count argument (`-n`/`-c`'s value): an optional leading
/// `+` is stripped (accepted but not currently given special "from offset"
/// semantics), and the remainder must be a valid `u64`.
fn parse_count(s: &str) -> Result<u64, String> {
    let s = s.trim_start_matches('+');
    s.parse::<u64>()
        .map_err(|_| format!("invalid number of lines/bytes: '{s}'"))
}

/// Copy the first `n` lines (including their terminators) of `path` (or
/// stdin, for `-`) to `out`.
fn head_lines(path: &str, n: u64, out: &mut impl Write) -> io::Result<()> {
    let mut reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(Path::new(path))?))
    };
    let mut count = 0u64;
    let mut buf = Vec::new();
    while count < n {
        buf.clear();
        let read = reader.read_until(b'\n', &mut buf)?;
        if read == 0 {
            break;
        }
        out.write_all(&buf)?;
        count += 1;
    }
    out.flush()
}

/// Copy the first `n` bytes of `path` (or stdin, for `-`) to `out`.
fn head_bytes(path: &str, n: u64, out: &mut impl Write) -> io::Result<()> {
    let mut reader: Box<dyn Read> = if path == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(Path::new(path))?)
    };
    let mut left = n;
    let mut buf = [0u8; 8192];
    while left > 0 {
        let want = (left as usize).min(buf.len());
        let got = reader.read(&mut buf[..want])?;
        if got == 0 {
            break;
        }
        out.write_all(&buf[..got])?;
        left -= got as u64;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_count_plain_number() {
        assert_eq!(parse_count("5"), Ok(5));
    }

    #[test]
    fn parse_count_strips_leading_plus() {
        assert_eq!(parse_count("+5"), Ok(5));
    }

    #[test]
    fn parse_count_rejects_non_numeric() {
        assert!(parse_count("abc").is_err());
    }

    #[test]
    fn parse_count_rejects_empty() {
        assert!(parse_count("").is_err());
    }

    fn scratch_file(name: &str, contents: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("user_head_test_{}_{}", std::process::id(), name));
        let mut f = File::create(&p).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        p
    }

    #[test]
    fn head_lines_reads_first_n_lines() {
        let p = scratch_file("lines", "a\nb\nc\nd\n");
        let mut out = Vec::new();
        head_lines(p.to_str().unwrap(), 2, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "a\nb\n");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn head_lines_n_larger_than_file_returns_all() {
        let p = scratch_file("lines_all", "a\nb\n");
        let mut out = Vec::new();
        head_lines(p.to_str().unwrap(), 100, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "a\nb\n");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn head_lines_zero_returns_nothing() {
        let p = scratch_file("lines_zero", "a\nb\n");
        let mut out = Vec::new();
        head_lines(p.to_str().unwrap(), 0, &mut out).unwrap();
        assert!(out.is_empty());
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn head_lines_missing_file_errors() {
        let missing = format!("/nonexistent_user_head_test_{}", std::process::id());
        let mut out = Vec::new();
        assert!(head_lines(&missing, 1, &mut out).is_err());
    }

    #[test]
    fn head_bytes_reads_first_n_bytes() {
        let p = scratch_file("bytes", "abcdefgh");
        let mut out = Vec::new();
        head_bytes(p.to_str().unwrap(), 3, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "abc");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn head_bytes_missing_file_errors() {
        let missing = format!("/nonexistent_user_head_test_{}", std::process::id());
        let mut out = Vec::new();
        assert!(head_bytes(&missing, 1, &mut out).is_err());
    }
}
