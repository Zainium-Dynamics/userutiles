//! user lsattr — list ext2/3/4 file attributes.
//! Ported from e2fsprogs' `misc/lsattr.c` (GPL-2.0).
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use usercore::Ui;

// ext2/3/4 inode flag bits (linux/fs.h — stable ABI, mirrors e2fsprogs'
// ext2fs/ext2_fs.h). libc doesn't expose these.
const FS_SECRM_FL: u32 = 0x0000_0001;
const FS_UNRM_FL: u32 = 0x0000_0002;
const FS_COMPR_FL: u32 = 0x0000_0004;
const FS_SYNC_FL: u32 = 0x0000_0008;
const FS_IMMUTABLE_FL: u32 = 0x0000_0010;
const FS_APPEND_FL: u32 = 0x0000_0020;
const FS_NODUMP_FL: u32 = 0x0000_0040;
const FS_NOATIME_FL: u32 = 0x0000_0080;
const FS_NOCOMPR_FL: u32 = 0x0000_0400;
const FS_ENCRYPT_FL: u32 = 0x0000_0800;
const FS_INDEX_FL: u32 = 0x0000_1000;
const FS_JOURNAL_DATA_FL: u32 = 0x0000_4000;
const FS_NOTAIL_FL: u32 = 0x0000_8000;
const FS_DIRSYNC_FL: u32 = 0x0001_0000;
const FS_TOPDIR_FL: u32 = 0x0002_0000;
const FS_EXTENTS_FL: u32 = 0x0008_0000;
const FS_VERITY_FL: u32 = 0x0010_0000;
const FS_NOCOW_FL: u32 = 0x0080_0000;
const FS_DAX_FL: u32 = 0x0200_0000;
const FS_INLINE_DATA_FL: u32 = 0x1000_0000;
const FS_PROJINHERIT_FL: u32 = 0x2000_0000;
const FS_CASEFOLD_FL: u32 = 0x4000_0000;

/// (bit, short char, long name), matching e2fsprogs' `pf.c` `flags_array`
/// exactly — order is display order.
const DISPLAY_FLAGS: &[(u32, char, &str)] = &[
    (FS_SECRM_FL, 's', "Secure_Deletion"),
    (FS_UNRM_FL, 'u', "Undelete"),
    (FS_SYNC_FL, 'S', "Synchronous_Updates"),
    (FS_DIRSYNC_FL, 'D', "Synchronous_Directory_Updates"),
    (FS_IMMUTABLE_FL, 'i', "Immutable"),
    (FS_APPEND_FL, 'a', "Append_Only"),
    (FS_NODUMP_FL, 'd', "No_Dump"),
    (FS_NOATIME_FL, 'A', "No_Atime"),
    (FS_COMPR_FL, 'c', "Compression_Requested"),
    (FS_ENCRYPT_FL, 'E', "Encrypted"),
    (FS_JOURNAL_DATA_FL, 'j', "Journaled_Data"),
    (FS_INDEX_FL, 'I', "Indexed_directory"),
    (FS_NOTAIL_FL, 't', "No_Tailmerging"),
    (FS_TOPDIR_FL, 'T', "Top_of_Directory_Hierarchies"),
    (FS_EXTENTS_FL, 'e', "Extents"),
    (FS_NOCOW_FL, 'C', "No_COW"),
    (FS_DAX_FL, 'x', "DAX"),
    (FS_CASEFOLD_FL, 'F', "Casefold"),
    (FS_INLINE_DATA_FL, 'N', "Inline_Data"),
    (FS_PROJINHERIT_FL, 'P', "Project_Hierarchy"),
    (FS_VERITY_FL, 'V', "Verity"),
    (FS_NOCOMPR_FL, 'm', "Dont_Compress"),
];

