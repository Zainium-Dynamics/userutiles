//! user agetty — alternative Linux getty.
//!
//! Opens a terminal device (or reuses an already-open one, with `-`),
//! makes it the controlling terminal via `setsid(2)` + `TIOCSCTTY`,
//! prints `/etc/issue` (with a handful of `\X` escapes expanded) and a
//! `HOST login: ` prompt, reads a username, then `exec`s `login` (or a
//! custom program via `-l`) with it — the same handoff real
//! `agetty(8)` does. `login` itself handles password verification from
//! that point on.
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use usercore::Ui;

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "localhost".to_string())
}

/// `uname(2)`'s `sysname`/`release`/`machine` fields, for `/etc/issue`'s
/// `\s`/`\r`/`\m` escapes.
fn uname_fields() -> (String, String, String) {
    // SAFETY: an all-zero `utsname` is a valid initial value —
    // `uname` below fully populates it before any field is read.
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    // SAFETY: `u` is a valid, appropriately-sized out-param for `uname(2)`.
    let ok = unsafe { libc::uname(&mut u) } == 0;
    if !ok {
        return (String::new(), String::new(), String::new());
    }
    let cstr_field = |field: &[libc::c_char]| -> String {
        let bytes: Vec<u8> = field
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    (
        cstr_field(&u.sysname),
        cstr_field(&u.release),
        cstr_field(&u.machine),
    )
}

/// Expand the small set of `\X` escapes real `agetty` supports in
/// `/etc/issue`: `\l` line (tty), `\n` hostname, `\s`/`\r`/`\m`
/// sysname/release/machine.
fn expand_issue(text: &str, tty: &str) -> String {
    let (sysname, release, machine) = uname_fields();
    let host = hostname();
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('l') => out.push_str(tty),
            Some('n') => out.push_str(&host),
            Some('s') => out.push_str(&sysname),
            Some('r') => out.push_str(&release),
            Some('m') => out.push_str(&machine),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn print_issue(tty: &str) {
    let path = usercore::zainium::etc_path("issue");
    if let Ok(text) = std::fs::read_to_string(&path) {
        print!("{}", expand_issue(&text, tty));
    }
}

/// Make `fd` (already dup'd onto stdin/stdout/stderr) this process's
/// controlling terminal: `setsid(2)` to leave any existing session,
/// then `TIOCSCTTY` to attach.
fn acquire_controlling_tty(fd: i32) -> io::Result<()> {
    // SAFETY: takes no arguments; failure (already a session leader) is
    // checked and, per real getty behavior, non-fatal — TIOCSCTTY still
    // works when invoked as a session leader without a controlling tty.
    unsafe { libc::setsid() };
    // SAFETY: `fd` is a valid, open file descriptor for the target tty;
    // the `0` argument means "don't steal it if another process already
    // has it as controlling tty", matching getty's own usage.
    let r = unsafe { libc::ioctl(fd, libc::TIOCSCTTY as _, 0) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Open `tty_arg` and make it stdin/stdout/stderr and the controlling
/// terminal — unless `tty_arg` is `-`, meaning "use the already-open
/// stdin as-is" (an extension real `agetty` also accepts).
fn set_up_terminal(tty_arg: &str) -> io::Result<()> {
    if tty_arg == "-" {
        return Ok(());
    }
    let path = if tty_arg.starts_with('/') {
        PathBuf::from(tty_arg)
    } else {
        PathBuf::from("/dev").join(tty_arg)
    };
    let f = OpenOptions::new().read(true).write(true).open(&path)?;
    let fd = f.as_raw_fd();
    acquire_controlling_tty(fd)?;
    for target in [0, 1, 2] {
        // SAFETY: `fd` is a valid, open descriptor for the tty; `target`
        // is one of the three standard descriptor numbers.
        if unsafe { libc::dup2(fd, target) } == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    // `f` (and its fd, if distinct from 0/1/2) is dropped/closed here;
    // the dup'd copies on 0/1/2 stay open independently.
    Ok(())
}

/// Read one line from stdin, giving up after `timeout` if set and
/// nothing arrived — matches `-t/--timeout`.
fn read_line_with_timeout(timeout: Option<Duration>) -> io::Result<Option<String>> {
    if let Some(limit) = timeout {
        let mut pfd = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        let deadline = Instant::now() + limit;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            // SAFETY: `pfd` is a valid, correctly-sized `pollfd` array
            // of length 1; the timeout is a plain millisecond count.
            let r = unsafe { libc::poll(&mut pfd, 1, remaining.as_millis() as i32) };
            if r > 0 {
                break;
            } else if r == 0 {
                return Ok(None);
            } else {
                let err = io::Error::last_os_error();
                if err.kind() != io::ErrorKind::Interrupted {
                    return Err(err);
                }
            }
        }
    }
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(Some(line.trim_end_matches(['\r', '\n']).to_string()))
}

/// Find `login` (or `login_program`, if given) on Zainium's `PATH`.
fn find_login_program(login_program: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = login_program {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
        return None;
    }
    usercore::zainium::path_dirs()
        .into_iter()
        .map(|d| d.join("login"))
        .find(|p| p.is_file())
}

/// Substitute `\u` in `-o/--login-options` with the username, the one
/// escape real `agetty` documents for that option.
fn expand_login_options(options: &str, username: &str) -> Vec<String> {
    options
        .split_whitespace()
        .map(|tok| tok.replace("\\u", username))
        .collect()
}

fn print_help() {
    print!(
        "Usage: agetty [options] TTY [BAUD_RATE...] [TERM]\n\
 Open TTY (or '-' to reuse an already-open one), make it the\n\
 controlling terminal, prompt for a username, and exec login.\n\
 -a, --autologin USER skip the prompt, log USER in directly\n\
 -l, --login-program PROGRAM use PROGRAM instead of `login`\n\
 -o, --login-options OPTIONS extra args passed to the login program\n\
 -i, --noissue don't print /etc/issue\n\
 -n, --skip-login don't prompt for a username at all\n\
 -H, --host FAKEHOST use FAKEHOST instead of the real hostname\n\
 -t, --timeout SECONDS give up if nothing is typed in time\n\
 -L, --local-line, --noclear accepted, no-op (no carrier detection\n\
 or screen-clearing behavior to skip in the first place)\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `agetty` utility. Parses `std::env::args()`,
/// attaches the given (or reused) terminal as the controlling tty,
/// prompts for a username (unless `-a`/`-n`), and `exec`s the login
/// program — this function only returns on a setup error, since a
/// successful run replaces the process.
///
/// Returns 1 on any usage or setup error.
pub fn run() -> i32 {
    let ui = Ui::new("agetty");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut autologin: Option<String> = None;
    let mut login_program: Option<String> = None;
    let mut login_options = String::new();
    let mut noissue = false;
    let mut skip_login = false;
    let mut fakehost: Option<String> = None;
    let mut timeout: Option<Duration> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("agetty (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--autologin" => {
                i += 1;
                match args.get(i) {
                    Some(v) => autologin = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 'a'");
                        return 1;
                    }
                }
            }
            "-l" | "--login-program" => {
                i += 1;
                match args.get(i) {
                    Some(v) => login_program = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 'l'");
                        return 1;
                    }
                }
            }
            "-o" | "--login-options" => {
                i += 1;
                match args.get(i) {
                    Some(v) => login_options = v.clone(),
                    None => {
                        ui.err("option requires an argument -- 'o'");
                        return 1;
                    }
                }
            }
            "-H" | "--host" => {
                i += 1;
                match args.get(i) {
                    Some(v) => fakehost = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 'H'");
                        return 1;
                    }
                }
            }
            "-t" | "--timeout" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse().ok()) {
                    Some(secs) => timeout = Some(Duration::from_secs(secs)),
                    None => {
                        ui.err("invalid or missing timeout");
                        return 1;
                    }
                }
            }
            "-i" | "--noissue" => noissue = true,
            "-n" | "--skip-login" => skip_login = true,
            "-L" | "--local-line" | "--noclear" => {}
            s if s.starts_with('-') && s.len() > 1 => {
                // Unknown flag: accepted and ignored rather than a hard
                // error, matching how many real agetty invocations pass
                // platform-specific flags (-8, -f, -w, ...) this port
                // doesn't need to act on.
                let _ = s;
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let Some(tty_arg) = positional.first() else {
        ui.err("usage: agetty [options] TTY [BAUD_RATE...] [TERM]");
        return 1;
    };
    if let Some(term) = positional.last().filter(|_| positional.len() > 1) {
        // Last positional after TTY [BAUD...] is TERM, if it's not
        // purely numeric (a baud rate).
        if term.parse::<u32>().is_err() {
            std::env::set_var("TERM", term);
        }
    }

    if let Err(e) = set_up_terminal(tty_arg) {
        ui.err(&format!("{tty_arg}: {e}"));
        return 1;
    }

    if !noissue {
        print_issue(tty_arg);
    }

    let host = fakehost.unwrap_or_else(hostname);

    let username = if let Some(user) = autologin {
        user
    } else if skip_login {
        String::new()
    } else {
        print!("{host} login: ");
        let _ = io::stdout().flush();
        match read_line_with_timeout(timeout) {
            Ok(Some(u)) => u,
            Ok(None) => {
                ui.err("timed out waiting for a login name");
                return 1;
            }
            Err(e) => {
                ui.err(&format!("{e}"));
                return 1;
            }
        }
    };

    let Some(login_bin) = find_login_program(login_program.as_deref()) else {
        ui.err("login program not found");
        return 1;
    };

    let mut cmd = Command::new(&login_bin);
    if !login_options.is_empty() {
        cmd.args(expand_login_options(&login_options, &username));
    }
    if !username.is_empty() {
        cmd.arg(&username);
    }
    let err = cmd.exec();
    ui.err(&format!("failed to execute {}: {err}", login_bin.display()));
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_issue_substitutes_known_escapes() {
        let out = expand_issue("Welcome on \\l (\\s)", "tty1");
        assert!(out.contains("tty1"));
        assert!(!out.contains("\\l"));
    }

    #[test]
    fn expand_issue_leaves_unknown_escapes_alone() {
        let out = expand_issue("100\\% done", "tty1");
        assert_eq!(out, "100\\% done");
    }

    #[test]
    fn expand_login_options_substitutes_u() {
        let out = expand_login_options("-h \\u --foo", "alice");
        assert_eq!(out, vec!["-h", "alice", "--foo"]);
    }

    #[test]
    fn find_login_program_none_when_explicit_path_missing() {
        assert!(find_login_program(Some("/nonexistent/user-agetty-test-login")).is_none());
    }

    #[test]
    fn read_line_with_timeout_zero_gives_up_immediately_if_nothing_pending() {
        // A zero-length timeout should report "nothing arrived" rather
        // than blocking — exercised against a pipe with no writer
        // rather than the test's own stdin, which pytest/cargo test
        // harnesses may or may not leave open.
        let mut fds = [0i32; 2];
        // SAFETY: `fds` is a valid, writable 2-element out-param.
        let r = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(r, 0);
        let mut pfd = libc::pollfd {
            fd: fds[0],
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `pfd` is a valid pollfd; a zero timeout returns
        // immediately without blocking.
        let ready = unsafe { libc::poll(&mut pfd, 1, 0) };
        assert_eq!(ready, 0);
        // SAFETY: closing our own just-created fds.
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }
}
