//! user mesg — control write access to the invoking user's terminal
//! (the setgid-tty group-write bit `crw--w----`, as toggled by
//! `write(1)`/`talk(1)`).
use std::io;

use usercore::Ui;

const HELP: &str = "Usage: mesg [options] [y | n]\n\
Display or change your message status.\n\n\
  -v, --verbose  explain what is being done\n\
  -h, --help     display this help and exit\n\
      --version  output version information and exit\n";

/// Entry point for the `mesg` utility. With no argument, prints `is y`/`is
/// n` for whether write access is currently allowed (exit 0/1). With `y`
/// or `n`, sets it by flipping the group/other write bits on whichever of
/// stdin/stdout/stderr is a terminal.
pub fn run() -> i32 {
    let ui = Ui::new("mesg");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut verbose = false;
    let mut enable: Option<bool> = None;

    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("mesg (user_utils) 0.1.0");
                return 0;
            }
            "-v" | "--verbose" => verbose = true,
            "y" | "Y" if enable.is_none() => enable = Some(true),
            "n" | "N" if enable.is_none() => enable = Some(false),
            other => {
                ui.err(&format!("invalid argument: '{other}'"));
                return 2;
            }
        }
    }

    let Some(fd) = first_tty_fd() else {
        ui.err("stdin/stdout/stderr is not a terminal");
        return 2;
    };

    let mode = match fstat_mode(fd) {
        Ok(m) => m,
        Err(e) => {
            ui.err(&format!("failed to stat terminal: {e}"));
            return 2;
        }
    };

    match enable {
        Some(want) => {
            // GNU/util-linux semantics: `mesg y` sets only the group-write
            // bit; `mesg n` clears both group and other write bits.
            let new_mode = if want { mode | 0o020 } else { mode & !0o022 };
            if let Err(e) = fchmod(fd, new_mode) {
                ui.err(&format!("failed to change terminal mode: {e}"));
                return 2;
            }
            if verbose {
                println!(
                    "write access to your terminal is {}",
                    if want { "allowed" } else { "denied" }
                );
            }
            if want {
                0
            } else {
                1
            }
        }
        None => {
            if mode & 0o022 != 0 {
                println!("is y");
                0
            } else {
                println!("is n");
                1
            }
        }
    }
}

/// First of stdin(0)/stdout(1)/stderr(2) that refers to a terminal.
fn first_tty_fd() -> Option<i32> {
    [0, 1, 2].into_iter().find(|&fd| {
        // SAFETY: `isatty` takes a plain fd, no pointers; safe for any fd
        // value including invalid ones (just returns 0/sets errno).
        unsafe { libc::isatty(fd) == 1 }
    })
}

fn fstat_mode(fd: i32) -> io::Result<u32> {
    // SAFETY: `st` is a plain, fully-initialized-on-success POD struct;
    // `fd` is a valid fd from `first_tty_fd`, and `&mut st` is a valid
    // pointer to stack memory of the correct type for `fstat(2)`.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        if libc::fstat(fd, &mut st) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(st.st_mode)
    }
}

fn fchmod(fd: i32, mode: u32) -> io::Result<()> {
    // SAFETY: `fd` is a valid fd; `mode` is a plain mode_t value with no
    // aliasing/pointer concerns.
    let rc = unsafe { libc::fchmod(fd, mode as libc::mode_t) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
