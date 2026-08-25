//! user chattr — change ext2/3/4 file attributes.
//! Ported from e2fsprogs' `misc/chattr.c` (GPL-2.0).
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

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
const FS_JOURNAL_DATA_FL: u32 = 0x0000_4000;
const FS_NOTAIL_FL: u32 = 0x0000_8000;
const FS_DIRSYNC_FL: u32 = 0x0001_0000;
const FS_TOPDIR_FL: u32 = 0x0002_0000;
const FS_EXTENTS_FL: u32 = 0x0008_0000;
const FS_NOCOW_FL: u32 = 0x0080_0000;
const FS_DAX_FL: u32 = 0x0200_0000;
const FS_PROJINHERIT_FL: u32 = 0x2000_0000;
const FS_CASEFOLD_FL: u32 = 0x4000_0000;

/// (bit, option char) pairs `chattr` can set/clear — matches e2fsprogs'
/// `chattr.c` `flags_array` exactly.
const SETTABLE_FLAGS: &[(u32, char)] = &[
    (FS_NOATIME_FL, 'A'),
    (FS_SYNC_FL, 'S'),
    (FS_DIRSYNC_FL, 'D'),
    (FS_APPEND_FL, 'a'),
    (FS_COMPR_FL, 'c'),
    (FS_NOCOMPR_FL, 'm'),
    (FS_NODUMP_FL, 'd'),
    (FS_EXTENTS_FL, 'e'),
    (FS_IMMUTABLE_FL, 'i'),
    (FS_JOURNAL_DATA_FL, 'j'),
    (FS_PROJINHERIT_FL, 'P'),
    (FS_SECRM_FL, 's'),
    (FS_UNRM_FL, 'u'),
    (FS_NOTAIL_FL, 't'),
    (FS_TOPDIR_FL, 'T'),
    (FS_NOCOW_FL, 'C'),
    (FS_DAX_FL, 'x'),
    (FS_CASEFOLD_FL, 'F'),
];

fn get_flag(c: char) -> Option<u32> {
    SETTABLE_FLAGS
        .iter()
        .find(|(_, ch)| *ch == c)
        .map(|(f, _)| *f)
}

/// Render `flags` the same way `chattr -V` does: one char per known bit in
/// table order, independent of a display width (chattr never pads).
fn format_flags(flags: u32) -> String {
    SETTABLE_FLAGS
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, c)| *c)
        .collect()
}

const FS_IOC_FSGETXATTR: libc::c_ulong = 0x801c_581f;
const FS_IOC_FSSETXATTR: libc::c_ulong = 0x401c_5820;

/// Mirrors the kernel's `struct fsxattr` (linux/fs.h) used by
/// `FS_IOC_FSGETXATTR`/`FS_IOC_FSSETXATTR` for the project id.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FsXAttr {
    fsx_xflags: u32,
    fsx_extsize: u32,
    fsx_nextents: u32,
    fsx_projid: u32,
    fsx_pad: [u8; 12],
}

/// Open `path` the way e2fsprogs' `fgetflags`/`fsetflags` do: read-only,
/// non-blocking, and (for the flags ioctls specifically) refusing to
/// follow a symlink.
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

