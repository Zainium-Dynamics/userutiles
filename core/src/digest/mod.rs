//! Pure-Rust digests for ZEX checksum tools (no external crypto crates).

pub mod blake2b;
pub mod md5;
pub mod sha1;
pub mod sha2;

pub use blake2b::Blake2b;
pub use md5::Md5;
pub use sha1::Sha1;
pub use sha2::{Sha224, Sha256, Sha384, Sha512};

/// Hex-encode raw digest bytes (lowercase).
pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Stream a file (or stdin if `-`) through an update closure.
pub fn hash_path_update(path: &str, mut update: impl FnMut(&[u8])) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::{self, Read};

    let mut buf = [0u8; 64 * 1024];
    if path == "-" {
        let mut stdin = io::stdin().lock();
        loop {
            let n = stdin.read(&mut buf)?;
            if n == 0 {
                break;
            }
            update(&buf[..n]);
        }
    } else {
        let mut f = File::open(path)?;
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            update(&buf[..n]);
        }
    }
    Ok(())
}
