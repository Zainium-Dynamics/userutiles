//! user basenc — encode/decode with multiple bases; `base64` and `base32` run through it too.
use std::fs::File;
use std::io::{self, Read, Write};

/// Entry point for the `basenc` utility.
pub fn run() -> i32 {
    run_as("basenc", None)
}

/// The shared tool: `fixed` is the base of `base64`/`base32`; `basenc` picks one with `--base*`.
pub fn run_as(name: &str, fixed: Option<Base>) -> i32 {
    let mut decode = false;
    let mut ignore_garbage = false;
    let mut base = fixed;
    let mut wrap = 76usize;
    let mut files: Vec<String> = Vec::new();
    let fail = |m: &str| {
        eprintln!("{name}: {m}\nTry '{name} --help' for more information.");
        1
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut opts_done = false;
    while i < args.len() {
        let a = args[i].clone();
        i += 1;
        if opts_done || a == "-" || !a.starts_with('-') {
            files.push(a);
            continue;
        }
        if let Some(long) = a.strip_prefix("--") {
            let (key, val) = match long.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (long, None),
            };
            let picked = match key {
                "base64" => Some(Base::Base64),
                "base64url" => Some(Base::Base64Url),
                "base32" => Some(Base::Base32),
                "base32hex" => Some(Base::Base32Hex),
                "base16" => Some(Base::Base16),
                "base2msbf" => Some(Base::Base2Msbf),
                "base2lsbf" => Some(Base::Base2Lsbf),
                "z85" => Some(Base::Z85),
                _ => None,
            };
            if let Some(b) = picked.filter(|_| fixed.is_none()) {
                base = Some(b);
                continue;
            }
            match key {
                "" => opts_done = true,
                "help" => {
                    print_help(name, fixed.is_none());
                    return 0;
                }
                "version" => {
                    println!("{name} (user_utils) 0.2.0");
                    return 0;
                }
                "decode" => decode = true,
                "ignore-garbage" => ignore_garbage = true,
                "wrap" => {
                    let v = match val {
                        Some(v) => v,
                        None if i < args.len() => {
                            i += 1;
                            args[i - 1].clone()
                        }
                        None => return fail("option '--wrap' requires an argument"),
                    };
                    match v.parse() {
                        Ok(n) => wrap = n,
                        Err(_) => return fail(&format!("invalid wrap size: '{v}'")),
                    }
                }
                _ => return fail(&format!("unrecognized option '{a}'")),
            }
            continue;
        }
        let chars: Vec<char> = a[1..].chars().collect();
        let mut k = 0;
        while k < chars.len() {
            match chars[k] {
                'd' => decode = true,
                'i' => ignore_garbage = true,
                'w' => {
                    let rest: String = chars[k + 1..].iter().collect();
                    let v = if !rest.is_empty() {
                        rest
                    } else if i < args.len() {
                        i += 1;
                        args[i - 1].clone()
                    } else {
                        return fail("option requires an argument -- 'w'");
                    };
                    match v.parse() {
                        Ok(n) => wrap = n,
                        Err(_) => return fail(&format!("invalid wrap size: '{v}'")),
                    }
                    break;
                }
                c => return fail(&format!("invalid option -- '{c}'")),
            }
            k += 1;
        }
    }
    let Some(base) = base else {
        return fail("missing encoding type");
    };
    if files.len() > 1 {
        return fail(&format!("extra operand '{}'", files[1]));
    }

    let mut data = Vec::new();
    let file = files.first().map(String::as_str).unwrap_or("-");
    let r = match file {
        "-" => io::stdin().read_to_end(&mut data),
        f => File::open(f).and_then(|mut fh| fh.read_to_end(&mut data)),
    };
    if let Err(e) = r {
        let m = e.to_string();
        let m = m.split(" (os error").next().unwrap_or(&m);
        eprintln!("{name}: {file}: {m}");
        return 1;
    }

    let mut out = io::stdout().lock();
    if decode {
        let (bin, err) = base.decode(&data, ignore_garbage);
        let _ = out.write_all(&bin);
        let _ = out.flush();
        if let Some(e) = err {
            eprintln!("{name}: {e}");
            return 1;
        }
    } else {
        let enc = match base.encode(&data) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("{name}: {e}");
                return 1;
            }
        };
        // -w 0 writes one unwrapped line without a newline, like GNU; empty input writes nothing
        if wrap == 0 {
            let _ = out.write_all(&enc);
        } else {
            for c in enc.chunks(wrap) {
                let _ = out.write_all(c);
                let _ = out.write_all(b"\n");
            }
        }
    }
    0
}

