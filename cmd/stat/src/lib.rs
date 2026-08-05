//! user stat — display file or filesystem status.
use colored::Colorize;
use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

use usercore::Ui;

/// Entry point for the `stat` utility. Parses `std::env::args()` and prints
/// file or filesystem status for each operand.
///
/// Returns 0 on success, 1 if any operand could not be `stat`-ed.
pub fn run() -> i32 {
    let ui = Ui::new("stat");
    let mut terse = false;
    let mut filesystem = false;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("stat (user_utils) 0.1.0");
                return 0;
            }
            "-t" | "--terse" => terse = true,
            "-f" | "--file-system" => filesystem = true,
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let mut status = 0;
    for f in &files {
        let result = if filesystem {
            print_fs(f)
        } else {
            print_file(f, terse)
        };
        if let Err(e) = result {
            ui.err(&format!("{f}: {e}"));
            status = 1;
        }
    }
    status
}

fn print_help() {
    print!(
        "Usage: stat [OPTION]... FILE...\n\
Display file or file system status.\n\n\
  -t, --terse           print the information in terse form\n\
  -f, --file-system      display file system status instead of file status\n\
      --help             display this help and exit\n\
      --version          output version information and exit\n"
    );
}

/// Return a human-readable file-type label for `ft`, matching GNU `stat`'s
/// wording (e.g. `"regular file"`, `"directory"`).
fn file_type_label(ft: fs::FileType) -> &'static str {
    if ft.is_dir() {
        "directory"
    } else if ft.is_symlink() {
        "symbolic link"
    } else if ft.is_file() {
        "regular file"
    } else if ft.is_fifo() {
        "fifo"
    } else if ft.is_socket() {
        "socket"
    } else if ft.is_block_device() {
        "block special file"
    } else if ft.is_char_device() {
        "character special file"
    } else {
        "unknown"
    }
}

/// Print status for a single file (or symlink, without following it) at
/// `path`, in either terse (`-t`) or the default multi-line form.
fn print_file(path: &str, terse: bool) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let ft = meta.file_type();
    let file_type = file_type_label(ft);
    if terse {
        println!(
            "{} {} {:o} {} {} {} {} {} {} {} {} {}",
            path,
            meta.len(),
            meta.mode() & 0o7777,
            meta.uid(),
            meta.gid(),
            meta.dev(),
            meta.ino(),
            meta.nlink(),
            meta.atime(),
            meta.mtime(),
            meta.ctime(),
            file_type
        );
        return Ok(());
    }
    println!(" {} {}", "File:".bright_green(), path.bright_magenta());
    println!(
        " {} {} {} {}",
        "Size:".bright_green(),
        meta.len().to_string().bright_magenta(),
        "Blocks:".bright_green(),
        meta.blocks().to_string().bright_magenta()
    );
    println!(
        " {} {} {} {:o}",
        "Type:".bright_green(),
        file_type.bright_magenta(),
        "Access:".bright_green(),
        (meta.mode() & 0o7777)
    );
    println!(
        " {} {} {} {}",
        "Uid:".bright_green(),
        meta.uid().to_string().bright_magenta(),
        "Gid:".bright_green(),
        meta.gid().to_string().bright_magenta()
    );
    println!(
        " {} {} {} {}",
        "Device:".bright_green(),
        format!("{:x}h/{}d", meta.dev(), meta.dev()).bright_magenta(),
        "Inode:".bright_green(),
        meta.ino().to_string().bright_magenta()
    );
    println!(
        " {} {}",
        "Links:".bright_green(),
        meta.nlink().to_string().bright_magenta()
    );
    if ft.is_symlink() {
        if let Ok(t) = fs::read_link(path) {
            println!(
                " {} {}",
                "Link:".bright_green(),
                t.display().to_string().bright_cyan()
            );
        }
    }
    Ok(())
}

/// Print filesystem-level status (`-f`/`--file-system`) for the filesystem
/// containing `path`, via `statvfs(2)`.
fn print_fs(path: &str) -> io::Result<()> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    let c = CString::new(path)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let mut st = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c` is a valid, NUL-terminated `CString` derived from
    // `path` and kept alive for the duration of this call, so `c.as_ptr()`
    // is a valid C string pointer. `st.as_mut_ptr()` points to a
    // correctly-sized, properly-aligned (but possibly-uninitialized)
    // `libc::statvfs` buffer that `statvfs(2)` is documented to fully
    // populate on success — matching the `MaybeUninit` contract.
    let rc = unsafe { libc::statvfs(c.as_ptr(), st.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `rc == 0` above confirms `statvfs` returned success, which
    // per POSIX means it fully initialized `st`, so `assume_init` is
    // sound only on this success path (the early `return` above prevents
    // reaching here on failure).
    let st = unsafe { st.assume_init() };
    let bsize = st.f_frsize as u64;
    println!(
        " {} {}",
        "File:".bright_green(),
        format!("\"{path}\"").bright_magenta()
    );
    println!(
        " {} {} {} {}",
        "ID:".bright_green(),
        format!("{:x}", st.f_fsid).bright_magenta(),
        "Namelen:".bright_green(),
        (st.f_namemax).to_string().bright_magenta()
    );
    println!(
        " {} {} {} {}",
        "Block size:".bright_green(),
        bsize.to_string().bright_magenta(),
        "Blocks:".bright_green(),
        (st.f_blocks as u64).to_string().bright_magenta()
    );
    println!(
        " {} {} {} {}",
        "Free:".bright_green(),
        (st.f_bfree as u64).to_string().bright_magenta(),
        "Available:".bright_green(),
        (st.f_bavail as u64).to_string().bright_magenta()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::os::unix::net::UnixListener;

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("user_stat_test_{}_{name}", std::process::id()))
    }

    #[test]
    fn file_type_label_regular_file() {
        let p = tmp("regular");
        File::create(&p).unwrap();
        let ft = fs::symlink_metadata(&p).unwrap().file_type();
        assert_eq!(file_type_label(ft), "regular file");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn file_type_label_directory() {
        let p = tmp("dir");
        fs::create_dir(&p).unwrap();
        let ft = fs::symlink_metadata(&p).unwrap().file_type();
        assert_eq!(file_type_label(ft), "directory");
        let _ = fs::remove_dir(&p);
    }

    #[test]
    fn file_type_label_symlink_not_followed() {
        let target = tmp("symlink_target");
        let link = tmp("symlink_link");
        File::create(&target).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let ft = fs::symlink_metadata(&link).unwrap().file_type();
        assert_eq!(file_type_label(ft), "symbolic link");
        let _ = fs::remove_file(&target);
        let _ = fs::remove_file(&link);
    }

    #[test]
    fn file_type_label_socket() {
        let p = tmp("socket");
        let _ = fs::remove_file(&p);
        let listener = UnixListener::bind(&p).unwrap();
        let ft = fs::symlink_metadata(&p).unwrap().file_type();
        assert_eq!(file_type_label(ft), "socket");
        drop(listener);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn print_file_missing_path_errors() {
        let missing = tmp("does_not_exist");
        assert!(print_file(missing.to_str().unwrap(), false).is_err());
    }

    #[test]
    fn print_fs_root_succeeds() {
        assert!(print_fs("/").is_ok());
    }

    #[test]
    fn print_fs_missing_path_errors() {
        let missing = tmp("does_not_exist_fs");
        assert!(print_fs(missing.to_str().unwrap()).is_err());
    }
}
