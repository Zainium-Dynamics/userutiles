//! user base32 — RFC 4648 base32 encode/decode.
use std::fs::File;
use std::io::{self, Read, Write};

use usercore::Ui;

const T: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// CLI entry point: parses arguments, encodes or decodes the given file (or
/// stdin), and returns the process exit code.
pub fn run() -> i32 {
    let ui = Ui::new("base32");
    let mut decode = false;
    let mut wrap: usize = 76;
    let mut file: Option<String> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: base32 [OPTION]... [FILE]\n -d, --decode decode\n -w, --wrap=COLS wrap (0=disable)\n");
                return 0;
            }
            "--version" => {
                println!("base32 (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--decode" => decode = true,
            "-w" | "--wrap" => {
                i += 1;
                wrap = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(76);
            }
            s if s.starts_with('-') => {
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
        ui.err(&e.to_string());
        return 1;
    }
    let mut out = io::stdout().lock();
    if decode {
        match b32_decode(&data) {
            Ok(bin) => {
                let _ = out.write_all(&bin);
            }
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        }
    } else {
        let enc = b32_encode(&data);
        if wrap == 0 {
            let _ = writeln!(out, "{}", String::from_utf8_lossy(&enc));
        } else {
            for c in enc.chunks(wrap) {
                let _ = out.write_all(c);
                let _ = out.write_all(b"\n");
            }
        }
    }
    0
}

/// Encode `data` as RFC 4648 base32 (uppercase alphabet, `=` padding).
pub fn b32_encode(data: &[u8]) -> Vec<u8> {
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

/// Decode RFC 4648 base32 text (case-insensitive, whitespace and `=`
/// padding ignored) back to bytes.
pub fn b32_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut inv = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        inv[c as usize] = i as u8;
        inv[(c as char).to_ascii_lowercase() as usize] = i as u8;
    }
    let mut buffer = 0u64;
    let mut bits = 0;
    let mut out = Vec::new();
    for &b in data {
        if b.is_ascii_whitespace() || b == b'=' {
            continue;
        }
        let v = inv[b as usize];
        if v == 255 {
            return Err(format!("invalid input byte {b:#04x}"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty() {
        assert_eq!(b32_encode(b""), b"");
    }

    #[test]
    fn encode_known_vector() {
        // RFC 4648 test vector.
        assert_eq!(b32_encode(b"foobar"), b"MZXW6YTBOI======");
    }

    #[test]
    fn decode_known_vector() {
        assert_eq!(b32_decode(b"MZXW6YTBOI======").unwrap(), b"foobar");
    }

    #[test]
    fn decode_is_case_insensitive() {
        assert_eq!(b32_decode(b"mzxw6ytboi======").unwrap(), b"foobar");
    }

    #[test]
    fn roundtrip() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let enc = b32_encode(data);
        assert_eq!(b32_decode(&enc).unwrap(), data);
    }

    #[test]
    fn decode_rejects_invalid_byte() {
        assert!(b32_decode(b"!!!!").is_err());
    }
}
