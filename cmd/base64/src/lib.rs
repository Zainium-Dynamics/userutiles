//! user base64 — encode/decode base64 (RFC 4648).

/// `base64` is `basenc --base64`: same encoder, options and messages.
pub fn run() -> i32 {
    user_basenc::run_as("base64", Some(user_basenc::Base::Base64))
}
