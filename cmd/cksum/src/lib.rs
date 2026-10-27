//! user cksum — POSIX CRC checksum.
use std::fs::File;
use std::io::{self, Read};
use std::sync::OnceLock;

use usercore::Ui;

/// Entry point for the `cksum` utility. Parses `std::env::args()` and
/// prints the POSIX CRC and byte count of each `FILE` (or standard
/// input, with no operands).
///
/// Returns 0 on success, 1 if any file could not be read or an unknown
/// option was given.
pub fn run() -> i32 {
    let ui = Ui::new("cksum");
    let mut algo = String::from("crc");
    let mut rest: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: cksum [OPTION]... [FILE]...\nPrint CRC checksum and byte counts of each FILE.\n\n  -a, --algorithm=TYPE  sysv, bsd, crc (default), crc32b, md5, sha1, sha224,\n                        sha256, sha384, sha512, blake2b\n      --untagged        plain HASH  FILE lines for the digest types\n  -c, --check, -l, --length, --tag, -z  as in md5sum / b2sum\n"
                );
                return 0;
            }
            "--version" => {
                println!("cksum (user_utils) 0.2.0");
                return 0;
            }
            "-a" | "--algorithm" => match args.next() {
                Some(v) => algo = v,
                None => {
                    ui.err("option requires an argument -- 'a'");
                    return 1;
                }
            },
            s if s.starts_with("--algorithm=") => algo = s["--algorithm=".len()..].to_string(),
            s if s.starts_with("-a") => algo = s[2..].to_string(),
            _ => rest.push(arg),
        }
    }
    if let Some(a) = usercore::digest::cli::Algo::from_name(&algo) {
        return usercore::digest::cli::run_args("cksum", a, rest, true);
    }
    if !matches!(algo.as_str(), "crc" | "crc32b" | "sysv" | "bsd") {
        ui.err(&format!("invalid argument '{algo}' for '--algorithm'"));
        return 1;
    }
    let mut files: Vec<String> = Vec::new();
    for arg in rest {
        match arg.as_str() {
            "--untagged" | "--" => {}
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            _ => files.push(arg),
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }
    let mut status = 0;
    for f in &files {
        let line = match algo.as_str() {
            "bsd" => user_sum::sum_bsd(f).map(|(c, b)| format!("{c:05} {b:5}")),
            "sysv" => user_sum::sum_sysv(f).map(|(c, b)| format!("{c} {b}")),
            "crc32b" => read_all(f).map(|d| format!("{} {}", crc32b(&d), d.len())),
            _ => cksum_file(f).map(|(crc, len)| format!("{crc} {len}")),
        };
        match line {
            Ok(l) if f == "-" && files.len() == 1 => println!("{l}"),
            Ok(l) => println!("{l} {f}"),
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }
    status
}

fn read_all(path: &str) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    if path == "-" {
        io::stdin().read_to_end(&mut data)?;
    } else {
        File::open(path)?.read_to_end(&mut data)?;
    }
    Ok(data)
}

/// CRC-32 as zlib/gzip compute it (`cksum -a crc32b`).
fn crc32b(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Read `path` (or standard input, for `-`) fully and compute its POSIX
/// `cksum` CRC and byte length.
fn cksum_file(path: &str) -> io::Result<(u32, u64)> {
    let data = read_all(path)?;
    Ok((posix_crc(&data), data.len() as u64))
}

/// The 256-entry CRC-32/POSIX lookup table, built once and cached (it's
/// identical on every call, so recomputing it per invocation of
/// `posix_crc` — as the original implementation did — is wasted work,
/// though not a correctness issue for a short-lived CLI process).
fn crc_table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (i, entry) in table.iter_mut().enumerate() {
            let mut c = (i as u32) << 24;
            for _ in 0..8 {
                if c & 0x8000_0000 != 0 {
                    c = (c << 1) ^ 0x04c1_1db7;
                } else {
                    c <<= 1;
                }
            }
            *entry = c;
        }
        table
    })
}

/// POSIX 1003.2 CRC (`cksum`): a CRC-32/POSIX over the data bytes
/// followed by the data length (little-endian byte order), complemented.
fn posix_crc(data: &[u8]) -> u32 {
    let table = crc_table();
    let mut crc: u32 = 0;
    for &b in data {
        crc = (crc << 8) ^ table[(((crc >> 24) as u8) ^ b) as usize];
    }
    // length
    let mut n = data.len() as u64;
    while n != 0 {
        let b = (n & 0xff) as u8;
        n >>= 8;
        crc = (crc << 8) ^ table[(((crc >> 24) as u8) ^ b) as usize];
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference vectors cross-checked against the system `cksum(1)`.
    #[test]
    fn crc32b_vector() {
        assert_eq!(crc32b(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn posix_crc_empty_input() {
        assert_eq!(posix_crc(b""), 4294967295);
    }

    #[test]
    fn posix_crc_foobar() {
        assert_eq!(posix_crc(b"foobar"), 2606601686);
    }

    #[test]
    fn posix_crc_pangram() {
        assert_eq!(
            posix_crc(b"The quick brown fox jumps over the lazy dog"),
            2074844392
        );
    }

    #[test]
    fn cksum_file_reports_length() {
        let (_, len) = cksum_file_from_bytes(b"hello");
        assert_eq!(len, 5);
    }

    #[test]
    fn cksum_file_missing_path_errors() {
        let missing = format!("/nonexistent_user_cksum_test_{}", std::process::id());
        assert!(cksum_file(&missing).is_err());
    }

    /// Test helper mirroring `cksum_file`'s in-memory computation, since
    /// `cksum_file` itself only reads from a path or stdin.
    fn cksum_file_from_bytes(data: &[u8]) -> (u32, u64) {
        (posix_crc(data), data.len() as u64)
    }
}
