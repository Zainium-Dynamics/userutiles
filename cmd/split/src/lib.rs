//! user split — split a file into pieces.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use usercore::Ui;

/// Entry point for the `split` utility. Parses `std::env::args()` and
/// splits the input (a named file, or stdin when `-`/omitted) into
/// `PREFIXaa`, `PREFIXab`, ... files, either `-l` lines or `-b` bytes per
/// piece (defaulting to 1000 lines).
///
/// Returns 0 on success, 1 on a usage or I/O error. Unlike the original
/// implementation, an unparsable `-l`/`-b` argument is now a hard error
/// rather than silently falling back to the default.
pub fn run() -> i32 {
    let ui = Ui::new("split");
    let mut lines: Option<u64> = None;
    let mut bytes: Option<u64> = None;
    let mut prefix = "x".to_string();
    let mut input = "-".to_string();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut positionals = Vec::new();
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: split [OPTION]... [FILE [PREFIX]]\nOutput pieces of FILE to PREFIXaa, PREFIXab, ...\n -l, --lines=N put N lines/records per output file (default 1000)\n -b, --bytes=N put N bytes per output file\n");
                return 0;
            }
            "--version" => {
                println!("split (user_utils) 0.1.0");
                return 0;
            }
            "-l" | "--lines" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'l'");
                    return 1;
                };
                let Some(n) = parse_count(arg) else {
                    ui.err(&format!("invalid number of lines: '{arg}'"));
                    return 1;
                };
                lines = Some(n);
                bytes = None;
            }
            "-b" | "--bytes" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'b'");
                    return 1;
                };
                let Some(n) = parse_bytes(arg) else {
                    ui.err(&format!("invalid number of bytes: '{arg}'"));
                    return 1;
                };
                bytes = Some(n);
                lines = None;
            }
            s if s.starts_with("-l") && s.len() > 2 => {
                let arg = &s[2..];
                let Some(n) = parse_count(arg) else {
                    ui.err(&format!("invalid number of lines: '{arg}'"));
                    return 1;
                };
                lines = Some(n);
                bytes = None;
            }
            s if s.starts_with("-b") && s.len() > 2 => {
                let arg = &s[2..];
                let Some(n) = parse_bytes(arg) else {
                    ui.err(&format!("invalid number of bytes: '{arg}'"));
                    return 1;
                };
                bytes = Some(n);
                lines = None;
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => positionals.push(other.to_string()),
        }
        i += 1;
    }
    if !positionals.is_empty() {
        input = positionals[0].clone();
    }
    if positionals.len() > 1 {
        prefix = positionals[1].clone();
    }

    let mut reader: Box<dyn Read> = if input == "-" {
        Box::new(io::stdin())
    } else {
        match File::open(&input) {
            Ok(f) => Box::new(f),
            Err(e) => {
                ui.err(&format!("{input}: {e}"));
                return 1;
            }
        }
    };

    let mut suffix_a = b'a';
    let mut suffix_b = b'a';

    if let Some(bsz) = bytes {
        let mut buf = vec![0u8; bsz as usize];
        loop {
            let mut filled = 0;
            while filled < buf.len() {
                match reader.read(&mut buf[filled..]) {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(e) => {
                        ui.err(&format!("{e}"));
                        return 1;
                    }
                }
            }
            if filled == 0 {
                break;
            }
            let name = next_name(&prefix, &mut suffix_a, &mut suffix_b);
            if let Err(e) = File::create(&name).and_then(|mut f| f.write_all(&buf[..filled])) {
                ui.err(&format!("{}: {e}", name.display()));
                return 1;
            }
        }
    } else {
        let nlines = lines.unwrap_or(1000);
        let mut breader = BufReader::new(reader);
        loop {
            let name = next_name(&prefix, &mut suffix_a, &mut suffix_b);
            let mut out = match File::create(&name) {
                Ok(f) => f,
                Err(e) => {
                    ui.err(&format!("{}: {e}", name.display()));
                    return 1;
                }
            };
            let mut count = 0u64;
            let mut line = Vec::new();
            while count < nlines {
                line.clear();
                match breader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if let Err(e) = out.write_all(&line) {
                            ui.err(&format!("{}: {e}", name.display()));
                            return 1;
                        }
                        count += 1;
                    }
                    Err(e) => {
                        ui.err(&format!("{e}"));
                        return 1;
                    }
                }
            }
            if count == 0 {
                remove_if_empty(&name);
                break;
            }
        }
    }
    0
}

