//! user tail — output the last part of files.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use usercore::Ui;

/// Entry point for the `tail` utility. Parses `std::env::args()` and prints
/// the last N lines (default 10) or last N bytes of each file, optionally
/// following growth of the last file with `-f`.
///
/// Returns 0 on success, 1 if a file could not be opened/read or an
/// argument was invalid.
pub fn run() -> i32 {
    let ui = Ui::new("tail");
    let mut lines: Option<u64> = Some(10);
    let mut bytes: Option<u64> = None;
    let mut quiet = false;
    let mut verbose = false;
    let mut follow = false;
    let mut files: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("tail (user_utils) 0.1.0");
                return 0;
            }
            "-q" | "--quiet" | "--silent" => quiet = true,
            "-v" | "--verbose" => verbose = true,
            "-f" | "--follow" => follow = true,
            "-n" | "--lines" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'n'");
                    return 1;
                }
                match parse_count(&args[i]) {
                    Ok(n) => {
                        lines = Some(n);
                        bytes = None;
                    }
                    Err(_) => {
                        ui.err(&format!("invalid number of lines: '{}'", args[i]));
                        return 1;
                    }
                }
            }
            "-c" | "--bytes" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'c'");
                    return 1;
                }
                match parse_count(&args[i]) {
                    Ok(n) => {
                        bytes = Some(n);
                        lines = None;
                    }
                    Err(_) => {
                        ui.err(&format!("invalid number of bytes: '{}'", args[i]));
                        return 1;
                    }
                }
            }
            s if s.starts_with("-n") && s.len() > 2 => match parse_count(&s[2..]) {
                Ok(n) => {
                    lines = Some(n);
                    bytes = None;
                }
                Err(_) => {
                    ui.err("invalid number of lines");
                    return 1;
                }
            },
            s if s.starts_with('-') && s.len() > 1 && s.as_bytes()[1].is_ascii_digit() => {
                match s[1..].parse::<u64>() {
                    Ok(n) => {
                        lines = Some(n);
                        bytes = None;
                    }
                    Err(_) => {
                        ui.err("invalid number");
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
            tail_bytes(f, b, &mut out)
        } else {
            tail_lines(f, lines.unwrap_or(10), &mut out)
        };
        if let Err(e) = r {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            ui.err(&format!("{f}: {e}"));
            status = 1;
            continue;
        }
        if follow && f != "-" {
            if let Err(e) = follow_file(f, &mut out) {
                if e.kind() != io::ErrorKind::BrokenPipe {
                    ui.err(&format!("{f}: {e}"));
                    status = 1;
                }
            }
        }
    }
    status
}

