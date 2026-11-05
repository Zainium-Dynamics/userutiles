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
