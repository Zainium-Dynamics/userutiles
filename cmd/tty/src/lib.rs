//! user tty
use std::ffi::CStr;
use std::io::IsTerminal;

pub fn run() -> i32 {
    let mut silent = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: tty [OPTION]...\nPrint the file name of the terminal connected to standard input.\n -s, --silent, --quiet print nothing, only return an exit status\n");
                return 0;
            }
            "--version" => {
                println!("tty (user_utils) 0.1.0");
                return 0;
            }
            "-s" | "--silent" | "--quiet" => silent = true,
            other => {
                eprintln!("tty: invalid option -- '{other}'");
                return 1;
            }
        }
    }
    if !std::io::stdin().is_terminal() {
        if !silent {
            println!("not a tty");
        }
        return 1;
    }
    // SAFETY: `libc::ttyname` takes a plain fd argument (`STDIN_FILENO`,
    // always valid as a constant) and returns either NULL (checked below
    // before use) or a pointer to an internal, NUL-terminated static buffer
    // owned by libc. We only dereference that pointer with `CStr::from_ptr`
    // after confirming it is non-null, and we don't hold onto it past this
    // block (it's converted to an owned/borrowed Rust string via
    // `to_string_lossy` and printed immediately), so there's no risk of it
    // being invalidated by a subsequent call before we're done with it.
    unsafe {
        let p = libc::ttyname(libc::STDIN_FILENO);
        if p.is_null() {
            if !silent {
                println!("not a tty");
            }
            return 1;
        }
        if !silent {
            println!("{}", CStr::from_ptr(p).to_string_lossy());
        }
    }
    0
}