fn print_help() {
    print!(
        "Usage: tail [OPTION]... [FILE]...\n\
 Print the last 10 lines of each FILE to standard output.\n\n\
 -c, --bytes=[+]NUM output the last NUM bytes\n\
 -n, --lines=[+]NUM output the last NUM lines\n\
 -f, --follow output appended data as the file grows\n\
 -q, --quiet, --silent never output headers\n\
 -v, --verbose always output headers\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Parse a `-n`/`-c` count argument, accepting (and ignoring, since this
/// build always counts from the end) a leading `+`. Returns an error
/// instead of silently defaulting on invalid input.
fn parse_count(s: &str) -> Result<u64, std::num::ParseIntError> {
    s.trim_start_matches('+').parse::<u64>()
}

/// Print the last `n` lines of `path` (or stdin, for `-`) to `out`.
fn tail_lines(path: &str, n: u64, out: &mut impl Write) -> io::Result<()> {
    if path == "-" {
        let stdin = io::stdin();
        return tail_lines_from(stdin.lock(), n, out);
    }
    let file = File::open(Path::new(path))?;
    tail_lines_from(BufReader::new(file), n, out)
}

/// Ring-buffer the last `n` newline-delimited records from `reader` and
/// write them to `out`.
fn tail_lines_from(mut reader: impl BufRead, n: u64, out: &mut impl Write) -> io::Result<()> {
    if n == 0 {
        return Ok(());
    }
    let mut ring: std::collections::VecDeque<Vec<u8>> =
        std::collections::VecDeque::with_capacity((n as usize).saturating_add(1).min(1 << 20));
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let r = reader.read_until(b'\n', &mut buf)?;
        if r == 0 {
            break;
        }
        if ring.len() as u64 >= n {
            ring.pop_front();
        }
        ring.push_back(buf.clone());
    }
    for line in ring {
        out.write_all(&line)?;
    }
    out.flush()
}

/// Print the last `n` bytes of `path` (or stdin, for `-`) to `out`.
fn tail_bytes(path: &str, n: u64, out: &mut impl Write) -> io::Result<()> {
    if path == "-" {
        let mut data = Vec::new();
        io::stdin().read_to_end(&mut data)?;
        let start = data.len().saturating_sub(n as usize);
        out.write_all(&data[start..])?;
        return out.flush();
    }
    let mut file = File::open(Path::new(path))?;
    let len = file.seek(SeekFrom::End(0))?;
    let start = len.saturating_sub(n);
    file.seek(SeekFrom::Start(start))?;
    io::copy(&mut file, out)?;
    out.flush()
}

/// Poll `path` for appended data (like `tail -f`) and stream it to `out`,
/// re-seeking to the start if the file shrinks (was truncated/rotated).
/// Never returns on success — only on an I/O error.
fn follow_file(path: &str, out: &mut impl Write) -> io::Result<()> {
    use std::thread;
    use std::time::Duration;
    let mut file = File::open(Path::new(path))?;
    file.seek(SeekFrom::End(0))?;
    let mut buf = [0u8; 4096];
    loop {
        match file.read(&mut buf) {
            Ok(0) => {
                thread::sleep(Duration::from_millis(200));
                // re-check size for truncation
                let pos = file.stream_position()?;
                let meta = std::fs::metadata(path)?;
                if meta.len() < pos {
                    file.seek(SeekFrom::Start(0))?;
                }
            }
            Ok(n) => {
                out.write_all(&buf[..n])?;
                out.flush()?;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parse_count_accepts_plain_and_plus_prefixed() {
        assert_eq!(parse_count("10").unwrap(), 10);
        assert_eq!(parse_count("+10").unwrap(), 10);
    }

    #[test]
    fn parse_count_rejects_garbage() {
        assert!(parse_count("abc").is_err());
        assert!(parse_count("").is_err());
    }

    #[test]
    fn tail_lines_from_returns_last_n_lines() {
        let input = Cursor::new(b"one\ntwo\nthree\nfour\n".to_vec());
        let mut out = Vec::new();
        tail_lines_from(input, 2, &mut out).unwrap();
        assert_eq!(out, b"three\nfour\n");
    }

    #[test]
    fn tail_lines_from_n_larger_than_input_returns_all() {
        let input = Cursor::new(b"one\ntwo\n".to_vec());
        let mut out = Vec::new();
        tail_lines_from(input, 100, &mut out).unwrap();
        assert_eq!(out, b"one\ntwo\n");
    }

    #[test]
    fn tail_lines_from_zero_returns_nothing() {
        let input = Cursor::new(b"one\ntwo\n".to_vec());
        let mut out = Vec::new();
        tail_lines_from(input, 0, &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn tail_bytes_file_returns_last_n_bytes() {
        let p = std::env::temp_dir().join(format!("user_tail_test_{}_bytes", std::process::id()));
        std::fs::write(&p, b"0123456789").unwrap();
        let mut out = Vec::new();
        tail_bytes(p.to_str().unwrap(), 4, &mut out).unwrap();
        assert_eq!(out, b"6789");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn tail_bytes_missing_file_errors() {
        let missing = format!("/nonexistent_user_tail_test_{}", std::process::id());
        let mut out = Vec::new();
        assert!(tail_bytes(&missing, 4, &mut out).is_err());
    }

    #[test]
    fn tail_lines_missing_file_errors() {
        let missing = format!("/nonexistent_user_tail_test_lines_{}", std::process::id());
        let mut out = Vec::new();
        assert!(tail_lines(&missing, 4, &mut out).is_err());
    }
}
