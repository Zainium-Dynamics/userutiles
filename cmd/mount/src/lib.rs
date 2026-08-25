//! user mount — mount a filesystem.
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::Path;

use usercore::Ui;

/// Filesystem types tried in order when none was given and `/etc/fstab`
/// doesn't say (or says `auto`) — a practical stand-in for the real
/// `mount`'s libblkid-based detection, which this workspace doesn't have.
const AUTO_FSTYPES: &[&str] = &[
    "ext4", "ext3", "ext2", "xfs", "btrfs", "vfat", "exfat", "ntfs3", "iso9660", "f2fs",
];

/// One `-o`-style comma option, mapped to a mount(2) flag bit — matches
/// `mount(8)`'s well-known option names.
const FLAG_OPTS: &[(&str, libc::c_ulong)] = &[
    ("ro", libc::MS_RDONLY),
    ("nosuid", libc::MS_NOSUID),
    ("nodev", libc::MS_NODEV),
    ("noexec", libc::MS_NOEXEC),
    ("sync", libc::MS_SYNCHRONOUS),
    ("remount", libc::MS_REMOUNT),
    ("mand", libc::MS_MANDLOCK),
    ("dirsync", libc::MS_DIRSYNC),
    ("noatime", libc::MS_NOATIME),
    ("nodiratime", libc::MS_NODIRATIME),
    ("bind", libc::MS_BIND),
    ("rbind", libc::MS_BIND | libc::MS_REC),
    ("move", libc::MS_MOVE),
    ("silent", libc::MS_SILENT),
    ("relatime", libc::MS_RELATIME),
    ("strictatime", libc::MS_STRICTATIME),
    ("lazytime", libc::MS_LAZYTIME),
];

/// Options that clear a bit rather than set one (the "un-" form of a
/// [`FLAG_OPTS`] entry) — `rw` is the default, but explicit for clarity
/// and to override an fstab default.
const CLEAR_FLAG_OPTS: &[(&str, libc::c_ulong)] = &[
    ("rw", libc::MS_RDONLY),
    ("suid", libc::MS_NOSUID),
    ("dev", libc::MS_NODEV),
    ("exec", libc::MS_NOEXEC),
    ("async", libc::MS_SYNCHRONOUS),
    ("atime", libc::MS_NOATIME),
    ("diratime", libc::MS_NODIRATIME),
];

/// Split a comma-separated `-o` option string into a mount(2) flags word
/// and the leftover filesystem-specific tokens (passed through verbatim
/// as the `data` argument, comma-joined — e.g. ext4's `data=ordered`).
fn parse_opts(opts: &str) -> (libc::c_ulong, String) {
    let mut flags: libc::c_ulong = 0;
    let mut data: Vec<&str> = Vec::new();
    for tok in opts.split(',').filter(|t| !t.is_empty()) {
        if let Some((_, bit)) = FLAG_OPTS.iter().find(|(name, _)| *name == tok) {
            flags |= bit;
        } else if let Some((_, bit)) = CLEAR_FLAG_OPTS.iter().find(|(name, _)| *name == tok) {
            flags &= !bit;
        } else {
            data.push(tok);
        }
    }
    (flags, data.join(","))
}

fn to_cstring(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"))
}

