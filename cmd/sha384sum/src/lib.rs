//! user sha384sum — print or check 384-bit checksums.

/// GNU-compatible `sha384sum`: hashing, `--tag`, `-b`, `-z` and `--check`, shared with the other checksum tools.
pub fn run() -> i32 {
    usercore::digest::cli::main("sha384sum", usercore::digest::cli::Algo::Sha384)
}
