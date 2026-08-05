//! user renice — alter the scheduling priority of one or more running
//! processes, process groups, or all processes owned by a user.
use std::ffi::CString;

use usercore::Ui;

const HELP: &str = "Usage: renice [-n] <priority> [-g | -p | -u] <identifier>...\n\
Alter the priority of running processes.\n\n\
  -n, --priority <num>  specify the nice increment/value\n\
  -g, --pgrp             interpret identifiers as process group IDs\n\
  -p, --pid               interpret identifiers as process IDs (default)\n\
  -u, --user               interpret identifiers as usernames or UIDs\n\
  -h, --help              display this help and exit\n\
      --version           output version information and exit\n";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Pid,
    Pgrp,
    User,
}

impl Mode {
    // `__priority_which_t` doesn't exist in musl's libc bindings (only
    // glibc's) — return a plain c_int and let `as _` at each call site
    // coerce to whatever `getpriority`/`setpriority` actually expect on
    // the target libc.
    fn which(self) -> libc::c_int {
        match self {
            Mode::Pid => libc::PRIO_PROCESS,
            Mode::Pgrp => libc::PRIO_PGRP,
            Mode::User => libc::PRIO_USER,
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Mode::Pid => "process ID",
            Mode::Pgrp => "process group ID",
            Mode::User => "user ID",
        }
    }
}

/// Entry point for the `renice` utility. Sets the nice value of one or
/// more targets via `setpriority(2)`, printing an "old priority, new
/// priority" line per target (matching util-linux's `renice`). Returns 0
/// if every target succeeded, 1 if any failed.
pub fn run() -> i32 {
    let ui = Ui::new("renice");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut priority: Option<i32> = None;
    let mut mode = Mode::Pid;
    let mut targets: Vec<(Mode, String)> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("renice (user_utils) 0.1.0");
                return 0;
            }
            "-n" | "--priority" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option '-n' requires an argument");
                    return 1;
                };
                match v.parse::<i32>() {
                    Ok(n) => priority = Some(n),
                    Err(_) => {
                        ui.err(&format!("invalid priority '{v}'"));
                        return 1;
                    }
                }
            }
            "-g" | "--pgrp" => mode = Mode::Pgrp,
            "-p" | "--pid" => mode = Mode::Pid,
            "-u" | "--user" => mode = Mode::User,
            other if priority.is_none() && other.parse::<i32>().is_ok() => {
                priority = Some(other.parse().unwrap());
            }
            other => targets.push((mode, other.to_string())),
        }
        i += 1;
    }

    let Some(priority) = priority else {
        ui.err("a priority value must be given");
        return 1;
    };

    if targets.is_empty() {
        ui.err("no identifiers given");
        return 1;
    }

    let mut had_error = false;
    for (mode, ident) in targets {
        match resolve_id(mode, &ident) {
            Ok(id) => {
                let old = get_priority(mode, id);
                if let Err(e) = apply_priority(mode, id, priority) {
                    ui.err(&format!("failed to set priority of {ident}: {e}"));
                    had_error = true;
                } else {
                    match old {
                        Some(old) => println!(
                            "{ident} ({}): old priority {old}, new priority {priority}",
                            mode.noun()
                        ),
                        None => println!(
                            "{ident} ({}): new priority {priority}",
                            mode.noun()
                        ),
                    }
                }
            }
            Err(e) => {
                ui.err(&e);
                had_error = true;
            }
        }
    }

    if had_error {
        1
    } else {
        0
    }
}

/// Resolves an identifier string to the numeric ID `setpriority(2)`
/// expects: for `-u` this accepts either a username (looked up via
/// `getpwnam`) or a bare numeric UID; other modes are always numeric.
fn resolve_id(mode: Mode, ident: &str) -> Result<u32, String> {
    if let Ok(n) = ident.parse::<u32>() {
        return Ok(n);
    }
    if mode == Mode::User {
        return lookup_uid(ident).ok_or_else(|| format!("unknown user '{ident}'"));
    }
    Err(format!("invalid identifier '{ident}'"))
}

fn lookup_uid(name: &str) -> Option<u32> {
    let c_name = CString::new(name).ok()?;
    // SAFETY: `getpwnam` is called with a valid NUL-terminated C string; the
    // returned pointer (if non-null) refers to a static/thread-local buffer
    // owned by libc that remains valid until the next passwd-database call,
    // which is sufficient for reading `pw_uid` immediately.
    unsafe {
        let pw = libc::getpwnam(c_name.as_ptr());
        if pw.is_null() {
            None
        } else {
            Some((*pw).pw_uid)
        }
    }
}

/// Reads the current priority of a target, or `None` if it can't be
/// determined (e.g. the target doesn't exist) — in which case we still
/// proceed to `setpriority` and let that report the real error.
fn get_priority(mode: Mode, id: u32) -> Option<i32> {
    // SAFETY: plain-integer arguments only. `getpriority` can legitimately
    // return -1 on success, so errno must be cleared first and checked to
    // disambiguate from failure.
    unsafe {
        *libc::__errno_location() = 0;
        let rc = libc::getpriority(mode.which() as _, id);
        if rc == -1 && *libc::__errno_location() != 0 {
            None
        } else {
            Some(rc)
        }
    }
}

fn apply_priority(mode: Mode, id: u32, priority: i32) -> Result<(), std::io::Error> {
    // SAFETY: `setpriority` takes only plain integer arguments; no pointers
    // are involved, so this cannot cause UB regardless of `id`/`priority`
    // (an invalid `id` simply fails with `ESRCH`, reported via errno).
    let rc = unsafe { libc::setpriority(mode.which() as _, id, priority) };
    if rc != 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_id_accepts_numeric_for_any_mode() {
        assert_eq!(resolve_id(Mode::Pid, "1234"), Ok(1234));
        assert_eq!(resolve_id(Mode::Pgrp, "1"), Ok(1));
        assert_eq!(resolve_id(Mode::User, "0"), Ok(0));
    }

    #[test]
    fn resolve_id_rejects_non_numeric_pid() {
        assert!(resolve_id(Mode::Pid, "bob").is_err());
    }

    #[test]
    fn resolve_id_looks_up_known_user() {
        // "root" is always UID 0 on any real Linux system.
        assert_eq!(resolve_id(Mode::User, "root"), Ok(0));
    }

    #[test]
    fn resolve_id_rejects_unknown_user() {
        assert!(resolve_id(Mode::User, "no-such-user-user-test").is_err());
    }
}
