//! user arch — print machine hardware name.
use std::ffi::CStr;
use std::io;
use std::mem::MaybeUninit;

use usercore::Ui;

/// CLI entry point: parses arguments, prints the machine architecture (or
/// help/version text), and returns the process exit code.
pub fn run() -> i32 {
    let ui = Ui::new("arch");
    // Only the first argument is meaningful: `arch` takes at most one flag.
    // (Using `if let` instead of a `for` loop avoids a clippy::never_loop
    // lint, since every match arm below returns unconditionally anyway.)
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
    // aligned `libc::utsname` with room for a full `utsname` (guaranteed by
    // `MaybeUninit`). `uname(2)` only writes into the struct pointed to by
    // its argument and never reads from it, so passing a pointer to
    // uninitialized memory of the correct size/alignment is sound.
    if unsafe { libc::uname(uts.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `uname` returned 0 above, which per POSIX means it
    // successfully filled in every field of `uts`, so the struct is fully
    // initialized and `assume_init` is sound.
    let uts = unsafe { uts.assume_init() };
    // SAFETY: `uts.machine` is a fixed-size `c_char` array embedded in the
    // now-initialized `utsname` struct; `uname(2)` always writes a
    // NUL-terminated string into it that fits within the array bounds, and
    // `uts` (and thus the array) remains alive for the duration of this
    // call, so `CStr::from_ptr` on its first-element pointer is sound.
    let m = unsafe { CStr::from_ptr(uts.machine.as_ptr()) };
    Ok(m.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_name_succeeds_and_is_non_empty() {
        // Hermetic: uname(2) always succeeds for the calling process and
        // never touches the filesystem, so this is safe to run anywhere.
        let m = machine_name().expect("uname(2) should succeed");
        assert!(!m.is_empty());
    }
}
