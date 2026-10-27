//! user sha1sum — print or check 160-bit checksums.

/// GNU-compatible `sha1sum`: hashing, `--tag`, `-b`, `-z` and `--check`, shared with the other checksum tools.
pub fn run() -> i32 {
    usercore::digest::cli::main("sha1sum", usercore::digest::cli::Algo::Sha1)
}
