//! user sha224sum — print or check 224-bit checksums.

/// GNU-compatible `sha224sum`: hashing, `--tag`, `-b`, `-z` and `--check`, shared with the other checksum tools.
pub fn run() -> i32 {
    usercore::digest::cli::main("sha224sum", usercore::digest::cli::Algo::Sha224)
}
