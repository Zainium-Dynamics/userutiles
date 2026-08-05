//! user od — dump files in octal and other formats.
use std::fs::File;
use std::io::{self, Read, Write};

use usercore::Ui;

/// Entry point for the `od` utility. Parses `std::env::args()` as
/// `[OPTION]... [FILE]...` (reading stdin if none are given) and writes an
/// unambiguous octal/decimal/hex/character dump of the concatenated input
/// to stdout.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("od");
    let mut address_radix = 'o'; // o d x n
    let mut fmt = 'o'; // o d x c
    let mut width = 2usize; // bytes per unit for integer formats
    let mut files: Vec<String> = Vec::new();
    let mut skip = 0u64;
    let mut limit: Option<u64> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: od [OPTION]... [FILE]...\n\
 Write an unambiguous representation of FILE.\n\
 -A RADIX offset base: o|d|x|n\n\
 -t TYPE select output format: o1 o2 o4 d1 d2 d4 x1 x2 x4 c\n\
 -j BYTES skip BYTES input bytes first\n\
 -N BYTES limit dump to BYTES\n\
 -An -tx1 -tc traditional style shortcuts via -t\n"
                );
                return 0;
            }
            "--version" => {
                println!("od (user_utils) 0.1.0");
                return 0;
            }
            "-A" => {
                i += 1;
                address_radix = args.get(i).and_then(|s| s.chars().next()).unwrap_or('o');
            }
            s if s.starts_with("-A") && s.len() > 2 => {
                address_radix = s.chars().nth(2).unwrap_or('o');
            }
            "-t" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("o2");
                if let Err(e) = parse_type(spec, &mut fmt, &mut width) {
                    ui.err(&e);
                    return 1;
                }
            }
            s if s.starts_with("-t") && s.len() > 2 => {
                if let Err(e) = parse_type(&s[2..], &mut fmt, &mut width) {
                    ui.err(&e);
                    return 1;
                }
            }
            "-j" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("0");
                skip = match parse_bytes(spec) {
                    Ok(n) => n,
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                };
            }
            "-N" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("0");
                limit = match parse_bytes(spec) {
                    Ok(n) => Some(n),
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                };
            }
            // classic: -bcx
            s if s.starts_with('-') && s != "-" => {
                for c in s.chars().skip(1) {
                    match c {
                        'b' => {
                            fmt = 'o';
                            width = 1;
                        }
                        'c' => fmt = 'c',
                        'd' => {
                            fmt = 'd';
                            width = 2;
                        }
                        'o' => {
                            fmt = 'o';
                            width = 2;
                        }
                        'x' => {
                            fmt = 'x';
                            width = 2;
                        }
                        'v' => {}
                        _ => {}
                    }
                }
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }
    if files.is_empty() {
        files.push("-".into());
    }

    let mut data = Vec::new();
    for f in &files {
        if let Err(e) = read_file(f, &mut data) {
            ui.err(&format!("{f}: {e}"));
            return 1;
        }
    }
    if skip as usize > data.len() {
        data.clear();
    } else {
        data = data.split_off(skip as usize);
    }
    if let Some(n) = limit {
        if (n as usize) < data.len() {
            data.truncate(n as usize);
        }
    }

    let mut out = io::stdout().lock();
    if let Err(e) = dump(&data, skip, address_radix, fmt, width, &mut out) {
        if e.kind() != io::ErrorKind::BrokenPipe {
            ui.err(&format!("{e}"));
            return 1;
        }
    }
    0
}

