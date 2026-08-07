//! user chown — change file owner and group.

use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use usercore::protect;

pub fn run() -> i32 {
    let mut recursive = false;
    let mut verbose = false;
    let mut owner_spec: Option<String> = None;
    let mut paths: Vec<PathBuf> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: chown [OPTION]... [OWNER][:[GROUP]] FILE...\n\
 Change the owner and/or group of each FILE.\n\n\
 -R, --recursive operate on files and directories recursively\n\
 -v, --verbose output a diagnostic for every file processed\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("chown (user_utils) 0.1.0");
                return 0;
            }
            "-R" | "--recursive" => recursive = true,
            "-v" | "--verbose" => verbose = true,
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'R' => recursive = true,
                        'v' => verbose = true,
                        _ => {
                            eprintln!("chown: invalid option -- '{c}'");
                            return 1;
                        }
                    }
                }
            }
            other => {
                if owner_spec.is_none() {
                    owner_spec = Some(other.to_string());
                } else {
                    paths.push(PathBuf::from(other));
                }
            }
        }
    }

    let owner_spec = match owner_spec {
        Some(s) => s,
        None => {
            eprintln!("chown: missing operand");
            return 1;
        }
    };
    if paths.is_empty() {
        eprintln!("chown: missing operand after '{owner_spec}'");
        return 1;
    }

    let (uid, gid) = match parse_owner_group(&owner_spec) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("chown: {e}");
            return 1;
        }
    };

    let mut status = 0;
    for p in &paths {
        if let Some(reason) = protect::modification_denied(p) {
            eprintln!("chown: changing ownership of '{}': {}", p.display(), reason.message());
            status = 1;
            continue;
        }
        if let Err(e) = chown_path(p, uid, gid, recursive, verbose) {
            eprintln!("chown: changing ownership of '{}': {e}", p.display());
            status = 1;
        }
    }
    status
}

fn parse_owner_group(spec: &str) -> Result<(Option<u32>, Option<u32>), String> {
    // OWNER:GROUP / OWNER: / :GROUP / OWNER / :
    if let Some((o, g)) = spec.split_once(':') {
        let uid = if o.is_empty() {
            None
        } else {
            Some(resolve_user(o)?)
        };
        let gid = if g.is_empty() {
            None
        } else {
            Some(resolve_group(g)?)
        };
        Ok((uid, gid))
    } else if let Some((o, g)) = spec.split_once('.') {
        // historic OWNER.GROUP
        Ok((Some(resolve_user(o)?), Some(resolve_group(g)?)))
    } else {
        Ok((Some(resolve_user(spec)?), None))
    }
}

fn resolve_user(name: &str) -> Result<u32, String> {
    if let Ok(n) = name.parse::<u32>() {
        return Ok(n);
    }
    let c = CString::new(name).map_err(|_| "invalid user name".to_string())?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` that outlives this
    // block, so `c.as_ptr()` is a sound `getpwnam` argument. `getpwnam`
    // returns either NULL (handled below) or a pointer to an internal
    // static `passwd` buffer valid until the next `getpwnam`/`getpwuid`/
    // `getpwent`-family call on this thread; we dereference it exactly
    // once, immediately, before any such call, so the read is sound.
    unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() {
            Err(format!("invalid user: '{name}'"))
        } else {
            Ok((*pw).pw_uid)
        }
    }
}

fn resolve_group(name: &str) -> Result<u32, String> {
    if let Ok(n) = name.parse::<u32>() {
        return Ok(n);
    }
    let c = CString::new(name).map_err(|_| "invalid group name".to_string())?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` that outlives this
    // block, so `c.as_ptr()` is a sound `getgrnam` argument. `getgrnam`
    // returns either NULL (handled below) or a pointer to an internal
    // static `group` buffer valid until the next `getgrnam`/`getgrgid`/
    // `getgrent`-family call on this thread; we dereference it exactly
    // once, immediately, before any such call, so the read is sound.
    unsafe {
        let gr = libc::getgrnam(c.as_ptr());
        if gr.is_null() {
            Err(format!("invalid group: '{name}'"))
        } else {
            Ok((*gr).gr_gid)
        }
    }
}

