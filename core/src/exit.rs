//! Process exit codes used across ZEX utils.

/// Successful completion.
pub const EXIT_SUCCESS: i32 = 0;

/// General failure (I/O error, runtime error).
pub const EXIT_FAILURE: i32 = 1;

/// Incorrect usage / invalid arguments (GNU convention for many utils).
pub const EXIT_USAGE: i32 = 2;