/// Write the full dump of `data` to `out`: 16-byte lines each prefixed with
/// an address in `address_radix`, formatted per `fmt`/`width`, followed by
/// a trailing address-only line (matching GNU `od`'s final offset marker).
fn dump(
    data: &[u8],
    start_offset: u64,
    address_radix: char,
    fmt: char,
    width: usize,
    out: &mut impl Write,
) -> io::Result<()> {
    let line_bytes = 16usize;
    let mut offset = start_offset;
    let mut idx = 0usize;
    while idx < data.len() {
        print_addr(out, offset, address_radix)?;
        let end = (idx + line_bytes).min(data.len());
        let chunk = &data[idx..end];
        match fmt {
            'c' => {
                for &b in chunk {
                    write!(out, " {}", ascii_escape(b))?;
                }
            }
            _ => {
                let mut j = 0;
                while j < chunk.len() {
                    let take = width.min(chunk.len() - j);
                    let mut v: u64 = 0;
                    for (k, &byte) in chunk[j..j + take].iter().enumerate() {
                        // little-endian grouping for multi-byte (traditional od);
                        // `width` is capped to 8 by `parse_type`, so `8 * k` never
                        // reaches or exceeds u64's 64-bit width here.
                        v |= (byte as u64) << (8 * k);
                    }
                    match fmt {
                        'o' => {
                            let w = width * 3;
                            write!(out, " {v:0w$o}")?;
                        }
                        'd' => {
                            write!(out, " {v:w$}", w = width * 3)?;
                        }
                        'x' => {
                            let w = width * 2;
                            write!(out, " {v:0w$x}")?;
                        }
                        _ => {
                            write!(out, " {v:o}")?;
                        }
                    }
                    j += take;
                }
            }
        }
        writeln!(out)?;
        offset += (end - idx) as u64;
        idx = end;
    }
    print_addr(out, offset, address_radix)?;
    writeln!(out)
}

/// Parse a `-t`-style type spec (`o1`, `d2`, `x4`, `c`, ...) into a format
/// character and byte width. The width is clamped to `1..=8` since it
/// represents the number of bytes packed into a `u64` for display — an
/// unclamped width (e.g. from a mistyped `-t o20`) would otherwise overflow
/// the `<<` shift used to assemble that value.
fn parse_type(s: &str, fmt: &mut char, width: &mut usize) -> Result<(), String> {
    let mut chars = s.chars();
    let Some(c) = chars.next() else {
        return Err("invalid type string ''".to_string());
    };
    if !matches!(c, 'o' | 'd' | 'x' | 'c') {
        return Err(format!("invalid type string '{s}'"));
    }
    *fmt = c;
    let rest = chars.as_str();
    if rest.is_empty() {
        if c == 'c' {
            *width = 1;
        }
        return Ok(());
    }
    match rest.parse::<usize>() {
        Ok(n) if n >= 1 => *width = n.min(8),
        Ok(_) => return Err(format!("invalid type string '{s}'")),
        Err(_) => return Err(format!("invalid type string '{s}'")),
    }
    Ok(())
}

/// Parse a `-j`/`-N`-style byte count with an optional `b`/`k`/`K`/`M`
/// multiplier suffix. Both the numeric part and the final multiply are
/// checked: a malformed number or an overflowing result is a hard error
/// rather than silently defaulting to 0 or wrapping.
fn parse_bytes(s: &str) -> Result<u64, String> {
    let (num, mult) = match s.as_bytes().last().copied() {
        Some(b'b') => (&s[..s.len() - 1], 512u64),
        Some(b'k') | Some(b'K') => (&s[..s.len() - 1], 1024),
        Some(b'M') => (&s[..s.len() - 1], 1024 * 1024),
        _ => (s, 1),
    };
    let n: u64 = num.parse().map_err(|_| format!("invalid byte count '{s}'"))?;
    n.checked_mul(mult)
        .ok_or_else(|| format!("byte count '{s}' overflows"))
}

/// Read `path` (or stdin, for `"-"`) fully into `out`.
fn read_file(path: &str, out: &mut Vec<u8>) -> io::Result<()> {
    if path == "-" {
        io::stdin().read_to_end(out)?;
    } else {
        File::open(path)?.read_to_end(out)?;
    }
    Ok(())
}

/// Write an offset in the requested `radix` (`'n'` suppresses the address
/// entirely, matching GNU `od -An`).
fn print_addr(out: &mut impl Write, off: u64, radix: char) -> io::Result<()> {
    match radix {
        'n' => Ok(()),
        'd' => write!(out, "{off:07}"),
        'x' => write!(out, "{off:07x}"),
        _ => write!(out, "{off:07o}"),
    }
}

