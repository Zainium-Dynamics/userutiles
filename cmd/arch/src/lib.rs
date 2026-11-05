//! user arch: print machine hardware name.
use std::ffi::CStr;
use std::io;
use std::mem::MaybeUninit;

use usercore::Ui;

/// CLI entry point: parses arguments, prints the machine architecture (or
/// help/version text), and returns the process exit code.
pub fn run() -> i32 {
    let ui = Ui::new("arch");
    // (Using `if let` instead of a `for` loop avoids a clippy::never_loop
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: arch\nPrint machine architecture.\n");
                return 0;
            }
            "--version" => {
                println!("arch (user_utils) 0.1.0");
                return 0;
            }
            other => {
                ui.err(&format!("invalid option -- '{other}'"));
                return 1;
            }
        }
    }
    match machine_name() {
        Ok(m) => {
            println!("{m}");
            0
        }
        Err(e) => {
            ui.err(&e.to_string());
            1
        }
    }
}

/// Return the machine hardware name reported by `uname(2)` (e.g. `x86_64`).
pub fn machine_name() -> io::Result<String> {
    let mut uts = MaybeUninit::<libc::utsname>::uninit();
    // SAFETY: `uts.as_mut_ptr()` points to a stack-allocated, properly
    if unsafe { libc::uname(uts.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `uname` returned 0 above, which per POSIX means it
    let uts = unsafe { uts.assume_init() };
    // SAFETY: `uts.machine` is a fixed-size `c_char` array embedded in the
    let m = unsafe { CStr::from_ptr(uts.machine.as_ptr()) };
    Ok(m.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_name_succeeds_and_is_non_empty() {
        let m = machine_name().expect("uname(2) should succeed");
        assert!(!m.is_empty());
    }
}
