//! user printf — format and print data.
use std::ffi::{CStr, CString};
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
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' {
            let (next, stop) = escape(s, i, out, false);
            if stop {
                return;
            }
            i = next;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
}

/// The escape at `b[i]` (a backslash): its bytes go to `out`. Returns the index after it,
/// and whether it was `\c` (in `%b`: produce no further output). In `%b` an octal escape
/// is `\0NNN`; in the format it is `\NNN`.
fn escape(b: &[u8], i: usize, out: &mut Vec<u8>, in_b: bool) -> (usize, bool) {
    let mut j = i + 1;
    let Some(&c) = b.get(j) else {
        out.push(b'\\');
        return (j, false);
    };
    j += 1;
    let digits = |j: &mut usize, max: usize, radix: u32| -> Option<u32> {
        let mut v = None;
        for _ in 0..max {
            match b.get(*j).and_then(|d| (*d as char).to_digit(radix)) {
                Some(d) => {
                    v = Some(v.unwrap_or(0) * radix + d);
                    *j += 1;
                }
                None => break,
            }
        }
        v
    };
    match c {
        b'n' => out.push(b'\n'),
        b't' => out.push(b'\t'),
        b'r' => out.push(b'\r'),
        b'\\' => out.push(b'\\'),
        b'a' => out.push(7),
        b'b' => out.push(8),
        b'f' => out.push(12),
        b'v' => out.push(11),
        b'"' => out.push(b'"'),
        b'\'' => out.push(b'\''),
        b'c' if in_b => return (j, true),
        b'x' => match digits(&mut j, 2, 16) {
            Some(v) => out.push(v as u8),
            None => out.extend_from_slice(b"\\x"),
        },
        b'u' | b'U' => {
            let n = if c == b'u' { 4 } else { 8 };
            match digits(&mut j, n, 16).and_then(char::from_u32) {
                Some(ch) => out.extend_from_slice(ch.encode_utf8(&mut [0u8; 4]).as_bytes()),
                None => out.extend_from_slice(&[b'\\', c]),
            }
        }
        b'0'..=b'7' => {
            j -= 1;
            if in_b && c == b'0' {
                j += 1;
            }
            out.push(digits(&mut j, 3, 8).unwrap_or(0) as u8);
        }
        other => {
            out.push(b'\\');
            out.push(other);
        }
    }
    (j, false)
}

/// What a conversion gets from its argument, formatted by C's printf.
enum CArg<'a> {
    Int(i64),
    Uint(u64),
    Float(f64),
    Str(&'a CStr),
}

/// `spec` (a C conversion like `%-08.3lld`) applied to `arg` by snprintf: exactly the
/// printf(1) rules for flags, width and precision.
fn c_format(spec: &str, arg: CArg) -> Vec<u8> {
    let Ok(f) = CString::new(spec) else { return Vec::new() };
    let mut buf = vec![0u8; 256];
    loop {
        let p = buf.as_mut_ptr() as *mut libc::c_char;
        // SAFETY: buf has buf.len() bytes, f is NUL-terminated and the argument matches the
        // conversion the caller built (lld / llu / a double conversion / s).
        let n = unsafe {
            match arg {
                CArg::Int(v) => libc::snprintf(p, buf.len(), f.as_ptr(), v as libc::c_longlong),
                CArg::Uint(v) => libc::snprintf(p, buf.len(), f.as_ptr(), v as libc::c_ulonglong),
                CArg::Float(v) => libc::snprintf(p, buf.len(), f.as_ptr(), v as libc::c_double),
                CArg::Str(v) => libc::snprintf(p, buf.len(), f.as_ptr(), v.as_ptr()),
            }
        };
        if n < 0 {
            return Vec::new();
        }
        if (n as usize) < buf.len() {
            buf.truncate(n as usize);
            return buf;
        }
        buf.resize(n as usize + 1, 0);
    }
}

/// Shell-quoted `s` (`%q`): unchanged when it needs no quoting, else in single quotes.
fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"_./-=,:+@%^".contains(&c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A numeric argument as printf(1) reads it: decimal, 0x hex, 0 octal, or 'c / "c for
/// the character's value. A trailing part that isn't a number is an error, the number
/// before it is still used.
fn int_arg(s: &str, ui: &Ui, status: &mut i32) -> i64 {
    let t = s.trim_start();
    if t.is_empty() {
        return 0;
    }
    if let Some(rest) = t.strip_prefix(['\'', '"']) {
        return rest.chars().next().map_or(0, |c| c as i64);
    }
    let (neg, body) = match t.as_bytes()[0] {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let (radix, digits) = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        (16, h)
    } else if body.len() > 1 && body.starts_with('0') {
        (8, &body[1..])
    } else {
        (10, body)
    };
    let end = digits.find(|c: char| !c.is_digit(radix)).unwrap_or(digits.len());
    let v = i64::from_str_radix(&digits[..end], radix).unwrap_or(if end == 0 { 0 } else { i64::MAX });
    if end == 0 && radix == 10 {
        ui.err(&format!("'{s}': expected a numeric value"));
        *status = 1;
    } else if end < digits.len() {
        ui.err(&format!("'{s}': value not completely converted"));
        *status = 1;
    }
    if neg { v.wrapping_neg() } else { v }
}

fn float_arg(s: &str, ui: &Ui, status: &mut i32) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    if let Some(rest) = t.strip_prefix(['\'', '"']) {
        return rest.chars().next().map_or(0.0, |c| c as u32 as f64);
    }
    match t.parse::<f64>() {
        Ok(v) => v,
        Err(_) => {
            ui.err(&format!("'{s}': expected a numeric value"));
            *status = 1;
            0.0
        }
    }
}

