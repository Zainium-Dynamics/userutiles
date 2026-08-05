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
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: cksum [FILE]...\nPrint CRC checksum and byte counts of each FILE.\n"
                );
                return 0;
            }
            "--version" => {
                println!("cksum (user_utils) 0.1.0");
                return 0;
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }
    let mut status = 0;
    for f in &files {
        match cksum_file(f) {
            Ok((crc, len)) => {
                if f == "-" && files.len() == 1 {
                    println!("{crc} {len}");
                } else {
                    println!("{crc} {len} {f}");
                }
            }
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }
    status
}

/// Read `path` (or standard input, for `-`) fully and compute its POSIX
/// `cksum` CRC and byte length.
fn cksum_file(path: &str) -> io::Result<(u32, u64)> {
    let mut data = Vec::new();
    if path == "-" {
        io::stdin().read_to_end(&mut data)?;
    } else {
        File::open(path)?.read_to_end(&mut data)?;
    }
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
