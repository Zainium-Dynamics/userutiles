//! user pwd
use std::env;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use usercore::Ui;

/// Entry point for the `pwd` utility. Parses `std::env::args_os()` and
/// prints the current working directory: logically (`-L`, default — trusts
/// `$PWD` from the environment when it still names the same directory as
/// the real cwd) or physically (`-P` — always resolved via
/// [`Path::canonicalize`], with all symlinks followed).
///
/// Returns 0 on success, 1 if the current directory could not be
/// determined, or 2 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("pwd");
    let mut logical = true;
    for arg in env::args_os().skip(1) {
        let s = arg.to_string_lossy();
        match s.as_ref() {
            "-L" | "--logical" => logical = true,
            "-P" | "--physical" => logical = false,
            "-h" | "--help" => {
                print!("Usage: pwd [OPTION]...\nPrint the full filename of the current working directory.\n -L, --logical use PWD from environment\n -P, --physical avoid all symlinks\n");
                return 0;
            }
            "--version" => {
                println!("pwd (user_utils) 0.1.0");
                return 0;
            }
            _ if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 2;
            }
            _ => {}
        }
    }
    let bytes = if logical {
        if let Some(pwd) = env::var_os("PWD") {
            if logical_pwd_matches_cwd(&pwd) {
                emit(pwd.as_bytes());
                return 0;
            }
        }
        match env::current_dir() {
            Ok(p) => p.into_os_string(),
            Err(e) => {
                ui.err(&format!("{e}"));
                return 1;
            }
        }
    } else {
        match env::current_dir().and_then(|p| p.canonicalize()) {
            Ok(p) => p.into_os_string(),
            Err(e) => {
                ui.err(&format!("{e}"));
                return 1;
            }
        }
    };
    emit(bytes.as_bytes());
    0
}

/// True if `pwd` (a candidate `$PWD` value) is an absolute, existing path
/// that canonicalizes to the same directory as the actual current working
/// directory — i.e. it's safe to trust `$PWD` verbatim (preserving any
/// symlink components a caller may have `cd`'d through) instead of falling
/// back to [`env::current_dir`].
fn logical_pwd_matches_cwd(pwd: &OsStr) -> bool {
    let p = Path::new(pwd);
    if !p.is_absolute() || !p.exists() {
        return false;
    }
    let (Ok(cur), Ok(canon_pwd)) = (env::current_dir(), p.canonicalize()) else {
        return false;
    };
    let Ok(canon_cur) = cur.canonicalize() else {
        return false;
    };
    canon_pwd == canon_cur
}

fn emit(bytes: &[u8]) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(bytes);
    let _ = out.write_all(b"\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_pwd_rejects_relative_path() {
        assert!(!logical_pwd_matches_cwd(OsStr::new("some/relative/dir")));
    }

    #[test]
    fn logical_pwd_rejects_nonexistent_path() {
        assert!(!logical_pwd_matches_cwd(OsStr::new(
            "/no/such/directory/user_pwd_test"
        )));
    }

    #[test]
    fn logical_pwd_accepts_real_cwd() {
        let cwd = env::current_dir().expect("current_dir");
        assert!(logical_pwd_matches_cwd(cwd.as_os_str()));
    }

    #[test]
    fn logical_pwd_rejects_different_existing_dir() {
        // "/" exists and is absolute but (almost certainly) isn't the test
        // process's cwd, so this must not be treated as a match.
        let cwd = env::current_dir().expect("current_dir");
        if cwd != Path::new("/") {
            assert!(!logical_pwd_matches_cwd(OsStr::new("/")));
        }
    }
}
