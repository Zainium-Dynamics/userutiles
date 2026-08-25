//! user pivot_root — change the root filesystem.
use std::ffi::CString;
use std::io;

use usercore::Ui;

fn to_cstring(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"))
}

/// `pivot_root(2)` has no libc wrapper (unlike `mount`/`umount2`), so it's
/// called via the raw syscall — same approach `libc` itself uses for
/// syscalls it only exposes a `SYS_*` number for.
fn pivot_root(new_root: &str, put_old: &str) -> io::Result<()> {
    let c_new = to_cstring(new_root)?;
    let c_old = to_cstring(put_old)?;
    // SAFETY: both `CString`s are valid, NUL-terminated, and kept alive
    // for the call; `pivot_root(2)` takes only these two path arguments.
    let r = unsafe { libc::syscall(libc::SYS_pivot_root, c_new.as_ptr(), c_old.as_ptr()) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn print_help() {
    print!(
        "Usage: pivot_root NEW_ROOT PUT_OLD\n\
 Move the current root filesystem to PUT_OLD and make NEW_ROOT the new one.\n\
 Both must be directories on the current root's filesystem tree, and\n\
 PUT_OLD must be underneath NEW_ROOT.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `pivot_root` utility: a thin wrapper around
/// `pivot_root(2)`. Parses `std::env::args()` for exactly two operands,
/// `NEW_ROOT` and `PUT_OLD`.
///
/// Returns 0 on success, 1 on a usage or `pivot_root(2)` error.
pub fn run() -> i32 {
    let ui = Ui::new("pivot_root");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("pivot_root (user_utils) 0.1.0");
        return 0;
    }
    if args.len() != 2 {
        ui.err("usage: pivot_root NEW_ROOT PUT_OLD");
        return 1;
    }

    match pivot_root(&args[0], &args[1]) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!(
                "failed to change root from `{}` to `{}`: {e}",
                args[1], args[0]
            ));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pivot_root_unprivileged_fails_cleanly() {
        // SAFETY: `libc::geteuid` takes no arguments and cannot fail or
        // cause UB.
        if unsafe { libc::geteuid() } != 0 {
            assert!(pivot_root("/tmp", "/tmp").is_err());
        }
    }

    #[test]
    fn pivot_root_rejects_embedded_nul() {
        assert!(pivot_root("/tmp\0x", "/tmp").is_err());
    }
}