fn print_help(name: &str, choose: bool) {
    println!("Usage: {name} [OPTION]... [FILE]\nEncode or decode FILE, or standard input, to standard output.\n");
    if choose {
        println!("      --base64          same as 'base64'\n      --base64url       file- and url-safe base64\n      --base32          same as 'base32'\n      --base32hex       extended hex alphabet base32\n      --base16          hex encoding\n      --base2msbf       bit string, most significant bit first\n      --base2lsbf       bit string, least significant bit first\n      --z85             ascii85-like encoding (ZeroMQ spec:32/Z85)");
    }
    println!("  -d, --decode          decode data\n  -i, --ignore-garbage  when decoding, ignore non-alphabet characters\n  -w, --wrap=COLS       wrap encoded lines after COLS characters (default 76), 0 disables\n\nWith no FILE, or when FILE is -, read standard input.");
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const B32HEX: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
const Z85: &[u8; 85] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#";

/// Supported encoding bases.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Base {
    Base64,
    Base64Url,
    Base32,
    Base32Hex,
    Base16,
    Base2Msbf,
    Base2Lsbf,
    Z85,
}

impl Base {
    /// Encode `data` in this base (unwrapped).
    pub fn encode(self, data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(match self {
            Base::Base64 => radix_encode(data, B64, 6),
            Base::Base64Url => radix_encode(data, B64URL, 6),
            Base::Base32 => radix_encode(data, B32, 5),
            Base::Base32Hex => radix_encode(data, B32HEX, 5),
            Base::Base16 => data.iter().flat_map(|b| format!("{b:02X}").into_bytes()).collect(),
            Base::Base2Msbf => data.iter().flat_map(|b| format!("{b:08b}").into_bytes()).collect(),
            Base::Base2Lsbf => data.iter().flat_map(|b| format!("{:08b}", b.reverse_bits()).into_bytes()).collect(),
            Base::Z85 => {
                if data.len() % 4 != 0 {
                    return Err("invalid input (length must be multiple of 4 characters)".into());
                }
                let mut out = Vec::with_capacity(data.len() / 4 * 5);
                for c in data.chunks(4) {
                    let mut v = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
                    let mut five = [0u8; 5];
                    for d in five.iter_mut().rev() {
                        *d = Z85[(v % 85) as usize];
                        v /= 85;
                    }
                    out.extend_from_slice(&five);
                }
                out
            }
        })
    }

    /// Decode `data`, newlines ignored (and anything outside the alphabet with
    /// `garbage`). Returns what decoded before the first error, and that error.
    pub fn decode(self, data: &[u8], garbage: bool) -> (Vec<u8>, Option<String>) {
        let in_alphabet = |b: u8| -> bool {
            match self {
                Base::Base64 => B64.contains(&b) || b == b'=',
                Base::Base64Url => B64URL.contains(&b) || b == b'=',
                Base::Base32 => B32.contains(&b.to_ascii_uppercase()) || b == b'=',
                Base::Base32Hex => B32HEX.contains(&b.to_ascii_uppercase()) || b == b'=',
                Base::Base16 => b.is_ascii_hexdigit(),
                Base::Base2Msbf | Base::Base2Lsbf => b == b'0' || b == b'1',
                Base::Z85 => Z85.contains(&b),
            }
        };
        let clean: Vec<u8> = data.iter().copied().filter(|&b| b != b'\n' && (!garbage || in_alphabet(b))).collect();
        let invalid = || Some("invalid input".to_string());
        match self {
            Base::Base64 => radix_decode(&clean, B64, 6),
            Base::Base64Url => radix_decode(&clean, B64URL, 6),
            Base::Base32 | Base::Base32Hex => {
                let up: Vec<u8> = clean.iter().map(u8::to_ascii_uppercase).collect();
                radix_decode(&up, if self == Base::Base32 { B32 } else { B32HEX }, 5)
            }
            Base::Base16 => {
                let mut out = Vec::new();
                for pair in clean.chunks(2) {
                    let digit = |b: u8| (b as char).to_digit(16);
                    match (pair.len(), digit(pair[0]), pair.get(1).and_then(|&b| digit(b))) {
                        (2, Some(h), Some(l)) => out.push((h * 16 + l) as u8),
                        _ => return (out, invalid()),
                    }
                }
                (out, None)
            }
            Base::Base2Msbf | Base::Base2Lsbf => {
                let mut out = Vec::new();
                for byte in clean.chunks(8) {
                    if byte.len() != 8 || !byte.iter().all(|&b| b == b'0' || b == b'1') {
                        return (out, invalid());
                    }
                    let v = byte.iter().fold(0u8, |acc, &b| (acc << 1) | (b - b'0'));
                    out.push(if self == Base::Base2Lsbf { v.reverse_bits() } else { v });
                }
                (out, None)
            }
            Base::Z85 => {
                let mut out = Vec::new();
                for group in clean.chunks(5) {
                    if group.len() != 5 {
                        return (out, invalid());
                    }
                    let mut v: u64 = 0;
                    for &b in group {
                        let Some(d) = Z85.iter().position(|&z| z == b) else { return (out, invalid()) };
                        v = v * 85 + d as u64;
                    }
                    if v > u64::from(u32::MAX) {
                        return (out, invalid());
                    }
                    out.extend_from_slice(&(v as u32).to_be_bytes());
                }
                (out, None)
            }
        }
    }
}