/// One pass over the format, taking arguments from `args`. Returns how many it used, or
/// `usize::MAX` after `\c` in a `%b` argument (no further output at all).
fn format_once(fmt: &str, args: &[String], out: &mut Vec<u8>, ui: &Ui, status: &mut i32) -> usize {
    let b = fmt.as_bytes();
    let mut i = 0;
    let mut ai = 0;
    let next = |ai: &mut usize| -> &str {
        let a = args.get(*ai).map(|s| s.as_str()).unwrap_or("");
        *ai += 1;
        a
    };
    while i < b.len() {
        if b[i] == b'\\' {
            i = escape(b, i, out, false).0;
            continue;
        }
        if b[i] != b'%' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        if b.get(i) == Some(&b'%') {
            out.push(b'%');
            i += 1;
            continue;
        }
        let mut spec = String::from("%");
        while let Some(&f) = b.get(i).filter(|f| b"-+ #0".contains(f)) {
            spec.push(f as char);
            i += 1;
        }
        if b.get(i) == Some(&b'*') {
            i += 1;
            spec += &int_arg(next(&mut ai), ui, status).to_string();
        } else {
            while let Some(&d) = b.get(i).filter(|d| d.is_ascii_digit()) {
                spec.push(d as char);
                i += 1;
            }
        }
        if b.get(i) == Some(&b'.') {
            i += 1;
            spec.push('.');
            if b.get(i) == Some(&b'*') {
                i += 1;
                spec += &int_arg(next(&mut ai), ui, status).max(0).to_string();
            } else {
                while let Some(&d) = b.get(i).filter(|d| d.is_ascii_digit()) {
                    spec.push(d as char);
                    i += 1;
                }
            }
        }
        while b.get(i).is_some_and(|c| b"hlLjzt".contains(c)) {
            i += 1;
        }
        let Some(&conv) = b.get(i) else {
            out.extend_from_slice(&b[start..]);
            break;
        };
        i += 1;
        let arg = next(&mut ai);
        match conv {
            b'd' | b'i' => out.extend(c_format(&format!("{spec}lld"), CArg::Int(int_arg(arg, ui, status)))),
            b'u' | b'o' | b'x' | b'X' => out.extend(c_format(
                &format!("{spec}ll{}", conv as char),
                CArg::Uint(int_arg(arg, ui, status) as u64),
            )),
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' | b'a' | b'A' => out.extend(c_format(
                &format!("{spec}{}", conv as char),
                CArg::Float(float_arg(arg, ui, status)),
            )),
            b'c' => {
                let first = arg.as_bytes().first().map_or(String::new(), |c| (*c as char).to_string());
                let c = CString::new(first).unwrap_or_default();
                out.extend(c_format(&format!("{spec}s"), CArg::Str(&c)));
            }
            b's' | b'b' | b'q' => {
                let mut text = Vec::new();
                let mut stop = false;
                match conv {
                    b's' => text.extend_from_slice(arg.as_bytes()),
                    b'q' => text.extend_from_slice(shell_quote(arg).as_bytes()),
                    _ => {
                        let a = arg.as_bytes();
                        let mut k = 0;
                        while k < a.len() && !stop {
                            if a[k] == b'\\' {
                                let (n, s) = escape(a, k, &mut text, true);
                                k = n;
                                stop = s;
                            } else {
                                text.push(a[k]);
                                k += 1;
                            }
                        }
                    }
                }
                // a NUL from %b ends the C string: write the bytes when no width applies
                if spec == "%" || text.contains(&0) {
                    out.extend_from_slice(&text);
                } else {
                    let t = CString::new(text).unwrap_or_default();
                    out.extend(c_format(&format!("{spec}s"), CArg::Str(&t)));
                }
                if stop {
                    return usize::MAX;
                }
            }
            other => {
                ui.err(&format!("%{}: invalid conversion specification", other as char));
                *status = 1;
                return usize::MAX;
            }
        }
    }
    ai
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
    fn width_precision_and_float_conversions() {
        assert_eq!(expand("%05.2f|%-6s|%6s\n", &["3.14159", "ab", "cd"]).0, "03.14|ab    |    cd\n");
        assert_eq!(expand("%x %X %o %e\n", &["255", "255", "8", "1234.5"]).0, "ff FF 10 1.234500e+03\n");
        assert_eq!(expand("%*d|%10.3s|\n", &["5", "42", "abcdef"]).0, "   42|       abc|\n");
        assert_eq!(expand("%q", &["a b"]).0, "'a b'");
        assert_eq!(expand("\\x41\\101%d", &["'A"]).0, "AA65");
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
        let (s, _) = expand("%b|", &["abc\\cdef"]);
        assert_eq!(s, "abc");
    }

    #[test]
    fn percent_f_formats_six_decimals() {
        let (s, status) = expand("%f\n", &["3.5"]);
        assert_eq!(s, "3.500000\n");
        assert_eq!(status, 0);
    }
}
