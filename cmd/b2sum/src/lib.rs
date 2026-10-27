//! user b2sum — print or check 512-bit BLAKE2b checksums.

/// GNU-compatible `b2sum`: hashing, `--tag`, `-b`, `-z` and `--check`, shared with the other checksum tools.
pub fn run() -> i32 {
    usercore::digest::cli::main("b2sum", usercore::digest::cli::Algo::Blake2b)
}
