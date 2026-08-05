//! user whoami
use std::ffi::CStr;
use std::io::{self, Write};

pub fn run() -> i32 {
    // whoami takes no positional operands, only an optional single flag, so
    // there is never a need to loop over more than the first argument —
    // every match arm below returns immediately. Using `if let` instead of
    // `for` makes that explicit (this also fixes a `clippy::never_loop`
    // warning; behavior is unchanged, since the old loop body diverged on
    // every path on its very first iteration anyway).
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: whoami\nPrint the user name associated with the current effective user ID.\n");
                return 0;
            }
            "--version" => {
                println!("whoami (user_utils) 0.1.0");
                return 0;
            }
            other => {
                eprintln!("whoami: invalid option -- '{other}'");
                return 1;
            }
        }
    }
    // SAFETY: `libc::geteuid` takes no arguments, reads no caller-supplied
    // pointers, and cannot fail — always safe to call.
    let euid = unsafe { libc::geteuid() };
    // SAFETY: `libc::getpwuid` takes a plain uid integer and returns either
    // NULL (checked before use) or a pointer to a libc-owned static `passwd`
    // record. `pw_name` is documented to be a valid NUL-terminated C string
    // owned by that same static record, so `CStr::from_ptr` on it is sound
    // as long as we're done with it before another passwd-database call
    // could reuse the static buffer — which we are, since we immediately
    // copy it into an owned `String` via `to_string_lossy().into_owned()`
    // and don't call getpwuid/getpwnam again afterward.
    let name = unsafe {
        let pw = libc::getpwuid(euid);
        if pw.is_null() {
            None
        } else {
            Some(CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned())
        }
    };
    match name {
        Some(n) => {
            let mut out = io::stdout().lock();
            let _ = writeln!(out, "{n}");
            0
        }
        None => {
            eprintln!("whoami: cannot find name for user ID {euid}");
            1
        }
    }
}
