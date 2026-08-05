//! user sync — flush filesystem buffers.
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use usercore::Ui;

/// Entry point for the `sync` utility. Parses `std::env::args()` and either
/// flushes all filesystem buffers (`sync(2)`, no operands) or synchronizes
/// each named file/its containing filesystem individually.
///
/// Returns 0 on success, 1 if any named path could not be opened or synced.
pub fn run() -> i32 {
    let ui = Ui::new("sync");
    let mut data_only = false;
    let mut file_sys = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("sync (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--data" => data_only = true,
            "-f" | "--file-system" => file_sys = true,
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => paths.push(PathBuf::from(other)),
        }
    }
    if paths.is_empty() {
        // SAFETY: `sync(2)` takes no arguments, dereferences no
        // pointers, and has no failure mode reported to the caller — it
        // cannot invoke UB.
        unsafe {
            libc::sync();
        }
        return 0;
    }
    let mut status = 0;
    for p in &paths {
        if let Err(e) = sync_path(p, data_only, file_sys) {
            ui.err(&format!("{}: {e}", p.display()));
            status = 1;
        }
    }
    status
}

fn print_help() {
    print!(
        "Usage: sync [OPTION] [FILE]...\n\
Synchronize cached writes to persistent storage.\n\n\
  -d, --data              sync only file data\n\
  -f, --file-system        sync the file systems that contain the FILEs\n\
      --help              display this help and exit\n\
      --version           output version information and exit\n"
    );
}

/// Open `path` and sync it (or the filesystem containing it, or just its
/// data) according to the `-f`/`-d` flags, then close it.
fn sync_path(path: &std::path::Path, data_only: bool, file_sys: bool) -> std::io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL")
    })?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` built from
    // `path`'s bytes and kept alive across this call, so `c.as_ptr()` is
    // a valid C string pointer; `open(2)` cannot invoke UB regardless
    // of whether the path exists (a failure is reported via a
    // negative return, checked below).
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `fd` was just returned by `open` above and confirmed
    // `>= 0` (i.e. a valid, open file descriptor owned by this
    // process); `syncfs`/`fdatasync`/`fsync` take only that integer
    // fd and have no pointer arguments.
    let rc = unsafe {
        if file_sys {
            libc::syncfs(fd)
        } else if data_only {
            libc::fdatasync(fd)
        } else {
            libc::fsync(fd)
        }
    };
    let sync_err = if rc != 0 {
        Some(std::io::Error::last_os_error())
    } else {
        None
    };
    // SAFETY: `fd` is still the same valid, open, and (aside from
    // the sync calls above, which don't close it) untouched
    // descriptor opened above; `close(2)` on an fd owned by this
    // process is well-defined even on error, and `fd` is not used
    // again after this point.
    unsafe {
        libc::close(fd);
    }
    match sync_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("user_sync_test_{}_{name}", std::process::id()))
    }

    #[test]
    fn sync_path_succeeds_on_regular_file() {
        let p = tmp("regular");
        let mut f = File::create(&p).unwrap();
        f.write_all(b"data").unwrap();
        assert!(sync_path(&p, false, false).is_ok());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sync_path_data_only_succeeds() {
        let p = tmp("data_only");
        File::create(&p).unwrap();
        assert!(sync_path(&p, true, false).is_ok());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sync_path_file_system_succeeds() {
        let p = tmp("file_system");
        File::create(&p).unwrap();
        assert!(sync_path(&p, false, true).is_ok());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sync_path_missing_file_errors() {
        let missing = tmp("does_not_exist");
        assert!(sync_path(&missing, false, false).is_err());
    }
}
