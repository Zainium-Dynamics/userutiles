//! user stat — display file or filesystem status.
use colored::Colorize;
use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

use usercore::Ui;

/// Entry point for the `stat` utility. Parses `std::env::args()` and prints
/// file or filesystem status for each operand.
///
/// Returns 0 on success, 1 if any operand could not be `stat`-ed.
pub fn run() -> i32 {
    let ui = Ui::new("stat");
    let mut terse = false;
    let mut filesystem = false;
    let mut deref = false;
    let mut format: Option<(String, bool)> = None;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        match arg.as_str() {
            "-c" | "--format" | "--printf" => {
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option requires an argument -- '{arg}'"));
                    return 1;
                };
                i += 1;
                format = Some((v.clone(), arg == "--printf"));
            }
            s if s.starts_with("--format=") => format = Some((s["--format=".len()..].to_string(), false)),
            s if s.starts_with("--printf=") => format = Some((s["--printf=".len()..].to_string(), true)),
            s if s.starts_with("-c") && s.len() > 2 => format = Some((s[2..].to_string(), false)),
            "-L" | "--dereference" => deref = true,
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
            s if s.starts_with('-') && s.len() > 1 && s[1..].chars().all(|c| "tfL".contains(c)) => {
                for c in s[1..].chars() {
                    match c {
                        't' => terse = true,
                        'f' => filesystem = true,
                        _ => deref = true,
                    }
                }
            }
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
        let result = match (&format, filesystem) {
            (Some((fmt, printf)), true) => format_fs(f, fmt, *printf),
            (Some((fmt, printf)), false) => format_file(f, fmt, *printf, deref),
            // GNU's terse layouts (no SELinux context field: Zainium has no SELinux)
            (None, true) if terse => format_fs(f, "%n %i %l %t %s %S %b %f %a %c %d", false),
            (None, false) if terse => format_file(f, "%n %s %b %f %u %g %D %i %h %t %T %X %Y %Z %W %o", false, deref),
            (None, true) => print_fs(f),
            (None, false) => print_file(f, deref),
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
  -c, --format=FMT      use FMT (%a %A %s %n %N %U %G %u %g %F %i %h %y %Y ...)\n\
      --printf=FMT      like --format, with backslash escapes and no newline\n\
  -L, --dereference     follow links\n\
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
/// `path`, in the default multi-line form.
fn print_file(path: &str, deref: bool) -> io::Result<()> {
    let meta = if deref { fs::metadata(path)? } else { fs::symlink_metadata(path)? };
    let ft = meta.file_type();
    let file_type = file_type_label(ft);
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

fn mode_string(m: u32) -> String {
    let t = match m & libc::S_IFMT {
        libc::S_IFDIR => 'd',
        libc::S_IFLNK => 'l',
        libc::S_IFCHR => 'c',
        libc::S_IFBLK => 'b',
        libc::S_IFIFO => 'p',
        libc::S_IFSOCK => 's',
        _ => '-',
    };
    let mut s = String::from(t);
    for (i, c) in "rwxrwxrwx".chars().enumerate() {
        s.push(if m & (0o400 >> i) != 0 { c } else { '-' });
    }
    let fix = |s: &mut String, idx: usize, bit: u32, set: char, unset: char| {
        if m & bit != 0 {
            let has_x = s.as_bytes()[idx] == b'x';
            s.replace_range(idx..idx + 1, &(if has_x { set } else { unset }).to_string());
        }
    };
    fix(&mut s, 3, 0o4000, 's', 'S');
    fix(&mut s, 6, 0o2000, 's', 'S');
    fix(&mut s, 9, 0o1000, 't', 'T');
    s
}

/// GNU-style "2026-10-11 00:46:12.000000000 +0500" in local time.
fn human_time(secs: i64, nsec: i64) -> String {
    // SAFETY: localtime_r only writes the tm we pass.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    #[allow(deprecated)]
    let t: libc::time_t = secs as _;
    unsafe { libc::localtime_r(&t, &mut tm) };
    let off = tm.tm_gmtoff;
    let sign = if off < 0 { '-' } else { '+' };
    let off = off.abs();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:09} {}{:02}{:02}",
        tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec, nsec, sign, off / 3600, (off % 3600) / 60
    )
}

fn escapes(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('0') => out.push('\0'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Expand `%x` directives with `f`; `%%` is a literal percent, unknown ones print as-is.
fn expand(fmt: &str, f: impl Fn(char) -> Option<String>) -> String {
    let mut out = String::new();
    let mut it = fmt.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('%') => out.push('%'),
            Some(d) => match f(d) {
                Some(v) => out.push_str(&v),
                None => {
                    out.push('%');
                    out.push(d);
                }
            },
            None => out.push('%'),
        }
    }
    out
}

fn format_file(path: &str, fmt: &str, printf: bool, deref: bool) -> io::Result<()> {
    let meta = if deref { fs::metadata(path)? } else { fs::symlink_metadata(path)? };
    let fmt = if printf { escapes(fmt) } else { fmt.to_string() };
    let m = meta.mode();
    let rdev = meta.rdev();
    let birth = meta
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_secs() as i64, i64::from(d.subsec_nanos())));
    let s = expand(&fmt, |d| {
        Some(match d {
            'a' => format!("{:o}", m & 0o7777),
            'A' => mode_string(m),
            'b' => meta.blocks().to_string(),
            'B' => "512".into(),
            'd' => meta.dev().to_string(),
            'D' => format!("{:x}", meta.dev()),
            'f' => format!("{:x}", m),
            'F' => file_type_label(meta.file_type()).into(),
            'g' => meta.gid().to_string(),
            'G' => usercore::nss::group_by_gid(meta.gid()).map(|g| g.name).unwrap_or_else(|| "UNKNOWN".into()),
            'h' => meta.nlink().to_string(),
            'i' => meta.ino().to_string(),
            'n' => path.to_string(),
            'N' => match fs::read_link(path).ok().filter(|_| meta.file_type().is_symlink()) {
                Some(t) => format!("'{path}' -> '{}'", t.display()),
                None => format!("'{path}'"),
            },
            'o' => meta.blksize().to_string(),
            's' => meta.len().to_string(),
            't' => format!("{:x}", libc::major(rdev)),
            'T' => format!("{:x}", libc::minor(rdev)),
            'u' => meta.uid().to_string(),
            'U' => usercore::nss::user_by_uid(meta.uid()).map(|u| u.name).unwrap_or_else(|| "UNKNOWN".into()),
            'w' => birth.map_or("-".into(), |(s, ns)| human_time(s, ns)),
            'W' => birth.map_or("0".into(), |(s, _)| s.to_string()),
            'x' => human_time(meta.atime(), meta.atime_nsec()),
            'X' => meta.atime().to_string(),
            'y' => human_time(meta.mtime(), meta.mtime_nsec()),
            'Y' => meta.mtime().to_string(),
            'z' => human_time(meta.ctime(), meta.ctime_nsec()),
            'Z' => meta.ctime().to_string(),
            _ => return None,
        })
    });
    if printf {
        print!("{s}");
    } else {
        println!("{s}");
    }
    Ok(())
}

