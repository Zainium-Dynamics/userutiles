//! user mknod — create special files.
use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `mknod` utility. Parses `std::env::args()` as
/// `[OPTION]... NAME TYPE [MAJOR MINOR]` and creates the special file NAME
/// via the `mknod(2)` syscall.
///
/// Returns 0 on success, 1 on a usage or creation error.
pub fn run() -> i32 {
    let ui = Ui::new("mknod");
    let mut mode = 0o666u32;
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print!("Usage: mknod [OPTION]... NAME TYPE [MAJOR MINOR]\nCreate a special file.\nTYPE is b (block), c or u (char), or p (FIFO).\n -m, --mode=MODE set permission bits\n");
        return if args.is_empty() { 1 } else { 0 };
    }
    if args[0] == "--version" {
        println!("mknod (user_utils) 0.1.0");
        return 0;
    }
    let mut i = 0;
    if args[i] == "-m" || args[i] == "--mode" {
        i += 1;
        let Some(m) = args.get(i) else {
            ui.err("option requires an argument -- 'm'");
            return 1;
        };
        mode = match parse_mode(m) {
            Ok(m) => m,
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        };
        i += 1;
    }
    let Some(name_arg) = args.get(i) else {
        ui.err("missing operand");
        return 1;
    };
    let name = PathBuf::from(name_arg);
    i += 1;
    let Some(ty) = args.get(i).map(String::as_str) else {
        ui.err(&format!("missing operand after '{}'", name.display()));
        return 1;
    };
    i += 1;

    let (file_type, major, minor) = match ty {
        "p" => (libc::S_IFIFO, 0, 0),
        "b" | "c" | "u" => {
            let (maj_s, min_s) = match (args.get(i), args.get(i + 1)) {
                (Some(a), Some(b)) => (a, b),
                _ => {
                    ui.err("missing major/minor");
                    return 1;
                }
            };
            let maj = match parse_dev_num(maj_s) {
                Ok(n) => n,
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            };
            let min = match parse_dev_num(min_s) {
                Ok(n) => n,
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            };
            let t = if ty == "b" {
                libc::S_IFBLK
            } else {
                libc::S_IFCHR
            };
            (t, maj, min)
        }
        _ => {
            ui.err(&format!("invalid type '{ty}'"));
            return 1;
        }
    };

    if let Err(e) = make_node(&name, file_type, mode, major, minor) {
        ui.err(&format!("{}: {e}", name.display()));
        return 1;
    }
    0
}

/// Parse a permission-bits string (interpreted as octal, matching GNU
/// `mknod -m`) into a raw mode value.
fn parse_mode(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 8).map_err(|_| format!("invalid mode '{s}'"))
}

/// Parse a major/minor device number argument as an unsigned integer.
fn parse_dev_num(s: &str) -> Result<u64, String> {
    s.parse::<u64>()
        .map_err(|_| format!("invalid device number '{s}'"))
}

/// Create the special file `path` via `mknod(2)` with the given
/// `file_type` (one of `S_IFIFO`/`S_IFBLK`/`S_IFCHR`), permission `mode`,
/// and (for block/char devices) `major`/`minor` numbers.
fn make_node(
    path: &Path,
    file_type: libc::mode_t,
    mode: u32,
    major: u64,
    minor: u64,
) -> io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?;
    let dev = libc::makedev(major as libc::c_uint, minor as libc::c_uint);
    let m = (file_type | (mode as libc::mode_t)) as libc::mode_t;
    // SAFETY: `c` is a `CString` kept alive across this call, so `c.as_ptr()` is a
    // valid pointer to a NUL-terminated C string as `mknod` requires. `m` and `dev`
    // are plain integer values built above from parsed CLI input, not pointers, so
    // they cannot themselves cause UB; any failure is reported via `errno`.
    let rc = unsafe { libc::mknod(c.as_ptr(), m, dev) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "user_mknod_test_{tag}_{}_{}",
            std::process::id(),
            tag
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn parse_mode_accepts_octal() {
        assert_eq!(parse_mode("666"), Ok(0o666));
        assert_eq!(parse_mode("0"), Ok(0));
        assert_eq!(parse_mode("777"), Ok(0o777));
    }

    #[test]
    fn parse_mode_rejects_non_octal() {
        assert!(parse_mode("9").is_err());
        assert!(parse_mode("xyz").is_err());
        assert!(parse_mode("").is_err());
    }

    #[test]
    fn parse_dev_num_accepts_digits() {
        assert_eq!(parse_dev_num("0"), Ok(0));
        assert_eq!(parse_dev_num("42"), Ok(42));
    }

    #[test]
    fn parse_dev_num_rejects_non_numeric() {
        assert!(parse_dev_num("abc").is_err());
        assert!(parse_dev_num("-1").is_err());
        assert!(parse_dev_num("").is_err());
    }

    #[test]
    fn make_node_creates_fifo_without_root() {
        // FIFOs don't require root, unlike block/char devices, so this is
        // hermetically testable.
        let dir = scratch_dir("fifo");
        let path = dir.join("myfifo");
        let _ = std::fs::remove_file(&path);
        make_node(&path, libc::S_IFIFO, 0o600, 0, 0).unwrap();
        let meta = std::fs::symlink_metadata(&path).unwrap();
        assert!(
            std::os::unix::fs::FileTypeExt::is_fifo(&meta.file_type()),
            "expected a FIFO at {}",
            path.display()
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn make_node_reports_error_on_existing_file() {
        let dir = scratch_dir("exists");
        let path = dir.join("already_there");
        std::fs::write(&path, b"x").unwrap();
        let err = make_node(&path, libc::S_IFIFO, 0o600, 0, 0).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn make_node_rejects_nul_in_path() {
        let bad = PathBuf::from("has\0nul");
        let err = make_node(&bad, libc::S_IFIFO, 0o600, 0, 0).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
