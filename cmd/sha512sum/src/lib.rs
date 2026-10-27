//! user sha512sum — print or check 512-bit checksums.

/// GNU-compatible `sha512sum`: hashing, `--tag`, `-b`, `-z` and `--check`, shared with the other checksum tools.
pub fn run() -> i32 {
    usercore::digest::cli::main("sha512sum", usercore::digest::cli::Algo::Sha512)
}
