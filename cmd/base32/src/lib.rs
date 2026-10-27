//! user base32 — RFC 4648 base32 encode/decode.

/// `base32` is `basenc --base32`: same encoder, options and messages.
pub fn run() -> i32 {
    user_basenc::run_as("base32", Some(user_basenc::Base::Base32))
}