/// Render `flags` the way `lsattr`'s `print_flags` does: short mode is one
/// char (or `-`) per known bit in table order; long mode (`-l`) is a
/// comma-joined list of the set bits' long names, or `---` if none.
fn print_flags(flags: u32, long: bool) -> String {
    if long {
        let names: Vec<&str> = DISPLAY_FLAGS
            .iter()
            .filter(|(bit, _, _)| flags & bit != 0)
            .map(|(_, _, name)| *name)
            .collect();
        if names.is_empty() {
            "---".to_string()
        } else {
            names.join(", ")
        }
    } else {
        DISPLAY_FLAGS
            .iter()
            .map(|(bit, c, _)| if flags & bit != 0 { *c } else { '-' })
            .collect()
    }
}

const FS_IOC_FSGETXATTR: libc::c_ulong = 0x801c_581f;

/// Mirrors the kernel's `struct fsxattr` (linux/fs.h) used by
/// `FS_IOC_FSGETXATTR` for the project id.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FsXAttr {
    fsx_xflags: u32,
    fsx_extsize: u32,
    fsx_nextents: u32,
    fsx_projid: u32,
    fsx_pad: [u8; 12],
}

fn open_for_ioctl(path: &Path, nofollow: bool) -> io::Result<fs::File> {
    let mut custom = libc::O_NONBLOCK;
    if nofollow {
        custom |= libc::O_NOFOLLOW;
    }
    OpenOptions::new()
        .read(true)
        .custom_flags(custom)
        .open(path)
}

fn get_flags(path: &Path) -> io::Result<u32> {
    let f = open_for_ioctl(path, true)?;
    let mut flags: libc::c_int = 0;
    // SAFETY: `flags` is a valid 4-byte out-param; e2fsprogs' own
    // `fgetflags` uses the same `int` (not the `long` the ioctl number's
    // encoded size suggests — the kernel handler always copies an `int`
    // regardless), and `f` stays open for the call.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), libc::FS_IOC_GETFLAGS as _, &mut flags) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(flags as u32)
}

fn get_version(path: &Path) -> io::Result<u32> {
    let f = open_for_ioctl(path, false)?;
    let mut ver: libc::c_int = 0;
    // SAFETY: `ver` is a valid 4-byte out-param for FS_IOC_GETVERSION.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), libc::FS_IOC_GETVERSION as _, &mut ver) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(ver as u32)
}

fn get_project(path: &Path) -> io::Result<u32> {
    let f = open_for_ioctl(path, false)?;
    let mut fsx = FsXAttr::default();
    // SAFETY: `fsx` is a correctly-sized (28-byte) `fsxattr` out-param for
    // FS_IOC_FSGETXATTR; `f` stays open for the call.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), FS_IOC_FSGETXATTR as _, &mut fsx) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(fsx.fsx_projid)
}

#[derive(Default, Clone, Copy)]
struct Options {
    all: bool,
    dirs_opt: bool,
    long: bool,
    recursive: bool,
    generation_opt: bool,
    project_opt: bool,
}

fn list_attributes(ui: &Ui, opts: Options, name: &Path) -> bool {
    let flags = match get_flags(name) {
        Ok(f) => f,
        Err(e) => {
            ui.err(&format!("while reading flags on {}: {e}", name.display()));
            return false;
        }
    };

    let mut line = String::new();
    if opts.project_opt {
        match get_project(name) {
            Ok(p) => line.push_str(&format!("{p:5} ")),
            Err(e) => {
                ui.err(&format!("while reading project on {}: {e}", name.display()));
                return false;
            }
        }
    }
    if opts.generation_opt {
        match get_version(name) {
            Ok(v) => line.push_str(&format!("{v:<10} ")),
            Err(e) => {
                ui.err(&format!("while reading version on {}: {e}", name.display()));
                return false;
            }
        }
    }
    if opts.long {
        line.push_str(&format!(
            "{:<28} {}",
            name.display(),
            print_flags(flags, true)
        ));
    } else {
        line.push_str(&format!("{} {}", print_flags(flags, false), name.display()));
    }
    println!("{line}");
    true
}

