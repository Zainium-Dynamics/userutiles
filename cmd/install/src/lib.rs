//! user install — copy files and set attributes (core subset).
//! Default destination layout is Zainium overlayer-aware (no /usr hardcoding).
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use std::io;
use usercore::Ui;

/// Entry point for the `install` utility. Parses `std::env::args()` and
/// either creates directories (`-d`) or copies SOURCE(s) to DEST, applying
/// mode/owner/group and optionally comparing content first (`-C`) or
/// preserving timestamps (`-p`).
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("install");
    let mut mode: Option<u32> = Some(0o755);
    let mut owner: Option<u32> = None;
    let mut group: Option<u32> = None;
    let mut directory = false;
    let mut compare = false;
    let mut verbose = false;
    let mut preserve_ts = false;
    let mut paths: Vec<PathBuf> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: install [OPTION]... [-T] SOURCE DEST\n\
 install [OPTION]... SOURCE... DIRECTORY\n\
 install [OPTION]... -d DIRECTORY...\n\
 Copy files and set attributes.\n\n\
 -d, --directory treat all arguments as directory names\n\
 -m, --mode=MODE set permission mode (default 755)\n\
 -o, --owner=OWNER set ownership\n\
 -g, --group=GROUP set group ownership\n\
 -C, --compare compare and skip if identical\n\
 -p, --preserve-timestamps preserve timestamps\n\
 -v, --verbose print name of each file\n\
 Installs into Zainium paths like /overlayer/syshub/bin (never assumes /usr).\n"
                );
                return 0;
            }
            "--version" => {
                println!("install (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--directory" => directory = true,
            "-v" | "--verbose" => verbose = true,
            "-C" | "--compare" => compare = true,
            "-p" | "--preserve-timestamps" => preserve_ts = true,
            "-m" | "--mode" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("755");
                let Some(m) = parse_mode(spec) else {
                    ui.err(&format!("invalid mode '{spec}'"));
                    return 1;
                };
                mode = Some(m);
            }
            s if s.starts_with("-m") && s.len() > 2 => {
                let Some(m) = parse_mode(&s[2..]) else {
                    ui.err(&format!("invalid mode '{}'", &s[2..]));
                    return 1;
                };
                mode = Some(m);
            }
            "-o" | "--owner" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("");
                let Some(u) = resolve_user(spec) else {
                    ui.err(&format!("invalid user '{spec}'"));
                    return 1;
                };
                owner = Some(u);
            }
            "-g" | "--group" => {
                i += 1;
                let spec = args.get(i).map(|s| s.as_str()).unwrap_or("");
                let Some(g) = resolve_group(spec) else {
                    ui.err(&format!("invalid group '{spec}'"));
                    return 1;
                };
                group = Some(g);
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
        ui.err("missing file operand");
        return 1;
    }

    if directory {
        let mut status = 0;
        for p in &paths {
            if let Err(e) = fs::create_dir_all(p) {
                ui.err(&format!("cannot create directory '{}': {e}", p.display()));
                status = 1;
                continue;
            }
            if let Some(m) = mode {
                let _ = fs::set_permissions(p, fs::Permissions::from_mode(m));
            }
            apply_owner(p, owner, group);
            if verbose {
                println!("install: creating directory '{}'", p.display());
            }
        }
        return status;
    }

    if paths.len() < 2 {
        ui.err("missing destination");
        return 1;
    }
    // `paths.len() >= 2` was just checked above, so `pop()` cannot be None.
    let dest = paths.pop().unwrap();
    let sources = paths;
    let dest_is_dir = dest.is_dir() || sources.len() > 1;

    let mut status = 0;
    for src in &sources {
        let target = if dest_is_dir {
            dest.join(src.file_name().unwrap_or_default())
        } else {
            dest.clone()
        };
        if compare && target.exists() {
            if let (Ok(a), Ok(b)) = (fs::read(src), fs::read(&target)) {
                if a == b {
                    if verbose {
                        println!("install: '{}' unchanged", target.display());
                    }
                    continue;
                }
            }
        }
        if let Err(e) = copy_file(src, &target, mode.unwrap_or(0o755), preserve_ts) {
            ui.err(&format!(
                "cannot install '{}' to '{}': {e}",
                src.display(),
                target.display()
            ));
            status = 1;
            continue;
        }
        apply_owner(&target, owner, group);
        if verbose {
            println!("'{}' -> '{}'", src.display(), target.display());
        }
    }
    status
}

/// Parse an octal permission-mode string (e.g. `"755"`), returning `None`
/// if `s` is not valid base-8. Symbolic mode strings (`u+x`, ...) are not
/// supported.
fn parse_mode(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 8).ok()
}

/// Resolve `name` to a uid: a bare number is used as-is, otherwise it is
/// looked up via `getpwnam(3)`. Returns `None` for an empty string or an
/// unknown user.
fn resolve_user(name: &str) -> Option<u32> {
    if name.is_empty() {
        return None;
    }
    if let Ok(n) = name.parse() {
        return Some(n);
    }
    let c = std::ffi::CString::new(name).ok()?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` that outlives this
    // block, so `c.as_ptr()` is a sound `getpwnam` argument. `getpwnam`
    // returns either NULL (handled below) or a pointer to an internal
    // static `passwd` buffer valid until the next `getpwnam`/`getpwuid`/
    // `getpwent`-family call on this thread; we read `pw_uid` (a plain
    // integer field) immediately, before any such call, so the
    // dereference is sound.
    unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() {
            None
        } else {
            Some((*pw).pw_uid)
        }
    }
}

/// Resolve `name` to a gid: a bare number is used as-is, otherwise it is
/// looked up via `getgrnam(3)`. Returns `None` for an empty string or an
/// unknown group.
fn resolve_group(name: &str) -> Option<u32> {
    if name.is_empty() {
        return None;
    }
    if let Ok(n) = name.parse() {
        return Some(n);
    }
    let c = std::ffi::CString::new(name).ok()?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` that outlives this
    // block, so `c.as_ptr()` is a sound `getgrnam` argument. `getgrnam`
    // returns either NULL (handled below) or a pointer to an internal
    // static `group` buffer valid until the next `getgrnam`/`getgrgid`/
    // `getgrent`-family call on this thread; we read `gr_gid` (a plain
    // integer field) immediately, before any such call, so the
    // dereference is sound.
    unsafe {
        let gr = libc::getgrnam(c.as_ptr());
        if gr.is_null() {
            None
        } else {
            Some((*gr).gr_gid)
        }
    }
}

