//! user printf — format and print data.
use std::io::{self, Write};

use usercore::Ui;

/// Entry point for the `printf` utility. Parses `std::env::args()` as
/// `FORMAT [ARGUMENT]...`, expands `FORMAT`'s `\`-escapes and `%`-directives
/// against the given `ARGUMENT`s (cycling back through `FORMAT` if there are
/// more arguments than directives, as POSIX `printf` does), and writes the
/// result to stdout.
///
/// Returns 0 on success, 1 on a usage error, an I/O error, or if any
/// `ARGUMENT` could not be parsed as the numeric type its directive expected
/// (matching GNU `printf`, the malformed argument is treated as `0` and
/// formatting continues, but the process still exits with failure).
pub fn run() -> i32 {
    let ui = Ui::new("printf");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    match args[0].as_str() {
        "--help" | "-h" => {
            println!("Usage: printf FORMAT [ARGUMENT]...");
            return 0;
        }
        "--version" => {
            println!("printf (user_utils) 0.1.0");
            return 0;
        }
        _ => {}
    }
    let format = &args[0];
    let mut out = Vec::new();
    let mut rest: &[String] = &args[1..];
    let mut status = 0;
    if !format.as_bytes().contains(&b'%') {
        expand_literal(format.as_bytes(), &mut out);
    } else {
        loop {
            let used = format_once(format, rest, &mut out, &ui, &mut status);
            if used == 0 {
                break;
            }
            if rest.len() <= used {
                break;
            }
            rest = &rest[used..];
        }
    }
    if let Err(e) = io::stdout().write_all(&out) {
        if e.kind() != io::ErrorKind::BrokenPipe {
            ui.err(&format!("{e}"));
            return 1;
        }
    }
    status
}

/// Expand backslash escapes (`\n`, `\t`, `\r`, `\\`) in `s` into `out`,
/// leaving any other `\X` sequence untouched. Used for the fast path where
/// `FORMAT` contains no `%` directives at all.
fn expand_literal(s: &[u8], out: &mut Vec<u8>) {
    // interpret \\ escapes in format itself for non-% path
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' && i + 1 < s.len() {
            i += 1;
            match s[i] {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'\\' => out.push(b'\\'),
                other => {
                    out.push(b'\\');
                    out.push(other);
                }
            }
            i += 1;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
}

/// Expand one pass of `fmt` (its `\`-escapes and `%`-directives) against
/// `args`, appending the result to `out`. On a malformed numeric `ARGUMENT`,
/// warns via `ui`, sets `*status = 1`, and substitutes `0` rather than
/// aborting — matching GNU `printf`'s recovery behavior.
///
/// Returns the number of `args` consumed, so the caller can detect when
/// `fmt` has no more directives left to cycle through (a return of `0`).
fn format_once(fmt: &str, args: &[String], out: &mut Vec<u8>, ui: &Ui, status: &mut i32) -> usize {
    let b = fmt.as_bytes();
    let mut i = 0;
    let mut ai = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            i += 1;
            match b[i] {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'\\' => out.push(b'\\'),
                other => {
                    out.push(b'\\');
                    out.push(other);
                }
            }
            i += 1;
            continue;
        }
        if b[i] != b'%' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i < b.len() && b[i] == b'%' {
            out.push(b'%');
            i += 1;
            continue;
        }
        while i < b.len() && matches!(b[i], b'-' | b'+' | b' ' | b'#' | b'0') {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i >= b.len() {
            break;
        }
        let spec = b[i] as char;
        i += 1;
        let arg = args.get(ai).map(|s| s.as_str()).unwrap_or("");
        ai += 1;
        match spec {
            's' => out.extend_from_slice(arg.as_bytes()),
            'c' => {
                if let Some(c) = arg.chars().next() {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
            'd' | 'i' => {
                let n: i64 = parse_or_warn(arg, ui, status);
                out.extend_from_slice(n.to_string().as_bytes());
            }
            'u' => {
                let n: u64 = parse_or_warn(arg, ui, status);
                out.extend_from_slice(n.to_string().as_bytes());
            }
            'x' => out.extend_from_slice(format!("{:x}", parse_int(arg, ui, status)).as_bytes()),
            'X' => out.extend_from_slice(format!("{:X}", parse_int(arg, ui, status)).as_bytes()),
            'o' => out.extend_from_slice(format!("{:o}", parse_int(arg, ui, status)).as_bytes()),
            'f' => {
                let n: f64 = parse_or_warn(arg, ui, status);
                out.extend_from_slice(format!("{n:.6}").as_bytes());
            }
            'b' => expand_b(arg.as_bytes(), out),
            _ => {
                out.push(b'%');
                out.push(spec as u8);
            }
        }
    }
    ai
}

/// Parse `s` as `T`, or — if empty — silently yield `T::default()` (an
/// omitted trailing `ARGUMENT` is not an error in `printf`). A non-empty but
/// unparsable `s` warns via `ui`, sets `*status = 1`, and yields
/// `T::default()`, matching GNU `printf`'s "expected a numeric value"
/// recovery.
fn parse_or_warn<T: std::str::FromStr + Default>(s: &str, ui: &Ui, status: &mut i32) -> T {
    if s.is_empty() {
        return T::default();
    }
    match s.parse() {
        Ok(n) => n,
        Err(_) => {
            ui.err(&format!("'{s}': expected a numeric value"));
            *status = 1;
            T::default()
        }
    }
}

/// Parse `s` as an unsigned integer for `%x`/`%X`/`%o`, accepting an
/// optional `0x`/`0X` hex prefix. Same empty/malformed handling as
/// [`parse_or_warn`].
fn parse_int(s: &str, ui: &Ui, status: &mut i32) -> u64 {
    if s.is_empty() {
        return 0;
    }
    let parsed = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    };
    match parsed {
        Some(n) => n,
        None => {
            ui.err(&format!("'{s}': expected a numeric value"));
            *status = 1;
            0
        }
    }
}