fn lsattr_dir_proc(ui: &Ui, opts: Options, dir_name: &Path) {
    let entries = match fs::read_dir(dir_name) {
        Ok(e) => e,
        Err(e) => {
            ui.err(&format!("{}: {e}", dir_name.display()));
            return;
        }
    };
    let mut names: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    names.sort();
    for path in names {
        let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let is_dotfile = file_name.as_deref().is_some_and(|n| n.starts_with('.'));
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                ui.err(&format!("{}: {e}", path.display()));
                continue;
            }
        };
        if !is_dotfile || opts.all {
            list_attributes(ui, opts, &path);
            if meta.is_dir() && opts.recursive {
                println!("\n{}:", path.display());
                lsattr_dir_proc(ui, opts, &path);
                println!();
            }
        }
    }
}

fn lsattr_args(ui: &Ui, opts: Options, name: &Path) -> bool {
    match fs::symlink_metadata(name) {
        Ok(m) if m.is_dir() && !opts.dirs_opt => {
            lsattr_dir_proc(ui, opts, name);
            true
        }
        Ok(_) => list_attributes(ui, opts, name),
        Err(e) => {
            ui.err(&format!("while trying to stat {}: {e}", name.display()));
            false
        }
    }
}

fn usage(ui: &Ui) -> i32 {
    ui.err("Usage: lsattr [-RVadlvp] [files...]");
    1
}

/// Entry point for the `lsattr` utility. Parses `std::env::args()` and
/// prints ext2/3/4 attributes for each FILE (or `.` with none given),
/// recursing into directories unless `-d` was given.
///
/// Returns 0 on success, 1 if any file could not be read or an option was
/// invalid.
pub fn run() -> i32 {
    let ui = Ui::new("lsattr");
    let mut opts = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut end_opts = false;

    for arg in std::env::args().skip(1) {
        if end_opts || !arg.starts_with('-') || arg == "-" {
            files.push(arg);
            continue;
        }
        if arg == "--" {
            end_opts = true;
            continue;
        }
        for c in arg.chars().skip(1) {
            match c {
                'R' => opts.recursive = true,
                'V' => {
                    eprintln!("lsattr (user_utils) 0.1.0");
                }
                'a' => opts.all = true,
                'd' => opts.dirs_opt = true,
                'l' => opts.long = true,
                'v' => opts.generation_opt = true,
                'p' => opts.project_opt = true,
                _ => return usage(&ui),
            }
        }
    }

    let mut status = 0;
    if files.is_empty() {
        if !lsattr_args(&ui, opts, Path::new(".")) {
            status = 1;
        }
    } else {
        for f in &files {
            if !lsattr_args(&ui, opts, Path::new(f)) {
                status = 1;
            }
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_flags_short_marks_set_bits_and_dashes_rest() {
        let s = print_flags(FS_IMMUTABLE_FL | FS_APPEND_FL, false);
        assert_eq!(s.len(), DISPLAY_FLAGS.len());
        assert!(s.contains('i'));
        assert!(s.contains('a'));
    }

    #[test]
    fn print_flags_long_lists_names_or_dashes() {
        assert_eq!(print_flags(0, true), "---");
        assert_eq!(print_flags(FS_IMMUTABLE_FL, true), "Immutable");
    }

    #[test]
    fn list_attributes_reads_a_real_file() {
        let dir = std::env::temp_dir().join(format!("user_lsattr_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        fs::write(&file, b"x").unwrap();

        let ui = Ui::new("lsattr");
        assert!(list_attributes(&ui, Options::default(), &file));

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn lsattr_args_missing_file_errors() {
        let ui = Ui::new("lsattr");
        let missing = std::env::temp_dir().join(format!(
            "user_lsattr_test_missing_{}_does_not_exist",
            std::process::id()
        ));
        assert!(!lsattr_args(&ui, Options::default(), &missing));
    }
}
