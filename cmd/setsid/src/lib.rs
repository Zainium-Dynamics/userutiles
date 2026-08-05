//! user setsid — run a program in a new session, detached from the
//! controlling terminal of the calling process.
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

use usercore::Ui;

const HELP: &str = "Usage: setsid [options] <program> [arguments...]\n\
Run a program in a new session.\n\n\
  -c, --ctty     set the controlling terminal to the current one\n\
  -f, --fork     always fork\n\
  -w, --wait     wait for program to exit, and use the same exit code\n\
  -h, --help     display this help and exit\n\
      --version  output version information and exit\n";

/// Entry point for the `setsid` utility. Calls the `setsid(2)` syscall to
/// start a new session (detaching from any controlling terminal), then
/// runs the given command. By default this happens in-place via `exec`
/// (matching setsid(1)); `-f`/`--fork` (or being a process-group leader,
/// for whom `setsid()` would otherwise fail with `EPERM`) forks first.
pub fn run() -> i32 {
    let ui = Ui::new("setsid");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut ctty = false;
    let mut force_fork = false;
    let mut wait = false;
    let mut cmd: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("setsid (user_utils) 0.1.0");
                return 0;
            }
            "-c" | "--ctty" => ctty = true,
            "-f" | "--fork" => force_fork = true,
            "-w" | "--wait" => wait = true,
            // Combined short options, e.g. `-cfw`.
            s if s.starts_with('-') && !s.starts_with("--") && s.len() > 1 => {
                for c in s[1..].chars() {
                    match c {
                        'c' => ctty = true,
                        'f' => force_fork = true,
                        'w' => wait = true,
                        other => {
                            ui.err(&format!("invalid option -- '{other}'"));
                            return 1;
                        }
                    }
                }
            }
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

    let already_leader = already_group_leader();
    let executable = cmd[0].clone();

    let mut to_run = Command::new(&cmd[0]);
    to_run.args(&cmd[1..]);
    // SAFETY: `prepare_child` only calls `setsid(2)` and, if requested,
    // `ioctl(TIOCSCTTY)` — both are plain syscalls taking no pointers derived
    // from Rust state, so they remain sound in the async-signal-restricted
    // post-fork/pre-exec context.
    unsafe {
        to_run.pre_exec(move || prepare_child(ctty));
    }

    if force_fork || already_leader {
        match to_run.spawn() {
            Ok(mut child) => {
                if wait {
                    match child.wait() {
                        Ok(status) => status.code().unwrap_or(1),
                        Err(e) => {
                            ui.err(&format!("failed to wait on child: {e}"));
                            1
                        }
                    }
                } else {
                    0
                }
            }
            Err(e) => report_exec_failure(&ui, &executable, &e),
        }
    } else {
        let err = to_run.exec();
        report_exec_failure(&ui, &executable, &err)
    }
}

fn already_group_leader() -> bool {
    // SAFETY: `getpgrp()` takes no arguments and cannot fail or cause UB.
    let pgrp = unsafe { libc::getpgrp() };
    pgrp == std::process::id() as i32
}

/// Runs post-fork, pre-exec: async-signal-safety rules apply (no
/// allocation, no locking). `setsid(2)` and `ioctl(2)` are both plain
/// syscalls and satisfy that.
fn prepare_child(take_controlling_tty: bool) -> io::Result<()> {
    // SAFETY: wraps the `setsid(2)` syscall; no pointer arguments.
    unsafe {
        libc::setsid();
    }
    if take_controlling_tty {
        // SAFETY: wraps `ioctl(0, TIOCSCTTY, 1)`; no Rust-owned memory is
        // passed, the `1` argument is a plain integer, not a pointer.
        let rc = unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 1) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn report_exec_failure(ui: &Ui, executable: &str, err: &io::Error) -> i32 {
    ui.err(&format!("failed to execute {executable}: {err}"));
    match err.kind() {
        io::ErrorKind::NotFound => 127,
        io::ErrorKind::PermissionDenied => 126,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_group_leader_matches_getpgrp() {
        // SAFETY: plain syscalls, no pointers.
        let (pid, pgrp) = unsafe { (libc::getpid(), libc::getpgrp()) };
        assert_eq!(already_group_leader(), pid == pgrp);
    }
}
