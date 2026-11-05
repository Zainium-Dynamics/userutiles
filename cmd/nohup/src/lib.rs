//! user nohup: run command immune to hangups, with output to nohup.out.
use std::fs::OpenOptions;
use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use usercore::Ui;

/// Entry point for the `nohup` utility. Parses `std::env::args()` as
/// `COMMAND [ARG]...`, ignores `SIGHUP`, redirects stdout/stderr to
/// `nohup.out` (or `$HOME/nohup.out`) if stdout is a terminal, and `exec`s
/// COMMAND in place.
///
/// Returns 127 on a usage error or if the target file/command couldn't be
/// opened/found, 126 on other exec failures; on success the process image
/// is replaced and `run` never returns to its caller.
pub fn run() -> i32 {
    let ui = Ui::new("nohup");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        if args.is_empty() {
            ui.err("missing operand");
            return 127;
        }
        print!("Usage: nohup COMMAND [ARG]...\nRun COMMAND, ignoring hangup signals.\n");
        return 0;
    }
    if args[0] == "--version" {
        println!("nohup (user_utils) 0.1.0");
        return 0;
    }

    // SAFETY: `libc::signal` is called with `SIGHUP` and the sentinel handler
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }

    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]);

    // SAFETY: `libc::isatty` is called with the fixed constant `STDOUT_FILENO`;
    let stdout_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) != 0 };
    if stdout_tty {
        match open_nohup_out() {
            Ok((path, f)) => {
                ui.err(&format!(
                    "ignoring input and appending output to '{}'",
                    path.display()
                ));
                let fd = f.as_raw_fd();
                // SAFETY: `fd` is the raw fd of `f`, an open `std::fs::File` that
                let dup_fd = unsafe { libc::dup(fd) };
                if dup_fd < 0 {
                    ui.err(&format!(
                        "failed to duplicate stdout fd: {}",
                        io::Error::last_os_error()
                    ));
                    return 127;
                }
                // SAFETY: `dup_fd` was just returned by a successful `dup(2)`
                let f2 = unsafe { std::fs::File::from_raw_fd(dup_fd) };
                cmd.stdout(Stdio::from(f));
                cmd.stderr(Stdio::from(f2));
            }
            Err(e) => {
                ui.err(&format!("failed to open nohup.out: {e}"));
                return 127;
            }
        }
    }
    // SAFETY: same as the earlier `isatty` call — fixed constant `STDIN_FILENO`,
    if unsafe { libc::isatty(libc::STDIN_FILENO) != 0 } {
        cmd.stdin(Stdio::null());
    }

    let err = cmd.exec();
    ui.err(&format!("{}: {err}", args[0]));
    if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// Open (creating if necessary, appending) `nohup.out` in the current
/// directory, falling back to `$HOME/nohup.out` if the current directory
/// isn't writable. Returns the path that was actually opened alongside the
/// open file handle.
fn open_nohup_out() -> io::Result<(PathBuf, std::fs::File)> {
    let cwd_path = PathBuf::from("nohup.out");
    match OpenOptions::new().create(true).append(true).open(&cwd_path) {
        Ok(f) => Ok((cwd_path, f)),
        Err(_) => {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            let dir = home.unwrap_or_else(|| PathBuf::from("."));
            let path = dir.join("nohup.out");
            let f = OpenOptions::new().create(true).append(true).open(&path)?;
            Ok((path, f))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn open_nohup_out_prefers_cwd_when_writable() {
        let _guard = CWD_LOCK.lock().unwrap();
        let scratch = env::temp_dir().join(format!(
            "user_nohup_test_cwd_{}_{}",
            std::process::id(),
            "a"
        ));
        std::fs::create_dir_all(&scratch).unwrap();
        let orig_cwd = env::current_dir().unwrap();
        env::set_current_dir(&scratch).unwrap();
        let result = open_nohup_out();
        env::set_current_dir(&orig_cwd).unwrap();
        let (path, _f) = result.unwrap();
        assert_eq!(path, PathBuf::from("nohup.out"));
        assert!(scratch.join("nohup.out").is_file());
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn open_nohup_out_falls_back_to_home_when_cwd_unwritable() {
        let _guard = CWD_LOCK.lock().unwrap();
        let ro_dir =
            env::temp_dir().join(format!("user_nohup_test_ro_{}_{}", std::process::id(), "b"));
        std::fs::create_dir_all(&ro_dir).unwrap();
        let home_dir = env::temp_dir().join(format!(
            "user_nohup_test_home_{}_{}",
            std::process::id(),
            "b"
        ));
        std::fs::create_dir_all(&home_dir).unwrap();

        let mut perms = std::fs::metadata(&ro_dir).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o555);
        std::fs::set_permissions(&ro_dir, perms.clone()).unwrap();

        let orig_cwd = env::current_dir().unwrap();
        let orig_home = env::var_os("HOME");
        env::set_current_dir(&ro_dir).unwrap();
        // SAFETY: test-only; this test process does not spawn threads that
        unsafe {
            env::set_var("HOME", &home_dir);
        }

        let result = open_nohup_out();

        env::set_current_dir(&orig_cwd).unwrap();
        // SAFETY: test-only; restoring prior env state, same reasoning as above.
        unsafe {
            match orig_home {
                Some(h) => env::set_var("HOME", h),
                None => env::remove_var("HOME"),
            }
        }
        let mut restore_perms = std::fs::metadata(&ro_dir).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut restore_perms, 0o755);
        let _ = std::fs::set_permissions(&ro_dir, restore_perms);

        // SAFETY: `libc::geteuid` takes no arguments and cannot fail or cause UB.
        if unsafe { libc::geteuid() } != 0 {
            let (path, _f) = result.unwrap();
            assert_eq!(path, home_dir.join("nohup.out"));
        }

        let _ = std::fs::remove_dir_all(&ro_dir);
        let _ = std::fs::remove_dir_all(&home_dir);
    }
}
