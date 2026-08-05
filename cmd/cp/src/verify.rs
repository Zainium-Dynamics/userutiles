// verify.rs — Post-copy integrity verification via XXH3-128.
//
// Enabled with --verify: after copying every byte to the destination, hash
// both sides and compare. A mismatch is reported as an error but the
// destination is left in place (unlike mv, cp never deletes the source).
//
// XXH3-128 is significantly faster than SHA-256 for bulk data and is
// sufficient to detect I/O corruption, bit-rot, and partial writes.

use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use xxhash_rust::xxh3::Xxh3;

use crate::error::{io_err, CpError, Result};

const BUF: usize = 4 * 1024 * 1024; // 4 MiB read buffer

/// Hash a file with XXH3-128 and return the 128-bit digest.
pub fn hash_file(path: &Path) -> Result<u128> {
    let file = File::open(path).map_err(|e| io_err(path, e))?;
    let mut rdr = BufReader::with_capacity(BUF, file);
    let mut h = Xxh3::new();
    let mut buf = vec![0u8; BUF];

    loop {
        let n = rdr.read(&mut buf).map_err(|e| io_err(path, e))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }

    Ok(h.digest128())
}

/// Compare XXH3-128 digests of `src` and `dest`.
/// Returns `CpError::ChecksumMismatch` on any discrepancy.
pub fn verify(src: &Path, dest: &Path) -> Result<()> {
    let src_hash = hash_file(src)?;
    let dest_hash = hash_file(dest)?;

    if src_hash != dest_hash {
        return Err(CpError::ChecksumMismatch {
            path: dest.to_owned(),
            src_hash,
            dest_hash,
        });
    }
    Ok(())
}
