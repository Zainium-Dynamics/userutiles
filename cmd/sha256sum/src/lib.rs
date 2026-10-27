//! user sha256sum — print or check 256-bit checksums.

/// GNU-compatible `sha256sum`: hashing, `--tag`, `-b`, `-z` and `--check`, shared with the other checksum tools.
pub fn run() -> i32 {
    usercore::digest::cli::main("sha256sum", usercore::digest::cli::Algo::Sha256)
}
