//! user setpgid — run a program in a new process group.
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;

use usercore::Ui;

const HELP: &str = "Usage: setpgid [options] <program> [arguments...]\n\
Run a program in a new process group.\n\n\
  -f, --foreground  make the new process group the foreground group on\n\
                     the controlling terminal\n\
  -h, --help        display this help and exit\n\
      --version     output version information and exit\n";

/// Entry point for the `setpgid` utility. Puts the calling process into a
/// new process group (`setpgid(0, 0)`), optionally makes that group the
/// foreground group of `/dev/tty`, then `exec`s the given command in place.
pub fn run() -> i32 {
    let ui = Ui::new("setpgid");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut foreground = false;
    let mut cmd: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("setpgid (user_utils) 0.1.0");
                return 0;
            }
            "-f" | "--foreground" => foreground = true,
            other => {
                cmd.push(other.to_string());
                cmd.extend(args[i + 1..].iter().cloned());
                break;
            }
        }
        i += 1;
    }

    if cmd.is_empty() {
        ui.err("no command specified");
        return 1;
    }

    // SAFETY: `setpgid(0, 0)` takes only plain integers ("the calling
    // process", "make it its own group leader"); no pointer arguments.
    if unsafe { libc::setpgid(0, 0) } != 0 {
        ui.err(&format!(
            "failed to create new process group: {}",
            io::Error::last_os_error()
        ));
        return 1;
    }

    if foreground {
        if let Ok(tty) = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty") {
            // SAFETY: `tcsetpgrp`/`getpgrp` take a valid open fd and plain
            // integers; failure (e.g. no controlling tty) is reported via
            // errno only and is intentionally ignored, matching setpgid(1).
            unsafe {
                libc::tcsetpgrp(tty.as_raw_fd(), libc::getpgrp());
            }
        }
    }

    let err = Command::new(&cmd[0]).args(&cmd[1..]).exec();
    ui.err(&format!("failed to execute '{}': {err}", cmd[0]));
    match err.kind() {
        io::ErrorKind::NotFound => 127,
        io::ErrorKind::PermissionDenied => 126,
        _ => 1,
    }
}