/// RFC 4648 base64/base32 encoding with `=` padding; `bits` is 6 or 5 per symbol.
fn radix_encode(data: &[u8], alphabet: &[u8], bits: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut n) = (0u32, 0u32);
    for &b in data {
        acc = (acc << 8) | u32::from(b);
        n += 8;
        while n >= bits {
            n -= bits;
            out.push(alphabet[((acc >> n) & ((1 << bits) - 1)) as usize]);
        }
        acc &= (1 << n) - 1;
    }
    if n > 0 {
        out.push(alphabet[((acc << (bits - n)) & ((1 << bits) - 1)) as usize]);
    }
    let quantum = if bits == 6 { 4 } else { 8 };
    while out.len() % quantum != 0 {
        out.push(b'=');
    }
    out
}

/// Decode base64/base32 symbols; padding may be left out at the end, as GNU accepts.
fn radix_decode(data: &[u8], alphabet: &[u8], bits: u32) -> (Vec<u8>, Option<String>) {
    let mut out = Vec::new();
    let (mut acc, mut n) = (0u32, 0u32);
    let mut symbols = 0usize;
    let mut padded = false;
    for &b in data {
        if b == b'=' {
            padded = true;
            continue;
        }
        let Some(v) = alphabet.iter().position(|&c| c == b) else {
            return (out, Some("invalid input".into()));
        };
        if padded {
            // data after the padding of an earlier quantum: a new, concatenated block
            padded = false;
            acc = 0;
            n = 0;
            symbols = 0;
        }
        symbols += 1;
        acc = (acc << bits) | v as u32;
        n += bits;
        if n >= 8 {
            n -= 8;
            out.push((acc >> n) as u8);
            acc &= (1 << n) - 1;
        }
    }
    // a lone trailing symbol (or 3 / 6 in base32) cannot carry a whole byte
    let rem = symbols % if bits == 6 { 4 } else { 8 };
    let bad = if bits == 6 { rem == 1 } else { matches!(rem, 1 | 3 | 6) };
    (out, bad.then(|| "invalid input".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(b: Base, s: &[u8]) -> Result<Vec<u8>, String> {
        match b.decode(s, false) {
            (v, None) => Ok(v),
            (_, Some(e)) => Err(e),
        }
    }

    #[test]
    fn base16_roundtrip() {
        let enc = Base::Base16.encode(b"foobar").unwrap();
        assert_eq!(enc, b"666F6F626172");
        assert_eq!(dec(Base::Base16, &enc).unwrap(), b"foobar");
        assert_eq!(dec(Base::Base16, b"666f6f626172").unwrap(), b"foobar");
        assert!(dec(Base::Base16, b"abc").is_err());
        assert!(dec(Base::Base16, b"zz").is_err());
        assert!(dec(Base::Base16, &[0x61, 0xC2, 0x80, 0x62]).is_err());
    }

    #[test]
    fn base2_roundtrip() {
        let enc = Base::Base2Msbf.encode(b"fo").unwrap();
        assert_eq!(enc, b"0110011001101111");
        assert_eq!(dec(Base::Base2Msbf, &enc).unwrap(), b"fo");
        assert_eq!(Base::Base2Lsbf.encode(b"\x01").unwrap(), b"10000000");
        assert!(dec(Base::Base2Msbf, b"0101").is_err());
    }

    #[test]
    fn rfc4648_vectors() {
        for (plain, b64, b32) in [
            ("", "", ""),
            ("f", "Zg==", "MY======"),
            ("fo", "Zm8=", "MZXQ===="),
            ("foo", "Zm9v", "MZXW6==="),
            ("foob", "Zm9vYg==", "MZXW6YQ="),
            ("fooba", "Zm9vYmE=", "MZXW6YTB"),
            ("foobar", "Zm9vYmFy", "MZXW6YTBOI======"),
        ] {
            assert_eq!(Base::Base64.encode(plain.as_bytes()).unwrap(), b64.as_bytes());
            assert_eq!(Base::Base32.encode(plain.as_bytes()).unwrap(), b32.as_bytes());
            assert_eq!(dec(Base::Base64, b64.as_bytes()).unwrap(), plain.as_bytes());
            assert_eq!(dec(Base::Base32, b32.as_bytes()).unwrap(), plain.as_bytes());
        }
        assert_eq!(Base::Base32Hex.encode(b"foobar").unwrap(), b"CPNMUOJ1E8======");
    }

    #[test]
    fn decode_edges() {
        assert_eq!(dec(Base::Base64, b"aGk").unwrap(), b"hi");
        assert_eq!(dec(Base::Base64, b"aGk=\naGk=").unwrap(), b"hihi");
        assert!(dec(Base::Base64, b"a").is_err());
        assert!(dec(Base::Base64, b"!!aGk=").is_err());
        assert_eq!(Base::Base64.decode(b"!!aGk=", true), (b"hi".to_vec(), None));
        assert_eq!(dec(Base::Base32, b"mzxw6ytboi======").unwrap(), b"foobar");
    }

    #[test]
    fn z85_vector() {
        let raw = [0x86, 0x4F, 0xD2, 0x6F, 0xB5, 0x59, 0xF7, 0x5B];
        assert_eq!(Base::Z85.encode(&raw).unwrap(), b"HelloWorld");
        assert_eq!(dec(Base::Z85, b"HelloWorld").unwrap(), raw);
        assert!(Base::Z85.encode(b"abc").is_err());
    }
}
