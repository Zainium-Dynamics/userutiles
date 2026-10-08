//! user whoami
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
    // Zainium's passwd file directly; libc's getpwuid only knows /etc/passwd
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
