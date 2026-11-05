//! user whoami
use std::io::{self, Write};

pub fn run() -> i32 {
    // `for` makes that explicit (this also fixes a `clippy::never_loop`
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
    let euid = unsafe { libc::geteuid() };
    let name = usercore::nss::user_by_uid(euid).map(|pw| pw.name);
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