fn statvfs(path: &str) -> io::Result<libc::statvfs> {
    let c = std::ffi::CString::new(path).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    // SAFETY: c is a valid C string and st is a properly sized out-buffer, read only on success.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(st)
}

/// The filesystem ID as GNU stat prints it: the fsid_t words, first word highest.
/// (statvfs folds them the other way round, and musl keeps only the first.)
fn fs_id(sf: &libc::statfs) -> u64 {
    // SAFETY: fsid_t is two C ints; reading them as two u32 is a plain reinterpretation.
    let w: [u32; 2] = unsafe { std::mem::transmute(sf.f_fsid) };
    (u64::from(w[0]) << 32) | u64::from(w[1])
}

fn format_fs(path: &str, fmt: &str, printf: bool) -> io::Result<()> {
    let st = statvfs(path)?;
    let c = std::ffi::CString::new(path).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    // SAFETY: as above, for statfs which also reports the filesystem type.
    let mut sf: libc::statfs = unsafe { std::mem::zeroed() };
    let fstype = if unsafe { libc::statfs(c.as_ptr(), &mut sf) } == 0 { sf.f_type as u64 } else { 0 };
    let fsid = fs_id(&sf);
    let fmt = if printf { escapes(fmt) } else { fmt.to_string() };
    let s = expand(&fmt, |d| {
        Some(match d {
            'n' => path.to_string(),
            'i' => format!("{fsid:x}"),
            'l' => st.f_namemax.to_string(),
            't' => format!("{fstype:x}"),
            'T' => fs_name(fstype).into(),
            's' | 'S' => st.f_frsize.to_string(),
            'b' => st.f_blocks.to_string(),
            'f' => st.f_bfree.to_string(),
            'a' => st.f_bavail.to_string(),
            'c' => st.f_files.to_string(),
            'd' => st.f_ffree.to_string(),
            _ => return None,
        })
    });
    if printf {
        print!("{s}");
    } else {
        println!("{s}");
    }
    Ok(())
}

fn fs_name(magic: u64) -> &'static str {
    match magic {
        0xef53 => "ext2/ext3",
        0x9123683e => "btrfs",
        0x58465342 => "xfs",
        0x4d44 => "msdos",
        0x794c7630 => "overlayfs",
        0x01021994 => "tmpfs",
        0xe0f5e1e2 => "erofs",
        0x9fa0 => "proc",
        0x62656572 => "sysfs",
        0x1373 => "devfs",
        0x6969 => "nfs",
        0x73717368 => "squashfs",
        0x2fc12fc1 => "zfs",
        0xf2f52010 => "f2fs",
        _ => "UNKNOWN",
    }
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
    // SAFETY: c is a valid C string and sf a zeroed out-buffer; on failure the ID stays 0.
    let mut sf: libc::statfs = unsafe { std::mem::zeroed() };
    unsafe { libc::statfs(c.as_ptr(), &mut sf) };
    let bsize = st.f_frsize;
    println!(
        " {} {}",
        "File:".bright_green(),
        format!("\"{path}\"").bright_magenta()
    );
    println!(
        " {} {} {} {}",
        "ID:".bright_green(),
        format!("{:x}", fs_id(&sf)).bright_magenta(),
        "Namelen:".bright_green(),
        (st.f_namemax).to_string().bright_magenta()
    );
    println!(
        " {} {} {} {}",
        "Block size:".bright_green(),
        bsize.to_string().bright_magenta(),
        "Blocks:".bright_green(),
        (st.f_blocks).to_string().bright_magenta()
    );
    println!(
        " {} {} {} {}",
        "Free:".bright_green(),
        (st.f_bfree).to_string().bright_magenta(),
        "Available:".bright_green(),
        (st.f_bavail).to_string().bright_magenta()
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
        assert!(print_file(missing.to_str().unwrap(), false, false).is_err());
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
