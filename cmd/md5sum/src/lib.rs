//! user md5sum — print or check 128-bit checksums.

/// GNU-compatible `md5sum`: hashing, `--tag`, `-b`, `-z` and `--check`, shared with the other checksum tools.
pub fn run() -> i32 {
    usercore::digest::cli::main("md5sum", usercore::digest::cli::Algo::Md5)
}
