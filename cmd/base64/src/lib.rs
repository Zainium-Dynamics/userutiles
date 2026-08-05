//! user base64 — encode/decode base64 (RFC 4648).
use std::fs::File;
use std::io::{self, Read, Write};

use usercore::Ui;

const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Entry point for the `base64` utility. Parses `std::env::args()` and
/// encodes (or, with `-d`/`--decode`, decodes) `FILE` (or standard input)
/// to standard output.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("base64");
    let mut decode = false;
    let mut ignore_garbage = false;
    let mut wrap: Option<usize> = Some(76);
    let mut file: Option<String> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: base64 [OPTION]... [FILE]\nBase64 encode or decode FILE, or standard input, to standard output.\n -d, --decode decode data\n -i, --ignore-garbage when decoding, ignore non-alphabet characters\n -w, --wrap=COLS wrap encoded lines after COLS (default 76, 0 disables)\n");
                return 0;
            }
            "--version" => {
                println!("base64 (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--decode" => decode = true,
            "-i" | "--ignore-garbage" => ignore_garbage = true,
            "-w" | "--wrap" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option requires an argument -- 'w'");
                    return 1;
                };
                match v.parse() {
                    Ok(n) => wrap = Some(n),
                    Err(_) => {
                        ui.err(&format!("invalid wrap size: '{v}'"));
                        return 1;
                    }
                }
            }
            s if s.starts_with("-w") && s.len() > 2 => match s[2..].parse() {
                Ok(n) => wrap = Some(n),
                Err(_) => {
                    ui.err(&format!("invalid wrap size: '{}'", &s[2..]));
                    return 1;
                }
            },
            s if s.starts_with("--wrap=") => {
                let v = &s["--wrap=".len()..];
                match v.parse() {
                    Ok(n) => wrap = Some(n),
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

    let data = match read_input(file.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            ui.err(&format!("{e}"));
            return 1;
        }
    };

    let mut out = io::stdout().lock();
    if decode {
        match b64_decode(&data, ignore_garbage) {
            Ok(bin) => {
                if let Err(e) = out.write_all(&bin) {
                    if e.kind() != io::ErrorKind::BrokenPipe {
                        ui.err(&format!("{e}"));
                        return 1;
                    }
                }
            }
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        }
    } else {
        let enc = b64_encode(&data);
        let w = wrap.unwrap_or(0);
        if w == 0 {
            let _ = out.write_all(&enc);
            let _ = out.write_all(b"\n");
        } else {
            for chunk in enc.chunks(w) {
                let _ = out.write_all(chunk);
                let _ = out.write_all(b"\n");
            }
        }
    }
    0
}

/// Read `FILE` (or standard input when `file` is `None` or `Some("-")`)
/// fully into memory.
fn read_input(file: Option<&str>) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    match file {
        None | Some("-") => {
            io::stdin().read_to_end(&mut data)?;
        }
        Some(f) => {
            File::open(f)?.read_to_end(&mut data)?;
        }
    }
    Ok(data)
}

/// Encode `data` as standard (RFC 4648 §4) base64, `=`-padded.
fn b64_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize]);
        out.push(T[((n >> 12) & 63) as usize]);
        out.push(T[((n >> 6) & 63) as usize]);
        out.push(T[(n & 63) as usize]);
        i += 3;
    }
    if i < data.len() {
        let rem = data.len() - i;
        let mut n = (data[i] as u32) << 16;
        if rem == 2 {
            n |= (data[i + 1] as u32) << 8;
        }
        out.push(T[((n >> 18) & 63) as usize]);
        out.push(T[((n >> 12) & 63) as usize]);
        if rem == 2 {
            out.push(T[((n >> 6) & 63) as usize]);
            out.push(b'=');
        } else {
            out.push(b'=');
            out.push(b'=');
        }
    }
    out
}

/// Decode standard base64 `data` back to bytes. Whitespace is always
/// skipped; any other non-alphabet byte is an error unless `ignore` is
/// set, in which case it is skipped too. Returns `Err` if the cleaned
/// input length isn't a multiple of 4.
fn b64_decode(data: &[u8], ignore: bool) -> Result<Vec<u8>, String> {
    let mut inv = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        inv[c as usize] = i as u8;
    }
    let mut cleaned = Vec::new();
    for &b in data {
        if b == b'=' || inv[b as usize] != 255 {
            cleaned.push(b);
        } else if b.is_ascii_whitespace() {
            continue;
        } else if !ignore {
            return Err("invalid input".into());
        }
    }
    if cleaned.len() % 4 != 0 {
        return Err("invalid input".into());
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

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4648 §10 test vectors.
    #[test]
    fn encode_rfc4648_vectors() {
        assert_eq!(b64_encode(b""), b"");
        assert_eq!(b64_encode(b"f"), b"Zg==");
        assert_eq!(b64_encode(b"fo"), b"Zm8=");
        assert_eq!(b64_encode(b"foo"), b"Zm9v");
        assert_eq!(b64_encode(b"foob"), b"Zm9vYg==");
        assert_eq!(b64_encode(b"fooba"), b"Zm9vYmE=");
        assert_eq!(b64_encode(b"foobar"), b"Zm9vYmFy");
    }

    #[test]
    fn decode_rfc4648_vectors() {
        assert_eq!(b64_decode(b"", false).unwrap(), b"");
        assert_eq!(b64_decode(b"Zg==", false).unwrap(), b"f");
        assert_eq!(b64_decode(b"Zm8=", false).unwrap(), b"fo");
        assert_eq!(b64_decode(b"Zm9v", false).unwrap(), b"foo");
        assert_eq!(b64_decode(b"Zm9vYg==", false).unwrap(), b"foob");
        assert_eq!(b64_decode(b"Zm9vYmE=", false).unwrap(), b"fooba");
        assert_eq!(b64_decode(b"Zm9vYmFy", false).unwrap(), b"foobar");
    }

    #[test]
    fn decode_rejects_bad_length() {
        assert!(b64_decode(b"Zg", false).is_err());
    }

    #[test]
    fn decode_rejects_invalid_byte_without_ignore() {
        assert!(b64_decode(b"Zg#=", false).is_err());
    }

    #[test]
    fn decode_ignore_garbage_skips_invalid_bytes() {
        assert_eq!(b64_decode(b"Z#g==", true).unwrap(), b"f");
    }

    #[test]
    fn decode_skips_whitespace_even_without_ignore() {
        assert_eq!(b64_decode(b"Zm9v\nYmFy\n", false).unwrap(), b"foobar");
    }

    #[test]
    fn roundtrip_arbitrary_bytes() {
        let data: Vec<u8> = (0..=255u8).collect();
        let enc = b64_encode(&data);
        assert_eq!(b64_decode(&enc, false).unwrap(), data);
    }

    #[test]
    fn read_input_missing_file_errors() {
        let missing = format!("/nonexistent_user_base64_test_{}", std::process::id());
        assert!(read_input(Some(&missing)).is_err());
    }
}
