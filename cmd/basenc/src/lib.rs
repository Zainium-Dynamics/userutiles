//! user basenc — encode/decode with multiple bases.
use std::fs::File;
use std::io::{self, Read, Write};

use usercore::Ui;

/// Entry point for the `basenc` utility. Parses `std::env::args()` and
/// encodes (or, with `-d`/`--decode`, decodes) `FILE` (or standard input)
/// using the selected base (base64/base64url/base32/base16/base2,
/// base64 by default).
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("basenc");
    let mut decode = false;
    let mut base = Base::Base64;
    let mut wrap = 76usize;
    let mut file: Option<String> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: basenc [OPTION]... [FILE]\n\
 Encode/decode FILE using selected base.\n\
 --base64 / --base64url / --base32 / --base16 / --base2\n\
 -d, --decode\n\
 -w, --wrap=COLS\n"
                );
                return 0;
            }
            "--version" => {
                println!("basenc (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--decode" => decode = true,
            "--base64" => base = Base::Base64,
            "--base64url" => base = Base::Base64Url,
            "--base32" => base = Base::Base32,
            "--base16" | "--hex" => base = Base::Base16,
            "--base2" => base = Base::Base2,
            "-w" | "--wrap" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option requires an argument -- 'w'");
                    return 1;
                };
                match v.parse() {
                    Ok(n) => wrap = n,
                    Err(_) => {
                        ui.err(&format!("invalid wrap size: '{v}'"));
                        return 1;
                    }
                }
            }
            s if s.starts_with("--wrap=") => {
                let v = &s["--wrap=".len()..];
                match v.parse() {
                    Ok(n) => wrap = n,
                    Err(_) => {
                        ui.err(&format!("invalid wrap size: '{v}'"));
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => file = Some(other.to_string()),
        }
        i += 1;
    }

    let mut data = Vec::new();
    let r = match file.as_deref() {
        None | Some("-") => io::stdin().read_to_end(&mut data),
        Some(f) => File::open(f).and_then(|mut fh| fh.read_to_end(&mut data)),
    };
    if let Err(e) = r {
        ui.err(&format!("{e}"));
        return 1;
    }

    let mut out = io::stdout().lock();
    if decode {
        match base.decode(&data) {
            Ok(bin) => {
                let _ = out.write_all(&bin);
            }
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        }
    } else {
        let enc = base.encode(&data);
        if wrap == 0 {
            let _ = out.write_all(&enc);
            let _ = out.write_all(b"\n");
        } else {
            for c in enc.chunks(wrap) {
                let _ = out.write_all(c);
                let _ = out.write_all(b"\n");
            }
        }
    }
    0
}

/// Supported encoding bases.
enum Base {
    Base64,
    Base64Url,
    Base32,
    Base16,
    Base2,
}

impl Base {
    /// Encode `data` in this base.
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        match self {
            Base::Base64 => b64(data, false),
            Base::Base64Url => b64(data, true),
            Base::Base32 => b32(data),
            Base::Base16 => data
                .iter()
                .flat_map(|b| format!("{b:02x}").into_bytes())
                .collect(),
            Base::Base2 => data
                .iter()
                .flat_map(|b| format!("{b:08b}").into_bytes())
                .collect(),
        }
    }

    /// Decode `data` from this base. `data` need not be pre-cleaned;
    /// ASCII whitespace is stripped before decoding.
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let cleaned: Vec<u8> = data
            .iter()
            .copied()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        match self {
            Base::Base64 => b64_dec(&cleaned, false),
            Base::Base64Url => b64_dec(&cleaned, true),
            Base::Base32 => b32_dec(&cleaned),
            Base::Base16 => hex_dec(&cleaned),
            Base::Base2 => bin_dec(&cleaned),
        }
    }
}

fn b64(data: &[u8], url: bool) -> Vec<u8> {
    let t: &[u8] = if url {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(t[((n >> 18) & 63) as usize]);
        out.push(t[((n >> 12) & 63) as usize]);
        out.push(t[((n >> 6) & 63) as usize]);
        out.push(t[(n & 63) as usize]);
        i += 3;
    }
    if i < data.len() {
        let rem = data.len() - i;
        let mut n = (data[i] as u32) << 16;
        if rem == 2 {
            n |= (data[i + 1] as u32) << 8;
        }
        out.push(t[((n >> 18) & 63) as usize]);
        out.push(t[((n >> 12) & 63) as usize]);
        if rem == 2 {
            out.push(t[((n >> 6) & 63) as usize]);
            if !url {
                out.push(b'=');
            }
        } else if !url {
            out.push(b'=');
            out.push(b'=');
        }
    }
    out
}

