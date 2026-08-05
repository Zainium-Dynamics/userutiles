//! user echo — GNU-compatible echo with -n/-e/-E.
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;

use usercore::Ui;

/// Entry point for the `echo` utility. Parses `std::env::args_os()`,
/// honoring the leading `-n`/`-e`/`-E` option cluster (GNU echo scans only
/// a prefix of pure `-[neE]*` tokens; the first non-matching argument ends
/// option parsing), then prints the remaining arguments space-joined,
/// interpreting backslash escapes when `-e` (or `POSIXLY_CORRECT`) is
/// active.
///
/// Always returns 0, unless writing to stdout fails for a reason other
/// than a broken pipe.
pub fn run() -> i32 {
    let ui = Ui::new("echo");
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let byte_args: Vec<&[u8]> = args.iter().map(|a| a.as_bytes()).collect();
    let posix_escape_default = std::env::var_os("POSIXLY_CORRECT").is_some();
    let (mut trailing_nl, escape, start) =
        parse_leading_options(&byte_args, posix_escape_default);

    let mut out = Vec::new();
    for (j, arg) in byte_args[start..].iter().enumerate() {
        if j > 0 {
            out.push(b' ');
        }
        if escape {
            if !append_escaped(&mut out, arg) {
                // \c — suppress rest + newline
                trailing_nl = false;
                break;
            }
        } else {
            out.extend_from_slice(arg);
        }
    }
    if trailing_nl {
        out.push(b'\n');
    }
    if let Err(e) = write_all_stdout(&out) {
        if e.kind() != io::ErrorKind::BrokenPipe {
            ui.err(&format!("{e}"));
            return 1;
        }
    }
    0
}

/// Scan the leading `-n`/`-e`/`-E` option cluster in `args`. Each argument
/// starting with `-` (but not bare `-`) whose remaining characters are all
/// `n`, `e`, or `E` toggles the corresponding flag; the first argument
/// that doesn't fit that shape ends option scanning.
///
/// Returns `(trailing_newline, escape_enabled, index_of_first_word)`.
fn parse_leading_options(args: &[&[u8]], posix_escape_default: bool) -> (bool, bool, usize) {
    let mut trailing_nl = true;
    let mut escape = posix_escape_default;
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if a.first() != Some(&b'-') || a == b"-" {
            break;
        }
        let mut opts = (trailing_nl, escape);
        let mut ok = true;
        for &c in &a[1..] {
            match c {
                b'n' => opts.0 = false,
                b'e' => opts.1 = true,
                b'E' => opts.1 = false,
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || a.len() == 1 {
            break;
        }
        trailing_nl = opts.0;
        escape = opts.1;
        i += 1;
    }
    (trailing_nl, escape, i)
}

/// Append `s` to `out`, expanding backslash escapes (`\n`, `\t`, `\0NNN`,
/// `\xHH`, ...) as GNU `echo -e` does. Returns `false` if a `\c` escape was
/// seen, signalling that the caller should stop processing immediately
/// (including suppressing the trailing newline).
fn append_escaped(out: &mut Vec<u8>, s: &[u8]) -> bool {
    let mut i = 0;
    while i < s.len() {
        if s[i] != b'\\' || i + 1 >= s.len() {
            out.push(s[i]);
            i += 1;
            continue;
        }
        i += 1;
        match s[i] {
            b'\\' => out.push(b'\\'),
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'c' => return false,
            b'e' | b'E' => out.push(0x1b),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'0' => {
                // up to 3 octal digits including this 0? GNU: \0nnn octal
                let mut val = 0u8;
                let mut count = 0;
                while count < 3 && i + 1 < s.len() && s[i + 1].is_ascii_digit() && s[i + 1] <= b'7'
                {
                    i += 1;
                    val = val.wrapping_mul(8).wrapping_add(s[i] - b'0');
                    count += 1;
                }
                if count == 0 {
                    out.push(0);
                } else {
                    out.push(val);
                }
            }
            b'x' => {
                let mut val = 0u8;
                let mut count = 0;
                while count < 2 && i + 1 < s.len() && s[i + 1].is_ascii_hexdigit() {
                    i += 1;
                    let d = s[i];
                    val = val.wrapping_mul(16).wrapping_add(match d {
                        b'0'..=b'9' => d - b'0',
                        b'a'..=b'f' => d - b'a' + 10,
                        b'A'..=b'F' => d - b'A' + 10,
                        _ => 0,
                    });
                    count += 1;
                }
                if count == 0 {
                    out.push(b'\\');
                    out.push(b'x');
                } else {
                    out.push(val);
                }
            }
            other => {
                out.push(b'\\');
                out.push(other);
            }
        }
        i += 1;
    }
    true
}

/// Write `bytes` to stdout, mapping a broken pipe (e.g. `echo ... | head`)
/// to success rather than an error, so pipelines don't trip a nonzero exit.
fn write_all_stdout(bytes: &[u8]) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(args: &[&str]) -> (bool, bool, usize) {
        let bytes: Vec<&[u8]> = args.iter().map(|s| s.as_bytes()).collect();
        parse_leading_options(&bytes, false)
    }

    #[test]
    fn parse_leading_options_no_flags() {
        assert_eq!(opts(&["hello", "world"]), (true, false, 0));
    }

    #[test]
    fn parse_leading_options_dash_n_suppresses_newline() {
        assert_eq!(opts(&["-n", "hi"]), (false, false, 1));
    }

    #[test]
    fn parse_leading_options_dash_e_enables_escape() {
        assert_eq!(opts(&["-e", "hi"]), (true, true, 1));
    }

    #[test]
    fn parse_leading_options_combined_flags() {
        assert_eq!(opts(&["-ne", "hi"]), (false, true, 1));
    }

    #[test]
    fn parse_leading_options_bare_dash_is_a_word() {
        assert_eq!(opts(&["-", "hi"]), (true, false, 0));
    }

    #[test]
    fn parse_leading_options_stops_at_non_flag_word() {
        assert_eq!(opts(&["-n", "-x", "hi"]), (false, false, 1));
    }

    #[test]
    fn parse_leading_options_empty_args() {
        assert_eq!(opts(&[]), (true, false, 0));
    }

    #[test]
    fn parse_leading_options_posix_default_enables_escape() {
        let bytes: Vec<&[u8]> = vec![b"hi"];
        assert_eq!(parse_leading_options(&bytes, true), (true, true, 0));
    }

    #[test]
    fn append_escaped_handles_common_escapes() {
        let mut out = Vec::new();
        assert!(append_escaped(&mut out, b"a\\tb\\nc"));
        assert_eq!(out, b"a\tb\nc");
    }

    #[test]
    fn append_escaped_c_stops_processing() {
        let mut out = Vec::new();
        assert!(!append_escaped(&mut out, b"abc\\cdef"));
        assert_eq!(out, b"abc");
    }

    #[test]
    fn append_escaped_hex_and_octal() {
        let mut out = Vec::new();
        // \x41 is hex 'A'; \0101 is octal 101 = 65 = 'A' (octal escapes
        // require the leading `0`, unlike hex's `x`).
        assert!(append_escaped(&mut out, b"\\x41\\0101"));
        assert_eq!(out, b"AA");
    }

    #[test]
    fn append_escaped_unknown_escape_passes_through() {
        let mut out = Vec::new();
        assert!(append_escaped(&mut out, b"\\q"));
        assert_eq!(out, b"\\q");
    }

    #[test]
    fn append_escaped_trailing_backslash_is_literal() {
        let mut out = Vec::new();
        assert!(append_escaped(&mut out, b"abc\\"));
        assert_eq!(out, b"abc\\");
    }
}
