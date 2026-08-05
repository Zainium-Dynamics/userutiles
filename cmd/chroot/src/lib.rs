//! user chroot — run command or interactive shell with special root directory.
use std::ffi::CString;
use std::os::unix::process::CommandExt;
use std::process::Command;

use usercore::Ui;

/// Entry point for the `chroot` utility. Parses `std::env::args()`,
/// calls `chroot(2)` to `NEWROOT`, `chdir`s to `/` inside it, then
/// `exec`s `COMMAND` (or an interactive shell if none was given) —
/// this function only returns if `exec` itself fails.
///
/// Returns 0 for `--help`/`--version`, 125 on a usage or `chroot(2)`
/// failure, 126 if the command exists but couldn't be executed, 127 if
/// it wasn't found (matching GNU `chroot`'s exit code convention).
pub fn run() -> i32 {
    let ui = Ui::new("chroot");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        if args.is_empty() {
            ui.err("missing operand");
            return 125;
        }
        print!("Usage: chroot NEWROOT [COMMAND [ARG]...]\nRun COMMAND with root directory set to NEWROOT.\n");
        return 0;
    }
    if args[0] == "--version" {
        println!("chroot (user_utils) 0.1.0");
        return 0;
    }
    let newroot = &args[0];
    let c = match CString::new(newroot.as_str()) {
        Ok(c) => c,
        Err(_) => {
            ui.err("invalid path");
            return 125;
        }
    };
    // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for the
    // duration of this call, so `c.as_ptr()` is a sound `chroot(2)`
    // argument. `chroot` does not retain the pointer past the call and
    // reports failure via a normal errno/return-value contract, which is
    // handled below.
    let rc = unsafe { libc::chroot(c.as_ptr()) };
    if rc != 0 {
        ui.err(&format!(
            "cannot change root directory to '{newroot}': {}",
            std::io::Error::last_os_error()
        ));
        return 125;
    }
    let _ = std::env::set_current_dir("/");

    let (cmd, rest) = parse_command(&args);
    let err = Command::new(&cmd).args(&rest).exec();
    ui.err(&format!("failed to run command '{cmd}': {err}"));
    if err.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// Split `args` (`[NEWROOT, COMMAND, ARG...]`, `argv[0]` already
/// stripped) into the command to run and its arguments. With no
/// `COMMAND`, falls back to an interactive shell (see `resolve_shell`).
fn parse_command(args: &[String]) -> (String, Vec<String>) {
    if args.len() <= 1 {
        (resolve_shell(), Vec::new())
    } else {
        (args[1].clone(), args[2..].to_vec())
    }
}

/// Pick a shell to run when no `COMMAND` was given: prefer `sh` found
/// via Zainium's standard `PATH` directories, then `$SHELL`, then
/// `/bin/sh` as a last-resort fallback.
fn resolve_shell() -> String {
    let shell_env = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    usercore::zainium::path_dirs()
        .into_iter()
        .map(|d| d.join("sh"))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
        .unwrap_or(shell_env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_with_command_and_args() {
        let args = vec![
            "/newroot".to_string(),
            "/bin/ls".to_string(),
            "-la".to_string(),
            "/tmp".to_string(),
        ];
        let (cmd, rest) = parse_command(&args);
        assert_eq!(cmd, "/bin/ls");
        assert_eq!(rest, vec!["-la".to_string(), "/tmp".to_string()]);
    }

    #[test]
    fn parse_command_with_command_no_args() {
        let args = vec!["/newroot".to_string(), "/bin/true".to_string()];
        let (cmd, rest) = parse_command(&args);
        assert_eq!(cmd, "/bin/true");
        assert!(rest.is_empty());
    }

    #[test]
    fn parse_command_no_command_falls_back_to_shell() {
        let args = vec!["/newroot".to_string()];
        let (cmd, rest) = parse_command(&args);
        assert!(!cmd.is_empty());
        assert!(rest.is_empty());
    }

    #[test]
    fn resolve_shell_never_returns_empty() {
        assert!(!resolve_shell().is_empty());
    }
}
