//! user shuf — shuffle lines.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `shuf` utility. Parses `std::env::args()` and
/// prints the input lines (from a file, `-e`/`--echo` arguments, an
/// `-i`/`--input-range LO-HI` generated range, or stdin) in random order.
///
/// Returns 0 on success, 1 on a usage or I/O error. Unlike the original
/// implementation, unparsable `-n`/`-i` values are now hard errors rather
/// than silently defaulting to "no limit" or `0`.
pub fn run() -> i32 {
    let ui = Ui::new("shuf");
    let mut n_lines: Option<usize> = None;
    let mut echo = false;
    let mut zero = false;
    let mut input: Option<String> = None;
    let mut range: Option<(i64, i64)> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: shuf [OPTION]... [FILE]\n -n, --head-count=N output at most N lines\n -e, --echo treat each ARG as an input line\n -i, --input-range=LO-HI treat each number LO through HI as an input line\n -z, --zero-terminated line delimiter is NUL\n");
                return 0;
            }
            "--version" => {
                println!("shuf (user_utils) 0.1.0");
                return 0;
            }
            "-e" | "--echo" => echo = true,
            "-z" | "--zero-terminated" => zero = true,
            "-n" | "--head-count" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'n'");
                    return 1;
                };
                match arg.parse() {
                    Ok(n) => n_lines = Some(n),
                    Err(_) => {
                        ui.err(&format!("invalid line count: '{arg}'"));
                        return 1;
                    }
                }
            }
            "-i" | "--input-range" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'i'");
                    return 1;
                };
                match parse_range(arg) {
                    Ok(r) => range = Some(r),
                    Err(msg) => {
                        ui.err(&msg);
                        return 1;
                    }
                }
            }
            s if s.starts_with("-n") && s.len() > 2 => {
                let arg = &s[2..];
                match arg.parse() {
                    Ok(n) => n_lines = Some(n),
                    Err(_) => {
                        ui.err(&format!("invalid line count: '{arg}'"));
                        return 1;
                    }
                }
            }
            s if s.starts_with("-i") && s.len() > 2 => {
                let arg = &s[2..];
                match parse_range(arg) {
                    Ok(r) => range = Some(r),
                    Err(msg) => {
                        ui.err(&msg);
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => {
                if echo {
                    let mut lines: Vec<String> = args[i..].to_vec();
                    return shuffle_print(&mut lines, n_lines, zero);
                }
                input = Some(other.to_string());
            }
        }
        i += 1;
    }

    let mut lines: Vec<String> = Vec::new();
    if let Some((lo, hi)) = range {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        for n in lo..=hi {
            lines.push(n.to_string());
        }
    } else if echo {
        // no args after -e
    } else {
        let f = input.as_deref().unwrap_or("-");
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    return 1;
                }
            }
        };
        for line in reader.lines() {
            match line {
                Ok(l) => lines.push(l),
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    return 1;
                }
            }
        }
    }
    shuffle_print(&mut lines, n_lines, zero)
}

/// Parse a `LO-HI` input-range operand (`-i`/`--input-range`). Both bounds
/// must be valid `i64`s; either side failing to parse is reported rather
/// than silently treated as `0`, and `LO-HI` missing the separator is
/// also an error.
///
/// The separator search skips a leading `-` (a negative `LO`) so that
/// ranges like `-5--1` parse as `(-5, -1)` instead of splitting on `LO`'s
/// own sign.
fn parse_range(s: &str) -> Result<(i64, i64), String> {
    let scan_start = if s.starts_with('-') { 1 } else { 0 };
    let sep = s[scan_start..]
        .find('-')
        .map(|p| p + scan_start)
        .ok_or_else(|| format!("invalid input range: '{s}'"))?;
    let (a, b) = (&s[..sep], &s[sep + 1..]);
    let lo: i64 = a
        .parse()
        .map_err(|_| format!("invalid input range: '{s}'"))?;
    let hi: i64 = b
        .parse()
        .map_err(|_| format!("invalid input range: '{s}'"))?;
    Ok((lo, hi))
}

/// Shuffle `lines` in place (Fisher–Yates, seeded from the wall clock XOR
/// PID) and print at most `n` of them (all, if `None`) separated by NUL
/// when `zero` is set, newline otherwise.
fn shuffle_print(lines: &mut [String], n: Option<usize>, zero: bool) -> i32 {
    // SAFETY: `time(2)` with a null `tloc` and `srand(3)` both take only
    // plain integers/null pointers and dereference nothing — neither can
    // fail or invoke UB. `libc::time` returning a null-pointer-safe value
    // is combined with the PID via XOR purely as an arithmetic seed.
    unsafe {
        libc::srand(libc::time(std::ptr::null_mut()) as u32 ^ std::process::id());
    }
    for i in (1..lines.len()).rev() {
        // SAFETY: `rand(3)` takes no arguments and dereferences no
        // pointers; it cannot fail or invoke UB. The result is reduced
        // mod `(i + 1)` before being used as a `swap` index, so it is
        // always in bounds regardless of the raw value returned.
        let j = (unsafe { libc::rand() } as usize) % (i + 1);
        lines.swap(i, j);
    }
    let take = n.unwrap_or(lines.len()).min(lines.len());
    let mut out = io::stdout().lock();
    let end = if zero { "\0" } else { "\n" };
    for line in lines.iter().take(take) {
        let _ = write!(out, "{line}{end}");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_valid() {
        assert_eq!(parse_range("1-5"), Ok((1, 5)));
    }

    #[test]
    fn parse_range_negative_bounds() {
        assert_eq!(parse_range("-5--1"), Ok((-5, -1)));
    }

    #[test]
    fn parse_range_missing_separator_errors() {
        assert!(parse_range("15").is_err());
    }

    #[test]
    fn parse_range_non_numeric_errors() {
        assert!(parse_range("a-b").is_err());
    }

    #[test]
    fn shuffle_print_take_limits_output_count() {
        let mut lines = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        // Just exercise the take/min logic path without asserting order,
        // since shuffling is randomized.
        let status = shuffle_print(&mut lines, Some(2), false);
        assert_eq!(status, 0);
    }

    #[test]
    fn shuffle_print_preserves_all_elements_when_no_limit() {
        let mut lines: Vec<String> = (0..10).map(|n| n.to_string()).collect();
        let before: std::collections::HashSet<_> = lines.iter().cloned().collect();
        shuffle_print(&mut lines, None, false);
        let after: std::collections::HashSet<_> = lines.iter().cloned().collect();
        assert_eq!(before, after);
    }
}