/// Render one byte for `-t c` output: C-style escapes for common control
/// characters, printable ASCII as itself, everything else as `\NNN` octal.
fn ascii_escape(b: u8) -> String {
    match b {
        b'\\' => "\\\\".into(),
        b'\0' => "\\0".into(),
        b'\n' => "\\n".into(),
        b'\r' => "\\r".into(),
        b'\t' => "\\t".into(),
        0x07 => "\\a".into(),
        0x08 => "\\b".into(),
        0x0b => "\\v".into(),
        0x0c => "\\f".into(),
        c if (0x20..0x7f).contains(&c) => format!("{}", c as char),
        c => format!("\\{c:03o}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_type_default_width_stays() {
        let mut fmt = 'o';
        let mut width = 2;
        parse_type("x", &mut fmt, &mut width).unwrap();
        assert_eq!(fmt, 'x');
        assert_eq!(width, 2);
    }

    #[test]
    fn parse_type_sets_explicit_width() {
        let mut fmt = 'o';
        let mut width = 2;
        parse_type("d4", &mut fmt, &mut width).unwrap();
        assert_eq!(fmt, 'd');
        assert_eq!(width, 4);
    }

    #[test]
    fn parse_type_char_format_defaults_width_one() {
        let mut fmt = 'o';
        let mut width = 2;
        parse_type("c", &mut fmt, &mut width).unwrap();
        assert_eq!(fmt, 'c');
        assert_eq!(width, 1);
    }

    #[test]
    fn parse_type_clamps_oversized_width_to_eight() {
        // This is the overflow guard: an unclamped width of 20 would later
        // shift a u64 by 8*19=152 bits, which panics (debug) or misbehaves
        // (release). It must be clamped instead.
        let mut fmt = 'o';
        let mut width = 2;
        parse_type("o20", &mut fmt, &mut width).unwrap();
        assert_eq!(width, 8);
    }

    #[test]
    fn parse_type_rejects_unknown_format_char() {
        let mut fmt = 'o';
        let mut width = 2;
        assert!(parse_type("q2", &mut fmt, &mut width).is_err());
    }

    #[test]
    fn parse_type_rejects_empty_string() {
        let mut fmt = 'o';
        let mut width = 2;
        assert!(parse_type("", &mut fmt, &mut width).is_err());
    }

    #[test]
    fn parse_bytes_plain_number() {
        assert_eq!(parse_bytes("512"), Ok(512));
    }

    #[test]
    fn parse_bytes_with_suffix() {
        assert_eq!(parse_bytes("2k"), Ok(2048));
        assert_eq!(parse_bytes("1b"), Ok(512));
        assert_eq!(parse_bytes("1M"), Ok(1024 * 1024));
    }

    #[test]
    fn parse_bytes_rejects_garbage_instead_of_defaulting() {
        assert!(parse_bytes("abc").is_err());
        assert!(parse_bytes("").is_err());
    }

    #[test]
    fn parse_bytes_rejects_overflow_instead_of_wrapping() {
        let huge = format!("{}M", u64::MAX);
        assert!(parse_bytes(&huge).is_err());
    }

    #[test]
    fn dump_empty_input_prints_only_final_offset() {
        let mut out = Vec::new();
        dump(&[], 0, 'o', 'o', 2, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "0000000\n");
    }

    #[test]
    fn dump_octal_bytes_golden_path() {
        let mut out = Vec::new();
        dump(&[0, 1, 255], 0, 'o', 'o', 1, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("0000000"));
        assert!(s.contains(" 000"));
        assert!(s.contains(" 377"));
    }

    #[test]
    fn dump_hex_address_radix() {
        let data = vec![0u8; 20];
        let mut out = Vec::new();
        dump(&data, 0, 'x', 'x', 1, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        // second line's address should be 16 (0x10) in hex.
        assert!(s.lines().nth(1).unwrap().starts_with("0000010"));
    }

    #[test]
    fn dump_address_radix_n_suppresses_offsets() {
        let mut out = Vec::new();
        dump(&[1, 2, 3], 0, 'n', 'o', 1, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(!s.starts_with('0'));
    }

    #[test]
    fn dump_char_format_escapes_control_bytes() {
        let mut out = Vec::new();
        dump(b"\n\t\\", 0, 'o', 'c', 1, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\\n"));
        assert!(s.contains("\\t"));
        assert!(s.contains("\\\\"));
    }

    #[test]
    fn dump_with_max_width_does_not_panic() {
        // Regression test for the shift-overflow bug: even if some caller
        // manages to hand `dump` an 8-byte width directly (the max
        // `parse_type` allows), it must not panic.
        let data: Vec<u8> = (0..16u8).collect();
        let mut out = Vec::new();
        dump(&data, 0, 'o', 'x', 8, &mut out).unwrap();
    }

    #[test]
    fn ascii_escape_printable_passthrough() {
        assert_eq!(ascii_escape(b'A'), "A");
    }

    #[test]
    fn ascii_escape_high_byte_is_octal() {
        assert_eq!(ascii_escape(0xff), "\\377");
    }
}