fn b64_dec(data: &[u8], url: bool) -> Result<Vec<u8>, String> {
    let mut inv = [255u8; 256];
    let t: &[u8] = if url {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    };
    for (i, &c) in t.iter().enumerate() {
        inv[c as usize] = i as u8;
    }
    let mut cleaned = Vec::new();
    for &b in data {
        if b == b'=' || inv[b as usize] != 255 {
            cleaned.push(b);
        }
    }
    while cleaned.len() % 4 != 0 {
        cleaned.push(b'=');
    }
    let mut out = Vec::new();
    for chunk in cleaned.chunks(4) {
        let mut n = 0u32;
        let mut pad = 0;
        for (i, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                pad += 1;
                continue;
            }
            n |= (inv[c as usize] as u32) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

fn b32(data: &[u8]) -> Vec<u8> {
    const T: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = Vec::new();
    let mut buffer = 0u64;
    let mut bits = 0;
    for &b in data {
        buffer = (buffer << 8) | b as u64;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(T[((buffer >> bits) & 31) as usize]);
        }
    }
    if bits > 0 {
        out.push(T[((buffer << (5 - bits)) & 31) as usize]);
    }
    while out.len() % 8 != 0 {
        out.push(b'=');
    }
    out
}

fn b32_dec(data: &[u8]) -> Result<Vec<u8>, String> {
    const T: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut inv = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        inv[c as usize] = i as u8;
        inv[(c as char).to_ascii_lowercase() as usize] = i as u8;
    }
    let mut buffer = 0u64;
    let mut bits = 0;
    let mut out = Vec::new();
    for &b in data {
        if b == b'=' {
            continue;
        }
        let v = inv[b as usize];
        if v == 255 {
            return Err("invalid input".into());
        }
        buffer = (buffer << 5) | v as u64;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

/// Map a single ASCII hex digit to its 4-bit value.
fn hex_digit(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("invalid hex".into()),
    }
}

/// Decode base16 (hex) `data`.
///
/// Operates on raw bytes (not `&str` byte-index slicing): the previous
/// implementation validated `data` as UTF-8 and then sliced it in 2-byte
/// windows, which panics ("byte index N is not a char boundary") if the
/// cleaned input happens to contain a multi-byte UTF-8 sequence at an odd
/// offset — reachable from arbitrary, user-controlled decode input.
fn hex_dec(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() % 2 != 0 {
        return Err("invalid hex".into());
    }
    data.chunks(2)
        .map(|pair| Ok((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?))
        .collect()
}

/// Map a single ASCII binary digit to its 1-bit value.
fn bin_digit(b: u8) -> Result<u8, String> {
    match b {
        b'0' => Ok(0),
        b'1' => Ok(1),
        _ => Err("invalid base2".into()),
    }
}

/// Decode base2 (binary) `data`. See `hex_dec` for why this works on raw
/// bytes rather than `&str` slicing.
fn bin_dec(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() % 8 != 0 {
        return Err("invalid base2".into());
    }
    data.chunks(8)
        .map(|byte_bits| {
            byte_bits
                .iter()
                .try_fold(0u8, |acc, &b| Ok((acc << 1) | bin_digit(b)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base16_roundtrip() {
        let enc = Base::Base16.encode(b"foobar");
        assert_eq!(enc, b"666f6f626172");
        assert_eq!(Base::Base16.decode(&enc).unwrap(), b"foobar");
    }

    #[test]
    fn base16_uppercase_decodes() {
        assert_eq!(Base::Base16.decode(b"666F6F626172").unwrap(), b"foobar");
    }

    #[test]
    fn base16_odd_length_errors_not_panics() {
        assert!(Base::Base16.decode(b"abc").is_err());
    }

    #[test]
    fn base16_invalid_digit_errors_not_panics() {
        assert!(Base::Base16.decode(b"zz").is_err());
    }

    #[test]
    fn base16_does_not_panic_on_multibyte_utf8_input() {
        // Regression: a naive `&str`-slicing decoder panics here because
        // slicing a 2-byte UTF-8 codepoint down the middle isn't a char
        // boundary. Bytes: 'a' (0x61), then a 2-byte UTF-8 sequence
        // (0xC2 0x80), then 'b' (0x62) — 4 bytes total (even), decodable
        // as raw bytes but not sliceable as `&str`.
        let input: &[u8] = &[0x61, 0xC2, 0x80, 0x62];
        // Must return an Err (invalid hex digits), not panic.
        assert!(Base::Base16.decode(input).is_err());
    }

    #[test]
    fn base2_roundtrip() {
        let enc = Base::Base2.encode(b"foobar");
        assert_eq!(
            enc,
            b"011001100110111101101111011000100110000101110010".as_slice()
        );
        assert_eq!(Base::Base2.decode(&enc).unwrap(), b"foobar");
    }

    #[test]
    fn base2_invalid_length_errors() {
        assert!(Base::Base2.decode(b"0101").is_err());
    }

    #[test]
    fn base2_does_not_panic_on_multibyte_utf8_input() {
        // 8 bytes, valid UTF-8, but not sliceable as ASCII binary digits.
        let input: &[u8] = &[0xC2, 0x80, 0xC2, 0x80, 0xC2, 0x80, 0xC2, 0x80];
        assert!(Base::Base2.decode(input).is_err());
    }

    #[test]
    fn base32_roundtrip() {
        let enc = Base::Base32.encode(b"foobar");
        assert_eq!(enc, b"MZXW6YTBOI======");
        assert_eq!(Base::Base32.decode(&enc).unwrap(), b"foobar");
    }

    #[test]
    fn base32_lowercase_decodes() {
        assert_eq!(Base::Base32.decode(b"mzxw6ytboi======").unwrap(), b"foobar");
    }

    #[test]
    fn base32_invalid_char_errors() {
        assert!(Base::Base32.decode(b"!!!!!!!!").is_err());
    }

    #[test]
    fn base64_roundtrip() {
        let enc = Base::Base64.encode(b"foobar");
        assert_eq!(enc, b"Zm9vYmFy");
        assert_eq!(Base::Base64.decode(&enc).unwrap(), b"foobar");
    }

    #[test]
    fn base64url_roundtrip() {
        let enc = Base::Base64Url.encode(b"foobar");
        assert_eq!(enc, b"Zm9vYmFy"); // no +/- chars needed for this input
        assert_eq!(Base::Base64Url.decode(&enc).unwrap(), b"foobar");
    }

    #[test]
    fn empty_input_encodes_and_decodes_to_empty() {
        assert_eq!(Base::Base16.encode(b""), b"");
        assert_eq!(Base::Base16.decode(b"").unwrap(), b"");
        assert_eq!(Base::Base32.encode(b""), b"");
        assert_eq!(Base::Base32.decode(b"").unwrap(), b"");
    }
}