/// Expand `%b`-style backslash escapes in `s` into `out`; unlike
/// [`expand_literal`], `\c` here means "stop output immediately" (matching
/// POSIX `printf`'s `%b` semantics), and unrecognized `\X` sequences drop
/// the backslash (again, `%b`-specific — differs from the plain-text path).
fn expand_b(s: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < s.len() {
        if s[i] != b'\\' || i + 1 >= s.len() {
            out.push(s[i]);
            i += 1;
            continue;
        }
        i += 1;
        match s[i] {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'\\' => out.push(b'\\'),
            b'c' => return, // stop
            other => out.push(other),
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(fmt: &str, args: &[&str]) -> (String, i32) {
        let ui = Ui::with_color("printf", false);
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let mut status = 0;
        let mut rest: &[String] = &args;
        loop {
            let used = format_once(fmt, rest, &mut out, &ui, &mut status);
            if used == 0 || rest.len() <= used {
                break;
            }
            rest = &rest[used..];
        }
        (String::from_utf8(out).unwrap(), status)
    }

    #[test]
    fn literal_text_passthrough() {
        let mut out = Vec::new();
        expand_literal(b"hello\\nworld", &mut out);
        assert_eq!(out, b"hello\nworld");
    }

    #[test]
    fn percent_s_and_percent_percent() {
        let (s, status) = expand("[%s] 100%%\n", &["hi"]);
        assert_eq!(s, "[hi] 100%\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn percent_d_cycles_format_over_extra_args() {
        let (s, status) = expand("%d\n", &["1", "2", "3"]);
        assert_eq!(s, "1\n2\n3\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn percent_x_hex_output_with_0x_prefix_input() {
        let (s, status) = expand("%x\n", &["0xff"]);
        assert_eq!(s, "ff\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn invalid_numeric_argument_defaults_to_zero_and_flags_status() {
        let (s, status) = expand("%d\n", &["notanumber"]);
        assert_eq!(s, "0\n");
        assert_eq!(status, 1);
    }

    #[test]
    fn missing_trailing_argument_is_not_an_error() {
        let (s, status) = expand("%s-%d\n", &["only"]);
        assert_eq!(s, "only-0\n");
        assert_eq!(status, 0);
    }

    #[test]
    fn percent_b_stops_at_backslash_c() {
        let mut out = Vec::new();
        expand_b(b"abc\\cdef", &mut out);
        assert_eq!(out, b"abc");
    }

    #[test]
    fn percent_f_formats_six_decimals() {
        let (s, status) = expand("%f\n", &["3.5"]);
        assert_eq!(s, "3.500000\n");
        assert_eq!(status, 0);
    }
}
