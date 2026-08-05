//! user fsfreeze — suspend or resume modifications to a mounted
//! filesystem via the `FIFREEZE`/`FITHAW` ioctls.
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

use usercore::Ui;

/// `FIFREEZE`/`FITHAW` from `<linux/fs.h>` — verified against the system
/// header (see the `blockdev` port for the same verification method)
/// rather than pulled in via `linux-raw-sys`.
const FIFREEZE: libc::c_ulong = 0xc004_5877;
const FITHAW: libc::c_ulong = 0xc004_5878;

const HELP: &str = "Usage: fsfreeze --freeze|--unfreeze MOUNTPOINT\n\
Suspend or resume modifications to a mounted filesystem.\n\n\
  -f, --freeze     freeze the filesystem\n\
  -u, --unfreeze   unfreeze the filesystem\n\
  -h, --help       display this help and exit\n\
      --version    output version information and exit\n";

/// Entry point for the `fsfreeze` utility. Parses `std::env::args()`,
/// validates that the operand is a directory, and issues `FIFREEZE` or
/// `FITHAW` against it.
///
/// Returns 0 on success, 1 on a usage error, a non-directory operand, or
/// if opening/freezing/thawing fails (e.g. not root, or not the root of
/// a mounted filesystem).
pub fn run() -> i32 {
    let ui = Ui::new("fsfreeze");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("fsfreeze (user_utils) 0.1.0");
        return 0;
    }

    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    match freeze_or_thaw(&parsed.mountpoint, parsed.freeze) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!(
                "failed to {} the filesystem: {e}",
                if parsed.freeze { "freeze" } else { "unfreeze" }
            ));
            1
        }
    }
}

#[derive(Debug)]
struct Parsed {
    freeze: bool,
    mountpoint: String,
}

/// Parse `fsfreeze`'s options and mountpoint operand out of `args`
/// (already stripped of `argv[0]`; `--help`/`--version` handled by the
/// caller). Exactly one of `--freeze`/`--unfreeze` and exactly one
/// mountpoint operand are required.
fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut freeze: Option<bool> = None;
    let mut mountpoint: Option<String> = None;

    for a in args {
        match a.as_str() {
            "-f" | "--freeze" => {
                if freeze.is_some() {
                    return Err("only one of --freeze/--unfreeze may be given".to_string());
                }
                freeze = Some(true);
            }
            "-u" | "--unfreeze" => {
                if freeze.is_some() {
                    return Err("only one of --freeze/--unfreeze may be given".to_string());
                }
                freeze = Some(false);
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option -- '{s}'"));
            }
            other => {
                if mountpoint.is_some() {
                    return Err(format!("extra operand '{other}'"));
                }
                mountpoint = Some(other.to_string());
            }
        }
    }

    let freeze = freeze.ok_or_else(|| "one of --freeze/--unfreeze is required".to_string())?;
    let mountpoint = mountpoint.ok_or_else(|| "missing mountpoint operand".to_string())?;
    Ok(Parsed { freeze, mountpoint })
}

/// Open `mountpoint`, verify it's a directory, and issue `FIFREEZE`
/// (`freeze = true`) or `FITHAW` (`freeze = false`) against it.
fn freeze_or_thaw(mountpoint: &str, freeze: bool) -> io::Result<()> {
    let file = File::open(mountpoint)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a directory"));
    }

    let code = if freeze { FIFREEZE } else { FITHAW };
    // SAFETY: `file` stays open (and thus `fd` valid) for the duration of
    // the call; FIFREEZE/FITHAW ignore their third argument (the kernel
    // does not dereference it), so passing the immediate `0` is correct
    // for both, matching util-linux's own `fsfreeze` implementation.
    // `as _` because `libc::ioctl`'s request type differs by target libc
    // (c_ulong on glibc, c_int on musl).
    let ret = unsafe { libc::ioctl(file.as_raw_fd(), code as _, 0) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_args_freeze_with_mountpoint() {
        let p = parse_args(&s(&["--freeze", "/mnt"])).unwrap();
        assert!(p.freeze);
        assert_eq!(p.mountpoint, "/mnt");
    }

    #[test]
    fn parse_args_unfreeze_with_short_flag() {
        let p = parse_args(&s(&["-u", "/mnt"])).unwrap();
        assert!(!p.freeze);
        assert_eq!(p.mountpoint, "/mnt");
    }

    #[test]
    fn parse_args_requires_freeze_or_unfreeze() {
        let err = parse_args(&s(&["/mnt"])).unwrap_err();
        assert!(err.contains("--freeze/--unfreeze"));
    }

    #[test]
    fn parse_args_requires_mountpoint() {
        let err = parse_args(&s(&["--freeze"])).unwrap_err();
        assert!(err.contains("mountpoint"));
    }

    #[test]
    fn parse_args_rejects_both_freeze_and_unfreeze() {
        assert!(parse_args(&s(&["--freeze", "--unfreeze", "/mnt"])).is_err());
    }

    #[test]
    fn parse_args_rejects_extra_operand() {
        assert!(parse_args(&s(&["--freeze", "/mnt", "/extra"])).is_err());
    }

    #[test]
    fn parse_args_rejects_unknown_option() {
        assert!(parse_args(&s(&["--bogus", "/mnt"])).is_err());
    }

    #[test]
    fn freeze_on_regular_file_is_rejected_as_not_a_directory() {
        let path = std::env::temp_dir().join(format!(
            "user-fsfreeze-test-file-{}",
            std::process::id()
        ));
        std::fs::write(&path, b"not a directory").unwrap();
        let err = freeze_or_thaw(path.to_str().unwrap(), true).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn freeze_on_missing_path_reports_error_not_panic() {
        let err = freeze_or_thaw("/nonexistent/user-fsfreeze-test-mount", true).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn freeze_on_real_directory_without_privilege_fails_cleanly() {
        // FIFREEZE requires CAP_SYS_ADMIN; as an unprivileged user this
        // must surface a clean OS error rather than crash.
        let dir = std::env::temp_dir();
        let result = freeze_or_thaw(dir.to_str().unwrap(), true);
        assert!(result.is_err(), "expected freeze without privilege to fail");
    }

    #[test]
    fn ioctl_constants_match_linux_fs_h() {
        assert_eq!(FIFREEZE, 0xc0045877);
        assert_eq!(FITHAW, 0xc0045878);
    }
}
