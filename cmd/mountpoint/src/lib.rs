//! user mountpoint — check whether a directory is a mountpoint, using the
//! standard st_dev/st_ino heuristic: a directory is a mountpoint if it
//! sits on a different device than its parent, or (for the root of a
//! bind mount / filesystem root) shares its parent's inode number.
use std::ffi::CString;
use std::io;
use std::path::Path;

use usercore::Ui;

const HELP: &str = "Usage: mountpoint [options] <directory>\n\
       mountpoint [options] -x <device>\n\
Check whether a directory is a mountpoint.\n\n\
  -q, --quiet      quiet mode - don't print anything\n\
  -d, --fs-devno   print the major/minor device number of the filesystem\n\
  -x, --devno      print the major/minor device number of the blockdevice\n\
  -n, --nofollow   do not follow symlink\n\
  -h, --help       display this help and exit\n\
      --version    output version information and exit\n";

/// Entry point for the `mountpoint` utility.
///
/// Exit codes match util-linux: 0 if the directory is a mountpoint (or
/// `-d`/`-x` printed successfully), 1 if it exists but is not a
/// mountpoint, 32 on usage/stat errors.
pub fn run() -> i32 {
    let ui = Ui::new("mountpoint");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut quiet = false;
    let mut fs_devno = false;
    let mut devno = false;
    let mut nofollow = false;
    let mut path: Option<String> = None;

    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("mountpoint (user_utils) 0.1.0");
                return 0;
            }
            "-q" | "--quiet" => quiet = true,
            "-d" | "--fs-devno" => fs_devno = true,
            "-x" | "--devno" => devno = true,
            "-n" | "--nofollow" => nofollow = true,
            other if !other.starts_with('-') || other == "-" => {
                if path.is_some() {
                    ui.err("only one path may be given");
                    return 32;
                }
                path = Some(other.to_string());
            }
            other => {
                ui.err(&format!("unknown option -- '{other}'"));
                return 32;
            }
        }
    }

    let Some(path) = path else {
        ui.err("no directory given");
        return 32;
    };

    if devno {
        return match device_number(&path) {
            Ok((maj, min)) => {
                println!("{maj}:{min}");
                0
            }
            Err(e) => {
                ui.err(&format!("{path}: {e}"));
                32
            }
        };
    }

    let st = match stat_path(&path, !nofollow) {
        Ok(st) => st,
        Err(e) => {
            ui.err(&format!("{path}: {e}"));
            return 32;
        }
    };

    if st.st_mode & libc::S_IFMT != libc::S_IFDIR {
        ui.err(&format!("{path}: not a directory"));
        return 32;
    }

    if fs_devno {
        let (maj, min) = split_devno(st.st_dev);
        println!("{maj}:{min}");
        return 0;
    }

    let parent = Path::new(&path).join("..");
    let is_mp = match stat_path(&parent.to_string_lossy(), true) {
        Ok(parent_st) => st.st_dev != parent_st.st_dev || st.st_ino == parent_st.st_ino,
        Err(_) => false,
    };

    if !quiet {
        if is_mp {
            println!("{path} is a mountpoint");
        } else {
            println!("{path} is not a mountpoint");
        }
    }

    if is_mp {
        0
    } else {
        1
    }
}

fn stat_path(path: &str, follow: bool) -> io::Result<libc::stat> {
    let c_path = CString::new(path).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: `st` is fully populated by a successful `stat`/`lstat` call
    // before use; `c_path` is a valid NUL-terminated string for the
    // duration of the call.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        let rc = if follow {
            libc::stat(c_path.as_ptr(), &mut st)
        } else {
            libc::lstat(c_path.as_ptr(), &mut st)
        };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(st)
    }
}

/// Prints the device number of `path` itself: if it is a block device,
/// its own major:minor; otherwise the device the file resides on.
fn device_number(path: &str) -> io::Result<(u32, u32)> {
    let st = stat_path(path, true)?;
    if st.st_mode & libc::S_IFMT == libc::S_IFBLK {
        Ok(split_devno(st.st_rdev))
    } else {
        Ok(split_devno(st.st_dev))
    }
}

/// Splits a Linux `dev_t` into (major, minor), matching glibc's
/// `gnu_dev_major`/`gnu_dev_minor` bit layout.
fn split_devno(dev: libc::dev_t) -> (u32, u32) {
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfffu64);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xffu64);
    (major as u32, minor as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_devno_recovers_common_encoding() {
        // /dev/sda1 is typically major 8, minor 1 -> dev_t 0x0801.
        let (maj, min) = split_devno(0x0801);
        assert_eq!((maj, min), (8, 1));
    }

    #[test]
    fn split_devno_handles_high_minor() {
        // major 259 (nvme), minor 300 encoded per glibc layout.
        let dev: u64 = ((259u64 & 0xfff) << 8) | (300u64 & 0xff) | ((300u64 & !0xffu64) << 12);
        let (maj, min) = split_devno(dev);
        assert_eq!((maj, min), (259, 300));
    }
}