fn chown_path(
    path: &Path,
    uid: Option<u32>,
    gid: Option<u32>,
    recursive: bool,
    verbose: bool,
) -> std::io::Result<()> {
    do_chown(path, uid, gid, verbose)?;
    // Use symlink_metadata (lstat), not path.is_dir() (stat): the latter
    // follows symlinks, so a symlink pointing at a directory — or a
    // self-referential symlink — would be treated as a directory and
    // recursed into forever. Matching GNU chown, -R never follows symlinks
    // during recursion.
    if recursive {
        let is_real_dir = fs::symlink_metadata(path)
            .map(|m| m.file_type().is_dir())
            .unwrap_or(false);
        if is_real_dir {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                chown_path(&entry.path(), uid, gid, recursive, verbose)?;
            }
        }
    }
    Ok(())
}

fn do_chown(path: &Path, uid: Option<u32>, gid: Option<u32>, verbose: bool) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let u = uid
        .map(|u| u as libc::uid_t)
        .unwrap_or(u32::MAX as libc::uid_t);
    // (uid_t)-1 means leave unchanged
    let g = gid
        .map(|g| g as libc::gid_t)
        .unwrap_or(u32::MAX as libc::gid_t);
    // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for the
    // duration of this call, so `c.as_ptr()` is a sound `chown(2)` path
    // argument. `u`/`g` are either real ids or the POSIX `(uid_t)-1` /
    // `(gid_t)-1` sentinel (`u32::MAX`), which tells the kernel to leave
    // that field unchanged.
    let rc = unsafe { libc::chown(c.as_ptr(), u, g) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if verbose {
        let uname = uid.map(uid_name).unwrap_or_else(|| "unchanged".into());
        let gname = gid.map(gid_name).unwrap_or_else(|| "unchanged".into());
        println!(
            "changed ownership of '{}' to {uname}:{gname}",
            path.display()
        );
    }
    Ok(())
}

fn uid_name(uid: u32) -> String {
    // SAFETY: `getpwuid` takes a plain integer and returns either NULL
    // (handled below) or a pointer to an internal static `passwd` buffer
    // whose `pw_name` field is a NUL-terminated string valid until the
    // next `getpwnam`/`getpwuid`/`getpwent`-family call on this thread.
    // We build the `CStr` and copy it into an owned `String` immediately,
    // before any such call, so both the dereference and `CStr::from_ptr`
    // are sound.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            uid.to_string()
        } else {
            CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned()
        }
    }
}

fn gid_name(gid: u32) -> String {
    // SAFETY: `getgrgid` takes a plain integer and returns either NULL
    // (handled below) or a pointer to an internal static `group` buffer
    // whose `gr_name` field is a NUL-terminated string valid until the
    // next `getgrnam`/`getgrgid`/`getgrent`-family call on this thread.
    // We build the `CStr` and copy it into an owned `String` immediately,
    // before any such call, so both the dereference and `CStr::from_ptr`
    // are sound.
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            gid.to_string()
        } else {
            CStr::from_ptr((*gr).gr_name).to_string_lossy().into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("user_chown_test_{tag}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn recursive_chown_does_not_hang_on_self_referential_symlink() {
        // Regression: chown_path used path.is_dir() (follows symlinks), so a
        // symlink pointing back into its own parent directory caused
        // unbounded recursion during -R.
        let dir = tmp_dir("symlink_loop");
        let loop_link = dir.join("loop");
        std::os::unix::fs::symlink(&dir, &loop_link).unwrap();

        // uid=None, gid=None means "leave unchanged" — exercises the walk
        // without requiring root privileges to actually change ownership.
        let result = chown_path(&dir, None, None, true, false);
        assert!(result.is_ok(), "recursive chown must terminate: {result:?}");

        fs::remove_file(&loop_link).ok();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_owner_group_colon_form() {
        // Numeric ids avoid depending on real user/group names existing.
        assert_eq!(
            parse_owner_group("1000:1000").unwrap(),
            (Some(1000), Some(1000))
        );
        assert_eq!(parse_owner_group("1000:").unwrap(), (Some(1000), None));
        assert_eq!(parse_owner_group(":1000").unwrap(), (None, Some(1000)));
        assert_eq!(parse_owner_group("1000").unwrap(), (Some(1000), None));
    }

    #[test]
    fn parse_owner_group_dot_form() {
        assert_eq!(
            parse_owner_group("1000.1000").unwrap(),
            (Some(1000), Some(1000))
        );
    }

    #[test]
    fn parse_owner_group_rejects_unknown_name() {
        assert!(parse_owner_group("__no_such_user_user_test__").is_err());
    }
}