/// Call mount(2) once for a specific filesystem type.
fn mount_once(
    source: &str,
    target: &str,
    fstype: &str,
    flags: libc::c_ulong,
    data: &str,
) -> io::Result<()> {
    let c_source = to_cstring(source)?;
    let c_target = to_cstring(target)?;
    let c_fstype = to_cstring(fstype)?;
    let c_data = to_cstring(data)?;
    let data_ptr = if data.is_empty() {
        std::ptr::null()
    } else {
        c_data.as_ptr() as *const libc::c_void
    };
    // SAFETY: all three C strings are valid and NUL-terminated and kept
    // alive for the call; `data_ptr` is either NULL or points at a live,
    // NUL-terminated buffer of the same lifetime. `mount(2)`'s contract is
    // a plain syscall wrapper with no further pointer retention.
    let r = unsafe {
        libc::mount(
            c_source.as_ptr(),
            c_target.as_ptr(),
            c_fstype.as_ptr(),
            flags,
            data_ptr,
        )
    };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Mount `source` at `target`. `fstype` of `""`/`"auto"`/`None` (and the
/// bind/move/remount flags, which the kernel ignores the type for) tries
/// [`AUTO_FSTYPES`] in turn; any other value is used as-is.
fn do_mount(source: &str, target: &str, fstype: Option<&str>, opts: &str) -> io::Result<()> {
    let (flags, data) = parse_opts(opts);
    let needs_no_type = flags & (libc::MS_BIND | libc::MS_MOVE | libc::MS_REMOUNT) != 0;
    match fstype {
        Some(t) if !t.is_empty() && t != "auto" => mount_once(source, target, t, flags, &data),
        _ if needs_no_type => mount_once(source, target, "", flags, &data),
        _ => {
            let mut last_err =
                io::Error::new(io::ErrorKind::InvalidInput, "no filesystem type worked");
            for candidate in AUTO_FSTYPES {
                match mount_once(source, target, candidate, flags, &data) {
                    Ok(()) => return Ok(()),
                    Err(e) => last_err = e,
                }
            }
            Err(last_err)
        }
    }
}

struct FstabEntry {
    device: String,
    mountpoint: String,
    fstype: String,
    options: String,
}

/// Parse `/etc/fstab`-format lines: `device mountpoint fstype options dump pass`,
/// `#`-comments and blank lines ignored.
fn parse_fstab(text: &str) -> Vec<FstabEntry> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some(FstabEntry {
                device: f.next()?.to_string(),
                mountpoint: f.next()?.to_string(),
                fstype: f.next()?.to_string(),
                options: f.next().unwrap_or("defaults").to_string(),
            })
        })
        .collect()
}

fn find_fstab_entry<'a>(entries: &'a [FstabEntry], target: &str) -> Option<&'a FstabEntry> {
    entries
        .iter()
        .find(|e| e.mountpoint == target || e.device == target)
}