/// Compute the next `PREFIXaa`, `PREFIXab`, ... output path and advance
/// the two-letter suffix counter (wraps `b` at `z`, carrying into `a`).
fn next_name(pref: &str, a: &mut u8, b: &mut u8) -> PathBuf {
    let name = format!("{pref}{}{}", *a as char, *b as char);
    *b += 1;
    if *b > b'z' {
        *b = b'a';
        *a += 1;
    }
    PathBuf::from(name)
}

/// Parse a plain positive line/record count (`-l N`). Zero is rejected
/// (an infinite stream of empty output files is never useful and GNU
/// `split` rejects it too), and anything that doesn't fully parse as a
/// `u64` is rejected rather than silently falling back to a default.
fn parse_count(s: &str) -> Option<u64> {
    let n: u64 = s.parse().ok()?;
    if n == 0 { None } else { Some(n) }
}

/// Parse a `-b`-style byte count with an optional `K`/`M`/`G` (powers of
/// 1024) suffix, e.g. `512`, `10K`, `4M`. Zero and multiplication
/// overflow are both rejected rather than silently wrapping or defaulting.
fn parse_bytes(s: &str) -> Option<u64> {
    let last = *s.as_bytes().last()?;
    let (digits, mult) = if last.is_ascii_digit() {
        (s, 1u64)
    } else {
        let mult = match last.to_ascii_uppercase() {
            b'K' => 1024u64,
            b'M' => 1024 * 1024,
            b'G' => 1024 * 1024 * 1024,
            _ => return None,
        };
        (&s[..s.len() - 1], mult)
    };
    let n: u64 = digits.parse().ok()?;
    if n == 0 {
        return None;
    }
    n.checked_mul(mult)
}

/// Remove `p` if it exists and is a zero-length file — cleans up the
/// trailing empty chunk `split` creates before discovering EOF.
fn remove_if_empty(p: &PathBuf) {
    if let Ok(m) = std::fs::metadata(p) {
        if m.len() == 0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_count_plain() {
        assert_eq!(parse_count("42"), Some(42));
    }

    #[test]
    fn parse_count_rejects_zero() {
        assert_eq!(parse_count("0"), None);
    }

    #[test]
    fn parse_count_rejects_garbage() {
        assert_eq!(parse_count("abc"), None);
        assert_eq!(parse_count("12x"), None);
        assert_eq!(parse_count(""), None);
    }

    #[test]
    fn parse_bytes_plain_digits() {
        assert_eq!(parse_bytes("512"), Some(512));
    }

    #[test]
    fn parse_bytes_kilo_mega_giga_suffixes() {
        assert_eq!(parse_bytes("10K"), Some(10 * 1024));
        assert_eq!(parse_bytes("2M"), Some(2 * 1024 * 1024));
        assert_eq!(parse_bytes("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_bytes("1k"), Some(1024)); // lowercase accepted
    }

    #[test]
    fn parse_bytes_rejects_zero() {
        assert_eq!(parse_bytes("0"), None);
        assert_eq!(parse_bytes("0K"), None);
    }

    #[test]
    fn parse_bytes_rejects_unknown_suffix() {
        assert_eq!(parse_bytes("10X"), None);
    }

    #[test]
    fn parse_bytes_rejects_empty() {
        assert_eq!(parse_bytes(""), None);
    }

    #[test]
    fn parse_bytes_rejects_overflow() {
        // u64::MAX worth of gigabytes overflows on multiply by 1024^3.
        assert_eq!(parse_bytes("99999999999999999999G"), None);
    }

    #[test]
    fn next_name_increments_and_carries() {
        let mut a = b'a';
        let mut b = b'y';
        let n1 = next_name("x", &mut a, &mut b);
        assert_eq!(n1, PathBuf::from("xay"));
        let n2 = next_name("x", &mut a, &mut b);
        assert_eq!(n2, PathBuf::from("xaz"));
        let n3 = next_name("x", &mut a, &mut b);
        assert_eq!(n3, PathBuf::from("xba"));
    }

    #[test]
    fn remove_if_empty_deletes_zero_length_file() {
        let dir = std::env::temp_dir().join(format!("user_split_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty");
        File::create(&path).unwrap();
        remove_if_empty(&path);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_if_empty_keeps_nonempty_file() {
        let dir = std::env::temp_dir().join(format!("user_split_test2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nonempty");
        std::fs::write(&path, b"data").unwrap();
        remove_if_empty(&path);
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