/// Apply `chown(2)` to `path` with `uid`/`gid`, leaving either field
/// unchanged (via the POSIX `(uid_t)-1`/`(gid_t)-1` sentinel) if `None`.
/// A no-op if both are `None`. Failures (e.g. missing privilege) are
/// intentionally ignored, matching `install`'s best-effort ownership
/// semantics when not running as root.
fn apply_owner(path: &Path, uid: Option<u32>, gid: Option<u32>) {
    use std::os::unix::ffi::OsStrExt;
    if uid.is_none() && gid.is_none() {
        return;
    }
    if let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) {
        let u = uid.unwrap_or(u32::MAX);
        let g = gid.unwrap_or(u32::MAX);
        // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for
        // the duration of this call, so `c.as_ptr()` is a sound
        // `chown(2)` path argument. `u`/`g` are either real ids or the
        // POSIX `(uid_t)-1`/`(gid_t)-1` sentinel (`u32::MAX`), which tells
        // the kernel to leave that field unchanged; the (ignored) return
        // value only reports an errno failure (e.g. missing privileges),
        // causing no memory unsafety.
        unsafe {
            let _ = libc::chown(c.as_ptr(), u as libc::uid_t, g as libc::gid_t);
        }
    }
}

/// Copy `src` to `dst` (creating `dst`'s parent directories as needed),
/// set `dst`'s permission bits to `mode`, and, if `preserve_ts`, copy
/// `src`'s atime/mtime onto `dst` via `utimensat(2)`.
fn copy_file(src: &Path, dst: &Path, mode: u32, preserve_ts: bool) -> io::Result<()> {
    if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut from = File::open(src)?;
    let mut to = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(dst)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = from.read(&mut buf)?;
        if n == 0 {
            break;
        }
        to.write_all(&buf[..n])?;
    }
    to.flush()?;
    fs::set_permissions(dst, fs::Permissions::from_mode(mode))?;
    if preserve_ts {
        let meta = fs::metadata(src)?;
        let times = [
            libc::timespec {
                tv_sec: meta.atime(),
                tv_nsec: meta.atime_nsec(),
            },
            libc::timespec {
                tv_sec: meta.mtime(),
                tv_nsec: meta.mtime_nsec(),
            },
        ];
        use std::os::unix::ffi::OsStrExt;
        if let Ok(c) = std::ffi::CString::new(dst.as_os_str().as_bytes()) {
            // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive
            // for the duration of this call, so `c.as_ptr()` is a sound
            // path argument. `times` is a local `[libc::timespec; 2]`
            // array — the exact element count `utimensat(2)` requires for
            // an explicit times argument — so `times.as_ptr()` points to
            // enough initialized, correctly laid-out memory. `AT_FDCWD`
            // needs no open fd, and the (ignored) return value only
            // reports an errno failure, causing no memory unsafety.
            unsafe {
                libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "user_install_test_{tag}_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_mode_accepts_valid_octal() {
        assert_eq!(parse_mode("755"), Some(0o755));
        assert_eq!(parse_mode("0"), Some(0));
        assert_eq!(parse_mode("4755"), Some(0o4755));
    }

    #[test]
    fn parse_mode_rejects_invalid_input() {
        assert_eq!(parse_mode("rwxr-xr-x"), None);
        assert_eq!(parse_mode("899"), None); // 9 is not an octal digit
        assert_eq!(parse_mode(""), None);
    }

    #[test]
    fn resolve_user_accepts_numeric_uid() {
        assert_eq!(resolve_user("1000"), Some(1000));
    }

    #[test]
    fn resolve_user_empty_is_none() {
        assert_eq!(resolve_user(""), None);
    }

    #[test]
    fn resolve_user_unknown_name_is_none() {
        assert_eq!(resolve_user("no_such_user_user_install_test"), None);
    }

    #[test]
    fn resolve_group_accepts_numeric_gid() {
        assert_eq!(resolve_group("1000"), Some(1000));
    }

    #[test]
    fn resolve_group_unknown_name_is_none() {
        assert_eq!(resolve_group("no_such_group_user_install_test"), None);
    }

    #[test]
    fn copy_file_copies_content_and_sets_mode() {
        let dir = scratch_dir("copy");
        let src = dir.join("src.txt");
        let dst = dir.join("dst.txt");
        fs::write(&src, b"hello install").unwrap();
        copy_file(&src, &dst, 0o600, false).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"hello install");
        let mode = fs::metadata(&dst).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_file_creates_missing_parent_dirs() {
        let dir = scratch_dir("parent");
        let src = dir.join("src.txt");
        fs::write(&src, b"x").unwrap();
        let dst = dir.join("nested/deep/dst.txt");
        copy_file(&src, &dst, 0o644, false).unwrap();
        assert!(dst.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_file_missing_source_errors() {
        let dir = scratch_dir("missing");
        let src = dir.join("nope.txt");
        let dst = dir.join("dst.txt");
        assert!(copy_file(&src, &dst, 0o644, false).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
