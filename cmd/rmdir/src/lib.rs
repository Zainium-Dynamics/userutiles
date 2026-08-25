//! user rmdir — remove empty directories.
use std::fs;
use std::path::PathBuf;

use usercore::{protect, Ui};

/// Entry point for the `rmdir` utility. Parses `std::env::args()` and
/// removes each `DIRECTORY`, which must be empty, optionally walking up and
/// removing now-empty ancestor directories too (`-p`).
///
/// Returns 0 on success, 1 if any directory could not be removed (unless
/// `--ignore-fail-on-non-empty` was given and the only failure reason was
/// non-emptiness), or on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("rmdir");
    let mut parents = false;
    let mut verbose = false;
    let mut ignore_fail = false;
    let mut dirs: Vec<PathBuf> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: rmdir [OPTION]... DIRECTORY...\n\
 Remove the DIRECTORY(ies), if they are empty.\n\n\
 --ignore-fail-on-non-empty\n\
 ignore each failure that is solely because a directory\n\
 is non-empty\n\
 -p, --parents remove DIRECTORY and its ancestors\n\
 -v, --verbose output a diagnostic for every directory processed\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("rmdir (user_utils) 0.1.0");
                return 0;
            }
            "-p" | "--parents" => parents = true,
            "-v" | "--verbose" => verbose = true,
            "--ignore-fail-on-non-empty" => ignore_fail = true,
            s if s.starts_with('-') && s != "-" => {
                for c in s.chars().skip(1) {
                    match c {
                        'p' => parents = true,
                        'v' => verbose = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            other => dirs.push(PathBuf::from(other)),
        }
    }
    if dirs.is_empty() {
        ui.err("missing operand");
        return 1;
    }

    let mut status = 0;
    for d in &dirs {
        if parents {
            remove_with_ancestors(d, ignore_fail, verbose, &ui, &mut status);
        } else {
            remove_one(d, ignore_fail, verbose, &ui, &mut status);
        }
    }
    status
}

/// Remove a single directory `d`, updating `*status` to 1 and reporting via
/// `ui` on failure — unless the only reason it failed was non-emptiness and
/// `ignore_fail` is set, in which case it's silently skipped.
fn remove_one(d: &std::path::Path, ignore_fail: bool, verbose: bool, ui: &Ui, status: &mut i32) {
    if let Some(reason) = protect::removal_denied(d) {
        ui.err(&format!(
            "failed to remove '{}': {}",
            d.display(),
            reason.message()
        ));
        *status = 1;
        return;
    }
    match fs::remove_dir(d) {
        Ok(()) => {
            if verbose {
                println!("rmdir: removing directory, '{}'", d.display());
            }
        }
        Err(e) => {
            if !(ignore_fail && is_not_empty(&e)) {
                ui.err(&format!("failed to remove '{}': {e}", d.display()));
                *status = 1;
            }
        }
    }
}

/// Remove directory `d`, then walk up through and remove each ancestor in
/// turn for as long as they're empty, stopping at the first failure (or at
/// the filesystem root).
fn remove_with_ancestors(
    d: &std::path::Path,
    ignore_fail: bool,
    verbose: bool,
    ui: &Ui,
    status: &mut i32,
) {
    let mut cur = d.to_path_buf();
    loop {
        if let Some(reason) = protect::removal_denied(&cur) {
            ui.err(&format!(
                "failed to remove '{}': {}",
                cur.display(),
                reason.message()
            ));
            *status = 1;
            break;
        }
        match fs::remove_dir(&cur) {
            Ok(()) => {
                if verbose {
                    println!("rmdir: removing directory, '{}'", cur.display());
                }
            }
            Err(e) => {
                if !(ignore_fail && is_not_empty(&e)) {
                    ui.err(&format!("failed to remove '{}': {e}", cur.display()));
                    *status = 1;
                }
                break;
            }
        }
        if !cur.pop() || cur.as_os_str().is_empty() || cur == std::path::Path::new("/") {
            break;
        }
    }
}

/// True if `e` indicates the directory could not be removed solely because
/// it still has entries in it (`ENOTEMPTY`, or `EEXIST` on some platforms
/// that report non-empty-directory removal that way).
fn is_not_empty(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::ENOTEMPTY) || e.raw_os_error() == Some(libc::EEXIST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "user_rmdir_test_{}_{}_{}",
            std::process::id(),
            tag,
            n
        ));
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn is_not_empty_matches_enotempty() {
        let e = std::io::Error::from_raw_os_error(libc::ENOTEMPTY);
        assert!(is_not_empty(&e));
    }

    #[test]
    fn is_not_empty_rejects_unrelated_error() {
        let e = std::io::Error::from_raw_os_error(libc::ENOENT);
        assert!(!is_not_empty(&e));
    }

    #[test]
    fn remove_one_removes_empty_directory() {
        let dir = scratch_dir("remove_one_ok");
        let ui = Ui::with_color("rmdir", false);
        let mut status = 0;
        remove_one(&dir, false, false, &ui, &mut status);
        assert_eq!(status, 0);
        assert!(!dir.exists());
    }

    #[test]
    fn remove_one_non_empty_dir_fails_without_ignore() {
        let dir = scratch_dir("remove_one_fail");
        fs::write(dir.join("f.txt"), b"x").unwrap();
        let ui = Ui::with_color("rmdir", false);
        let mut status = 0;
        remove_one(&dir, false, false, &ui, &mut status);
        assert_eq!(status, 1);
        assert!(dir.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_one_non_empty_dir_ignored_with_flag() {
        let dir = scratch_dir("remove_one_ignore");
        fs::write(dir.join("f.txt"), b"x").unwrap();
        let ui = Ui::with_color("rmdir", false);
        let mut status = 0;
        remove_one(&dir, true, false, &ui, &mut status);
        assert_eq!(status, 0);
        assert!(dir.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_with_ancestors_removes_empty_chain_and_stops_at_nonempty_container() {
        // `container` holds a marker file so the ancestor walk is
        // guaranteed to hit a non-empty directory and stop there
        // deterministically, regardless of what else lives in the system
        // temp directory.
        let container = scratch_dir("ancestors");
        fs::write(container.join("marker.txt"), b"keep me").unwrap();
        let nested = container.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();

        let ui = Ui::with_color("rmdir", false);
        let mut status = 0;
        remove_with_ancestors(&nested, true, false, &ui, &mut status);

        assert_eq!(status, 0);
        assert!(!nested.exists());
        assert!(!container.join("a").exists());
        assert!(container.exists());
        assert!(container.join("marker.txt").exists());
        fs::remove_dir_all(&container).ok();
    }

    #[test]
    fn remove_missing_directory_errors() {
        let missing =
            std::env::temp_dir().join(format!("user_rmdir_missing_{}", std::process::id()));
        let ui = Ui::with_color("rmdir", false);
        let mut status = 0;
        remove_one(&missing, false, false, &ui, &mut status);
        assert_eq!(status, 1);
    }
}