fn set_flags(path: &Path, flags: u32) -> io::Result<()> {
    let f = open_for_ioctl(path, true)?;
    let val: libc::c_int = flags as libc::c_int;
    // SAFETY: `val` is a valid 4-byte in-param for FS_IOC_SETFLAGS, same
    // reasoning as `get_flags`.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), libc::FS_IOC_SETFLAGS as _, &val) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_version(path: &Path, version: u32) -> io::Result<()> {
    let f = open_for_ioctl(path, false)?;
    let val: libc::c_int = version as libc::c_int;
    // SAFETY: same as `set_flags`, for FS_IOC_SETVERSION.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), libc::FS_IOC_SETVERSION as _, &val) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_project(path: &Path, project: u32) -> io::Result<()> {
    let f = open_for_ioctl(path, false)?;
    let mut fsx = FsXAttr::default();
    // SAFETY: `fsx` is a correctly-sized (28-byte) `fsxattr` out-param for
    // FS_IOC_FSGETXATTR; `f` stays open for the call.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), FS_IOC_FSGETXATTR as _, &mut fsx) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    fsx.fsx_projid = project;
    // SAFETY: `fsx` is a valid, correctly-sized `fsxattr` in-param for
    // FS_IOC_FSSETXATTR, round-tripped from the GETXATTR call above.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), FS_IOC_FSSETXATTR as _, &fsx) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[derive(Default)]
struct Options {
    add: bool,
    rem: bool,
    set: bool,
    set_version: bool,
    set_project: bool,
    recursive: bool,
    verbose: bool,
    silent: bool,
    version: u32,
    project: u32,
    af: u32,
    rf: u32,
    sf: u32,
}

fn usage(ui: &Ui) -> i32 {
    ui.err("Usage: chattr [-RVf] [-+=aAcCdDeijPsStTuFx] [-p project] [-v version] files...");
    1
}

/// Parse one argument (`-...`, `+...`, or `=...`); consumes a following
/// argument for `-p`/`-v`. Returns `Ok(None)` once a non-option (the start
/// of the file list) is reached, matching `decode_arg`'s `EOF` return.
fn decode_arg(opts: &mut Options, args: &[String], i: &mut usize) -> Result<bool, ()> {
    let arg = &args[*i];
    let mut chars = arg.chars();
    match chars.next() {
        Some('-') => {
            for c in chars {
                match c {
                    'R' => opts.recursive = true,
                    'V' => opts.verbose = true,
                    'f' => opts.silent = true,
                    'p' => {
                        *i += 1;
                        let v = args.get(*i).ok_or(())?;
                        opts.project = v.parse().map_err(|_| ())?;
                        opts.set_project = true;
                    }
                    'v' => {
                        *i += 1;
                        let v = args.get(*i).ok_or(())?;
                        opts.version = v.parse().map_err(|_| ())?;
                        opts.set_version = true;
                    }
                    _ => {
                        let fl = get_flag(c).ok_or(())?;
                        opts.rf |= fl;
                        opts.rem = true;
                    }
                }
            }
            Ok(true)
        }
        Some('+') => {
            opts.add = true;
            for c in chars {
                opts.af |= get_flag(c).ok_or(())?;
            }
            Ok(true)
        }
        Some('=') => {
            opts.set = true;
            for c in chars {
                opts.sf |= get_flag(c).ok_or(())?;
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn change_attributes(ui: &Ui, opts: &Options, name: &Path) -> bool {
    // Attribute changes (especially clearing -i) can otherwise be used to
    // defeat rm/chmod/chown's own protection of these trees, so chattr
    // guards them the same way those tools do.
    if let Some(reason) = usercore::protect::modification_denied(name) {
        if !opts.silent {
            ui.err(&format!(
                "while setting flags on {}: {}",
                name.display(),
                reason.message()
            ));
        }
        return false;
    }

    let meta = match fs::symlink_metadata(name) {
        Ok(m) => m,
        Err(e) => {
            if !opts.silent {
                ui.err(&format!("while trying to stat {}: {e}", name.display()));
            }
            return false;
        }
    };

    let flags = match get_flags(name) {
        Ok(f) => f,
        Err(e) => {
            if !opts.silent {
                ui.err(&format!("while reading flags on {}: {e}", name.display()));
            }
            return false;
        }
    };

    if opts.set {
        if opts.verbose {
            println!(
                "Flags of {} set as {}",
                name.display(),
                format_flags(opts.sf)
            );
        }
        if let Err(e) = set_flags(name, opts.sf) {
            ui.err(&format!("{}: {e}", name.display()));
        }
    } else {
        let mut new_flags = flags;
        if opts.rem {
            new_flags &= !opts.rf;
        }
        if opts.add {
            new_flags |= opts.af;
        }
        if opts.verbose {
            println!(
                "Flags of {} set as {}",
                name.display(),
                format_flags(new_flags)
            );
        }
        if !meta.is_dir() {
            new_flags &= !FS_DIRSYNC_FL;
        }
        if let Err(e) = set_flags(name, new_flags) {
            if !opts.silent {
                ui.err(&format!("while setting flags on {}: {e}", name.display()));
            }
            return false;
        }
    }

    if opts.set_version {
        if opts.verbose {
            println!("Version of {} set as {}", name.display(), opts.version);
        }
        if let Err(e) = set_version(name, opts.version) {
            if !opts.silent {
                ui.err(&format!("while setting version on {}: {e}", name.display()));
            }
            return false;
        }
    }

    if opts.set_project {
        if opts.verbose {
            println!("Project of {} set as {}", name.display(), opts.project);
        }
        if let Err(e) = set_project(name, opts.project) {
            if !opts.silent {
                ui.err(&format!("while setting project on {}: {e}", name.display()));
            }
            return false;
        }
    }

    if meta.is_dir() && opts.recursive {
        let entries = match fs::read_dir(name) {
            Ok(e) => e,
            Err(e) => {
                if !opts.silent {
                    ui.err(&format!("{}: {e}", name.display()));
                }
                return false;
            }
        };
        let mut ok = true;
        for entry in entries.flatten() {
            if !change_attributes(ui, opts, &entry.path()) {
                ok = false;
            }
        }
        return ok;
    }
    true
}

/// Entry point for the `chattr` utility. Parses `std::env::args()`,
/// applies the requested flag/version/project changes to each file
/// (recursing into directories with `-R`), and returns 0 on success or 1
/// if any file could not be changed.
pub fn run() -> i32 {
    let ui = Ui::new("chattr");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = Options::default();
    let mut i = 0;
    let mut end_opts = false;
    while i < args.len() && !end_opts {
        if args[i] == "--" {
            i += 1;
            end_opts = true;
            continue;
        }
        match decode_arg(&mut opts, &args, &mut i) {
            Ok(true) => i += 1,
            Ok(false) => end_opts = true,
            Err(()) => return usage(&ui),
        }
    }
    if i >= args.len() {
        return usage(&ui);
    }
    if opts.set && (opts.add || opts.rem) {
        ui.err("= is incompatible with - and +");
        return 1;
    }
    if opts.rf & opts.af != 0 {
        ui.err("Can't both set and unset same flag.");
        return 1;
    }
    if !(opts.add || opts.rem || opts.set || opts.set_version || opts.set_project) {
        ui.err("Must use '-v', =, - or +");
        return 1;
    }
    if opts.verbose {
        eprintln!("chattr (user_utils) 0.1.0");
    }

    let mut status = 0;
    for f in &args[i..] {
        if !change_attributes(&ui, &opts, Path::new(f)) {
            status = 1;
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_flag_known_and_unknown() {
        assert_eq!(get_flag('i'), Some(FS_IMMUTABLE_FL));
        assert_eq!(get_flag('a'), Some(FS_APPEND_FL));
        assert_eq!(get_flag('?'), None);
    }

    #[test]
    fn format_flags_round_trips_bits() {
        assert_eq!(format_flags(FS_IMMUTABLE_FL | FS_APPEND_FL), "ai");
        assert_eq!(format_flags(0), "");
    }

    #[test]
    fn decode_arg_plus_sets_add() {
        let mut opts = Options::default();
        let args = vec!["+i".to_string()];
        let mut i = 0;
        assert_eq!(decode_arg(&mut opts, &args, &mut i), Ok(true));
        assert!(opts.add);
        assert_eq!(opts.af, FS_IMMUTABLE_FL);
    }

    #[test]
    fn decode_arg_rejects_unknown_flag() {
        let mut opts = Options::default();
        let args = vec!["+Z".to_string()];
        let mut i = 0;
        assert_eq!(decode_arg(&mut opts, &args, &mut i), Err(()));
    }

    #[test]
    fn decode_arg_minus_p_consumes_project_id() {
        let mut opts = Options::default();
        let args = vec!["-p".to_string(), "42".to_string()];
        let mut i = 0;
        assert_eq!(decode_arg(&mut opts, &args, &mut i), Ok(true));
        assert!(opts.set_project);
        assert_eq!(opts.project, 42);
        assert_eq!(i, 1);
    }

    #[test]
    fn change_attributes_refuses_protected_syshub_path() {
        let ui = Ui::new("chattr");
        let opts = Options {
            rem: true,
            rf: FS_IMMUTABLE_FL,
            silent: true,
            ..Default::default()
        };
        // Same guard rm/chmod/chown use — `-i` here would otherwise be a
        // way to defeat their protection of this tree.
        assert!(!change_attributes(
            &ui,
            &opts,
            Path::new("/overlayer/syshub/does-not-need-to-exist")
        ));
    }

    #[test]
    fn change_attributes_round_trips_immutable_flag() {
        let dir = std::env::temp_dir().join(format!("user_chattr_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        fs::write(&file, b"x").unwrap();

        let ui = Ui::new("chattr");
        let mut opts = Options {
            add: true,
            af: FS_IMMUTABLE_FL,
            ..Default::default()
        };
        // Immutable/append-only ioctls need CAP_LINUX_IMMUTABLE; skip
        // assertions (not the failure) when unprivileged so this still
        // exercises the ioctl path in CI.
        let set_ok = change_attributes(&ui, &opts, &file);
        // SAFETY: `libc::geteuid` takes no arguments and cannot fail or
        // cause UB.
        if unsafe { libc::geteuid() } == 0 {
            assert!(set_ok);
            assert_eq!(get_flags(&file).unwrap() & FS_IMMUTABLE_FL, FS_IMMUTABLE_FL);
            opts.add = false;
            opts.rem = true;
            opts.rf = FS_IMMUTABLE_FL;
            assert!(change_attributes(&ui, &opts, &file));
            assert_eq!(get_flags(&file).unwrap() & FS_IMMUTABLE_FL, 0);
        }

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
    }
}
