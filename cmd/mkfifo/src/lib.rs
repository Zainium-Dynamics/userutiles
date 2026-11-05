//! user mkfifo: create named pipes.
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use usercore::Ui;

/// Entry point for the `mkfifo` utility. Parses `std::env::args()` and
/// creates each NAME as a FIFO via `mkfifo(2)`, applying `-m MODE`
/// (default `0666`) minus the process umask: matching GNU `mkfifo`'s
/// permission semantics.
///
/// Returns 0 if every FIFO was created, 1 on a usage error or if any
/// individual `mkfifo(2)` call failed.
pub fn run() -> i32 {
    let ui = Ui::new("mkfifo");
    let mut mode = 0o666;
    let mut paths: Vec<PathBuf> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: mkfifo [OPTION]... NAME...\nCreate named pipes (FIFOs).\n -m, --mode=MODE set file permission bits\n");
                return 0;
            }
            "--version" => {
                println!("mkfifo (user_utils) 0.1.0");
                return 0;
            }
            "-m" | "--mode" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("666");
                let Some(m) = parse_mode(spec) else {
                    ui.err(&format!("invalid mode '{spec}'"));
                    return 1;
                };
                mode = m;
            }
            s if s.starts_with("-m") && s.len() > 2 => {
                let Some(m) = parse_mode(&s[2..]) else {
                    ui.err(&format!("invalid mode '{}'", &s[2..]));
                    return 1;
                };
                mode = m;
            }
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => paths.push(PathBuf::from(other)),
        }
        i += 1;
    }
    if paths.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let mut status = 0;
    let final_mode = (mode & !process_umask()) as libc::mode_t;
    for p in &paths {
        let c = match CString::new(p.as_os_str().as_bytes()) {
            Ok(c) => c,
            Err(_) => {
                ui.err(&format!(
                    "cannot create fifo '{}': path contains NUL",
                    p.display()
                ));
                status = 1;
                continue;
            }
        };
        // SAFETY: `c` is a `CString` that owns a NUL-terminated buffer and is kept alive
        let rc = unsafe { libc::mkfifo(c.as_ptr(), final_mode) };
        if rc != 0 {
            ui.err(&format!(
                "cannot create fifo '{}': {}",
                p.display(),
                std::io::Error::last_os_error()
            ));
            status = 1;
        }
    }
    status
}

/// Parse an octal permission-mode string (e.g. `"666"`), returning `None`
/// if `s` is not valid base-8. Symbolic mode strings (`u+x`, ...) are not
/// supported.
fn parse_mode(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 8).ok()
}

/// Read the calling process's umask without changing it: `umask(2)` only
/// offers a set-and-return-previous interface, so this reads it by
/// setting to `0` and immediately restoring the original value.
fn process_umask() -> u32 {
    // SAFETY: `libc::umask` takes a `mode_t` by value and unconditionally returns the
    let mask = unsafe { libc::umask(0) };
    // SAFETY: same as above — `libc::umask` cannot fail or cause UB. This restores the
    unsafe {
        libc::umask(mask);
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_accepts_valid_octal() {
        assert_eq!(parse_mode("666"), Some(0o666));
        assert_eq!(parse_mode("0"), Some(0));
        assert_eq!(parse_mode("777"), Some(0o777));
    }

    #[test]
    fn parse_mode_rejects_invalid_input() {
        assert_eq!(parse_mode("rw-rw-rw-"), None);
        assert_eq!(parse_mode("899"), None);
        assert_eq!(parse_mode(""), None);
    }

    #[test]
    fn process_umask_is_idempotent() {
        let a = process_umask();
        let b = process_umask();
        assert_eq!(a, b);
    }

    #[test]
    fn mkfifo_creates_a_fifo() {
        use std::os::unix::fs::FileTypeExt;
        let dir = std::env::temp_dir().join(format!("user_mkfifo_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pipe");
        let c = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for the
        let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
        assert_eq!(rc, 0);
        let meta = std::fs::symlink_metadata(&path).unwrap();
        assert!(meta.file_type().is_fifo());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
