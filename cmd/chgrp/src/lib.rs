//! user chgrp — change group ownership.
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `chgrp` utility. Parses `std::env::args()` and
/// changes the group ownership of each `FILE` to `GROUP` (a group name
/// or numeric gid), optionally recursing into directories with `-R`.
///
/// Returns 0 on success, 1 on a usage error or if any `FILE` could not
/// be changed.
pub fn run() -> i32 {
    let ui = Ui::new("chgrp");
    let mut recursive = false;
    let mut verbose = false;
    let mut group: Option<String> = None;
    let mut paths: Vec<PathBuf> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: chgrp [OPTION]... GROUP FILE...\nChange the group of each FILE to GROUP.\n -R, --recursive operate on files and directories recursively\n -v, --verbose output a diagnostic for every file processed\n");
                return 0;
            }
            "--version" => {
                println!("chgrp (user_utils) 0.1.0");
                return 0;
            }
            "-R" | "--recursive" => recursive = true,
            "-v" | "--verbose" => verbose = true,
            s if s.starts_with('-') && s != "-" => {
                for c in s.chars().skip(1) {
                    match c {
                        'R' => recursive = true,
                        'v' => verbose = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            other => {
                if group.is_none() {
                    group = Some(other.to_string());
                } else {
                    paths.push(PathBuf::from(other));
                }
            }
        }
    }
    let Some(group) = group else {
        ui.err("missing operand");
        return 1;
    };
    if paths.is_empty() {
        ui.err(&format!("missing operand after '{group}'"));
        return 1;
    }
    let gid = match resolve_group(&group) {
        Ok(g) => g,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };
    let mut status = 0;
    for p in &paths {
        if let Err(e) = chgrp_path(p, gid, recursive, verbose, &group) {
            ui.err(&format!("changing group of '{}': {e}", p.display()));
            status = 1;
        }
    }
    status
}

/// Resolve `name` to a gid: a plain number is used as-is, otherwise it's
/// looked up via `getgrnam(3)`.
fn resolve_group(name: &str) -> Result<u32, String> {
    if let Ok(n) = name.parse::<u32>() {
        return Ok(n);
    }
    let c = CString::new(name).map_err(|_| "invalid group".to_string())?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` that outlives this
    // block, so `c.as_ptr()` is a sound argument for `getgrnam`.
    // `getgrnam` either returns NULL (handled below) or a pointer to an
    // internal static `group` buffer that remains valid until the next
    // call to a `getgrnam`/`getgrgid`/`getgrent`-family function on this
    // thread; we dereference it exactly once, immediately, before any
    // such call could invalidate it, so the `(*gr).gr_gid` read is sound.
    unsafe {
        let gr = libc::getgrnam(c.as_ptr());
        if gr.is_null() {
            Err(format!("invalid group: '{name}'"))
        } else {
            Ok((*gr).gr_gid)
        }
    }
}

/// Change the group of `path` to `gid`, recursing into its children if
/// `recursive` and `path` is a *real* directory.
///
/// Uses `fs::symlink_metadata` (not `path.is_dir()`, which follows
/// symlinks) to decide whether to recurse: `path.is_dir()` would treat a
/// symlink to a directory as a subdirectory to walk into, which can
/// recurse forever on a self-referential symlink (e.g. `dir/self ->
/// dir`) and, more generally, would follow `-R` outside the intended
/// tree via any directory symlink — matching neither GNU `chgrp`'s
/// default (non-`-H`/`-L`) behavior nor safe recursive-walk practice.
fn chgrp_path(
    path: &Path,
    gid: u32,
    recursive: bool,
    verbose: bool,
    gname: &str,
) -> io::Result<()> {
    do_chown(path, gid)?;
    if verbose {
        println!("changed group of '{}' to {gname}", path.display());
    }
    if recursive {
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_dir() {
            for ent in fs::read_dir(path)? {
                let ent = ent?;
                chgrp_path(&ent.path(), gid, recursive, verbose, gname)?;
            }
        }
    }
    Ok(())
}

/// Call `chown(2)` on `path`, leaving the owner unchanged and setting
/// only the group to `gid`.
fn do_chown(path: &Path, gid: u32) -> io::Result<()> {
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for the
    // duration of this call, so `c.as_ptr()` is a sound `chown(2)` path
    // argument. Passing `u32::MAX` (i.e. `(uid_t)-1`) as the uid is the
    // documented POSIX sentinel meaning "leave the owner unchanged", so
    // only the group is modified.
    let rc = unsafe { libc::chown(c.as_ptr(), u32::MAX as libc::uid_t, gid as libc::gid_t) };
    if rc != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "user_chgrp_test_{}_{}_{}",
            std::process::id(),
            tag,
            n
        ));
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// Our own current gid, used because chown-to-own-group is one of the
    /// few group changes an unprivileged test process is guaranteed to be
    /// allowed to make.
    fn own_gid() -> u32 {
        unsafe { libc::getegid() }
    }

    #[test]
    fn resolve_group_numeric() {
        assert_eq!(resolve_group("1000").unwrap(), 1000);
    }

    #[test]
    fn resolve_group_own_gid_by_name_or_number() {
        // Resolving our own gid by its numeric form must always work,
        // regardless of what group databases are configured in the test
        // sandbox.
        let gid = own_gid();
        assert_eq!(resolve_group(&gid.to_string()).unwrap(), gid);
    }

    #[test]
    fn resolve_group_unknown_name_errors() {
        let bogus = format!("user_no_such_group_{}", std::process::id());
        assert!(resolve_group(&bogus).is_err());
    }

    #[test]
    fn chgrp_path_changes_own_file_to_own_group() {
        let dir = scratch_dir("single_file");
        let file = dir.join("f.txt");
        fs::write(&file, b"hi").unwrap();
        let gid = own_gid();
        chgrp_path(&file, gid, false, false, "self").unwrap();
        assert_eq!(fs::metadata(&file).unwrap().gid(), gid);
    }

    #[test]
    fn chgrp_path_recursive_covers_nested_files() {
        let dir = scratch_dir("recursive");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.txt"), b"a").unwrap();
        fs::write(dir.join("sub").join("b.txt"), b"b").unwrap();
        let gid = own_gid();
        chgrp_path(&dir, gid, true, false, "self").unwrap();
        assert_eq!(fs::metadata(dir.join("a.txt")).unwrap().gid(), gid);
        assert_eq!(fs::metadata(dir.join("sub").join("b.txt")).unwrap().gid(), gid);
    }

    #[test]
    fn chgrp_path_missing_file_errors() {
        let missing = std::env::temp_dir()
            .join(format!("user_chgrp_missing_{}", std::process::id()));
        assert!(chgrp_path(&missing, own_gid(), false, false, "self").is_err());
    }

    #[test]
    fn chgrp_path_recursive_does_not_follow_self_referential_symlink() {
        // Regression: recursing via `path.is_dir()` follows symlinks, so
        // a symlink pointing back at its own parent directory would be
        // walked forever. `fs::symlink_metadata`-based recursion treats
        // the symlink as a non-directory leaf, so this returns promptly.
        let dir = scratch_dir("symlink_loop");
        std::os::unix::fs::symlink(&dir, dir.join("self_link")).expect("create symlink");
        let gid = own_gid();
        // Must terminate (not hang) and must succeed: chown on the
        // symlink itself follows it to `dir`, which we already own.
        chgrp_path(&dir, gid, true, false, "self").unwrap();
    }
}