/// Print currently mounted filesystems the way `mount` with no arguments
/// does, from `/proc/self/mounts`.
fn print_mounts() -> io::Result<()> {
    let text = fs::read_to_string("/proc/self/mounts")?;
    let mut out = String::new();
    for line in text.lines() {
        let mut f = line.split_whitespace();
        let (Some(dev), Some(target), Some(fstype), Some(opts)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        out.push_str(&format!("{dev} on {target} type {fstype} ({opts})\n"));
    }
    usercore::ui::write_stdout(out.as_bytes())?;
    usercore::ui::flush_stdout()
}

fn print_help() {
    print!(
        "Usage: mount [-nrvw] [-t fstype] [-o options] device dir\n\
 mount -a [-t fstype]\n\
 mount [-nrvw] {{device|dir}}\n\
 mount --bind|--rbind|--move olddir newdir\n\
 mount\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `mount` utility. Parses `std::env::args()` and
/// either lists current mounts (no operands), mounts a single fstab entry
/// by device/mountpoint, mounts `SOURCE` at `TARGET` directly, or (`-a`)
/// mounts every non-`noauto` `/etc/fstab` entry.
///
/// Returns 0 on success, 1 if any requested mount failed.
pub fn run() -> i32 {
    let ui = Ui::new("mount");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut fstype: Option<String> = None;
    let mut opts = String::new();
    let mut all = false;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("mount (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => all = true,
            "-r" | "--read-only" => {
                if !opts.is_empty() {
                    opts.push(',');
                }
                opts.push_str("ro");
            }
            "-w" | "--rw" => {
                if !opts.is_empty() {
                    opts.push(',');
                }
                opts.push_str("rw");
            }
            "-n" | "-v" => {} // no mtab to skip writing; verbose is a no-op here
            "--bind" => opts_add(&mut opts, "bind"),
            "--rbind" => opts_add(&mut opts, "rbind"),
            "--move" => opts_add(&mut opts, "move"),
            "-t" | "--types" => {
                i += 1;
                match args.get(i) {
                    Some(v) => fstype = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 't'");
                        return 1;
                    }
                }
            }
            "-o" | "--options" => {
                i += 1;
                match args.get(i) {
                    Some(v) => opts_add(&mut opts, v),
                    None => {
                        ui.err("option requires an argument -- 'o'");
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if all {
        let fstab = match fs::read_to_string("/etc/fstab") {
            Ok(t) => parse_fstab(&t),
            Err(e) => {
                ui.err(&format!("/etc/fstab: {e}"));
                return 1;
            }
        };
        let mut status = 0;
        for e in &fstab {
            if e.options.split(',').any(|o| o == "noauto") {
                continue;
            }
            if let Err(err) = do_mount(&e.device, &e.mountpoint, Some(&e.fstype), &e.options) {
                ui.err(&format!("{}: {err}", e.mountpoint));
                status = 1;
            }
        }
        return status;
    }

    match positional.len() {
        0 => match print_mounts() {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&format!("{e}"));
                1
            }
        },
        1 => {
            let target = &positional[0];
            let fstab_text = fs::read_to_string("/etc/fstab").unwrap_or_default();
            let fstab = parse_fstab(&fstab_text);
            match find_fstab_entry(&fstab, target) {
                Some(e) => {
                    let merged = if opts.is_empty() {
                        e.options.clone()
                    } else {
                        format!("{},{}", e.options, opts)
                    };
                    let ft = fstype.as_deref().or(Some(e.fstype.as_str()));
                    match do_mount(&e.device, &e.mountpoint, ft, &merged) {
                        Ok(()) => 0,
                        Err(err) => {
                            ui.err(&format!("{}: {err}", e.mountpoint));
                            1
                        }
                    }
                }
                None => {
                    ui.err(&format!("can't find {target} in /etc/fstab"));
                    1
                }
            }
        }
        _ => {
            let source = &positional[0];
            let target = &positional[1];
            if let Some(reason) = usercore::protect::modification_denied(Path::new(target)) {
                ui.err(&format!("{target}: {}", reason.message()));
                return 1;
            }
            match do_mount(source, target, fstype.as_deref(), &opts) {
                Ok(()) => 0,
                Err(e) => {
                    ui.err(&format!("{source}: {e}"));
                    1
                }
            }
        }
    }
}

fn opts_add(opts: &mut String, extra: &str) {
    if !opts.is_empty() {
        opts.push(',');
    }
    opts.push_str(extra);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_opts_maps_known_flags() {
        let (flags, data) = parse_opts("ro,noexec,nosuid");
        assert_eq!(flags, libc::MS_RDONLY | libc::MS_NOEXEC | libc::MS_NOSUID);
        assert!(data.is_empty());
    }

    #[test]
    fn parse_opts_collects_unknown_tokens_as_data() {
        let (flags, data) = parse_opts("ro,data=ordered,errors=remount-ro");
        assert_eq!(flags, libc::MS_RDONLY);
        assert_eq!(data, "data=ordered,errors=remount-ro");
    }

    #[test]
    fn parse_opts_rw_clears_rdonly() {
        let (flags, _) = parse_opts("ro,rw");
        assert_eq!(flags & libc::MS_RDONLY, 0);
    }

    #[test]
    fn parse_opts_bind_and_rbind() {
        assert_eq!(parse_opts("bind").0, libc::MS_BIND);
        assert_eq!(parse_opts("rbind").0, libc::MS_BIND | libc::MS_REC);
    }

    #[test]
    fn parse_fstab_skips_comments_and_blanks() {
        let text = "\
# comment
/dev/sda1 / ext4 defaults 0 1

/dev/sda2 /home ext4 noauto,rw 0 2
";
        let entries = parse_fstab(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].mountpoint, "/");
        assert_eq!(entries[1].options, "noauto,rw");
    }

    #[test]
    fn find_fstab_entry_matches_device_or_mountpoint() {
        let entries = parse_fstab("/dev/sda1 / ext4 defaults 0 1\n");
        assert!(find_fstab_entry(&entries, "/").is_some());
        assert!(find_fstab_entry(&entries, "/dev/sda1").is_some());
        assert!(find_fstab_entry(&entries, "/nope").is_none());
    }

    #[test]
    fn do_mount_unprivileged_fails_cleanly() {
        // Regression guard for the syscall plumbing itself: an
        // unprivileged process must get a real OS error (EPERM), not a
        // panic or a silently-wrong success.
        let dir = std::env::temp_dir().join(format!("user_mount_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let result = do_mount("tmpfs", dir.to_str().unwrap(), Some("tmpfs"), "");
        // SAFETY: `libc::geteuid` takes no arguments and cannot fail or
        // cause UB.
        if unsafe { libc::geteuid() } != 0 {
            assert!(result.is_err());
        }
        let _ = std::fs::remove_dir(&dir);
    }
}
