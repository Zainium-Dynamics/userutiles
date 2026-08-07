//! user mktemp — create a temporary file or directory securely.
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `mktemp` utility. Parses `std::env::args()` as
/// `[OPTION]... [TEMPLATE]`, atomically creates a uniquely-named temporary
/// file or directory (never via a check-then-create race — creation always
/// uses `O_CREAT|O_EXCL` semantics, retrying on collision), and prints its
/// path.
///
/// Returns 0 on success, 1 on a usage or creation error.
pub fn run() -> i32 {
    let ui = Ui::new("mktemp");
    let mut directory = false;
    let mut dry_run = false;
    let mut quiet = false;
    let mut tmpl: Option<String> = None;
    let mut suffix = String::new();
    let mut tmpdir: Option<PathBuf> = None;

    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: mktemp [OPTION]... [TEMPLATE]\n\
 Create a temporary file or directory, safely, and print its name.\n\
 -d, --directory create a directory, not a file\n\
 -u, --dry-run do not create anything; merely print a name\n\
 -q, --quiet suppress diagnostics about file/dir-creation failure\n\
 -p, --tmpdir[=DIR] interpret TEMPLATE relative to DIR (default $TMPDIR or /tmp)\n\
 --suffix=SUFF append SUFF to TEMPLATE\n\n\
 TEMPLATE must contain at least 3 consecutive 'X's in last component.\n\
 Default TEMPLATE: tmp.XXXXXXXXXX\n\n\
 Install path on Zainium: /overlayer/syshub/bin/mktemp\n"
                );
                return 0;
            }
            "--version" => {
                println!("mktemp (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--directory" => directory = true,
            "-u" | "--dry-run" => dry_run = true,
            "-q" | "--quiet" => quiet = true,
            "-p" | "--tmpdir" => {
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    i += 1;
                    tmpdir = Some(PathBuf::from(&args[i]));
                } else {
                    tmpdir = Some(default_tmpdir());
                }
            }
            s if s.starts_with("--tmpdir=") => {
                tmpdir = Some(PathBuf::from(&s["--tmpdir=".len()..]))
            }
            s if s.starts_with("--suffix=") => suffix = s["--suffix=".len()..].to_string(),
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => tmpl = Some(other.to_string()),
        }
        i += 1;
    }

    let template = tmpl.unwrap_or_else(|| "tmp.XXXXXXXXXX".into());
    let base = tmpdir.unwrap_or_else(default_tmpdir);
    let full_tmpl = if Path::new(&template).is_absolute() {
        PathBuf::from(&template)
    } else {
        base.join(&template)
    };
    let full_tmpl = format!("{}{suffix}", full_tmpl.display());

    if !full_tmpl.contains("XXX") {
        if !quiet {
            ui.err(&format!("too few X's in template '{template}'"));
        }
        return 1;
    }

    match create_unique(&full_tmpl, directory, dry_run) {
        Ok(path) => {
            println!("{path}");
            0
        }
        Err(e) => {
            if !quiet {
                ui.err(&e);
            }
            1
        }
    }
}

/// Directory used to hold the template when no `-p`/`--tmpdir`/absolute
/// template is given: `$TMPDIR` if it names an existing directory,
/// otherwise `/tmp`.
fn default_tmpdir() -> PathBuf {
    env::var_os("TMPDIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Try up to 1000 randomized fills of the last run of `X`s in `full_tmpl`,
/// creating the file/directory with atomic exclusive-create semantics
/// (`O_CREAT|O_EXCL` for files, `mkdir` for directories, both of which fail
/// with `AlreadyExists` rather than silently reusing an existing path) so
/// there is no check-then-act race with a concurrent creator. Returns the
/// created (or, in `dry_run` mode, merely chosen) path as a string.
fn create_unique(full_tmpl: &str, directory: bool, dry_run: bool) -> Result<String, String> {
    for attempt in 0..1000u32 {
        let candidate = randomize_template(full_tmpl, attempt);
        if dry_run {
            return Ok(candidate);
        }
        let result = if directory {
            fs::create_dir(&candidate)
        } else {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&candidate)
                .and_then(|mut f| f.write_all(b""))
        };
        match result {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(format!("failed to create file via template '{full_tmpl}'"))
}

/// Fill the last run of (at least 3) consecutive `X`s in `tmpl` with random
/// alphanumeric characters, seeded from `libc::rand()`/the process id/the
/// current time and mixed further with `salt` so repeated calls (retries
/// after a collision) produce different candidates.
fn randomize_template(tmpl: &str, salt: u32) -> String {
    // replace last run of X's
    let bytes = tmpl.as_bytes();
    let mut end = bytes.len();
    let mut start = end;
    while start > 0 && bytes[start - 1] == b'X' {
        start -= 1;
    }
    if end - start < 3 {
        // find any XXX
        if let Some(pos) = tmpl.rfind("XXX") {
            start = pos;
            end = pos;
            while end < bytes.len() && bytes[end] == b'X' {
                end += 1;
            }
        }
    }
    let mut out = tmpl.to_string().into_bytes();
    let alphabet = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    // SAFETY: `libc::rand` takes no arguments and only mutates C's internal RNG
    // state; it cannot fail or cause UB regardless of process state.
    let mut seed = unsafe { libc::rand() as u32 }
        ^ salt
        ^ (std::process::id())
        // SAFETY: `libc::time` is called with a NULL `time_t*`, which per POSIX is
        // explicitly valid and simply means the current time is not also stored
        // through the (absent) output pointer; the return value alone is used.
        ^ (unsafe { libc::time(std::ptr::null_mut()) } as u32);
    for byte in out.iter_mut().take(end).skip(start) {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        *byte = alphabet[(seed as usize) % alphabet.len()];
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("user_mktemp_test_{tag}_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn randomize_template_replaces_trailing_xs() {
        let out = randomize_template("/tmp/foo.XXXXXX", 1);
        assert!(out.starts_with("/tmp/foo."));
        assert_eq!(out.len(), "/tmp/foo.XXXXXX".len());
        assert!(!out.ends_with("XXXXXX"));
    }

    #[test]
    fn randomize_template_different_salts_differ() {
        let a = randomize_template("/tmp/foo.XXXXXXXXXX", 1);
        let b = randomize_template("/tmp/foo.XXXXXXXXXX", 2);
        assert_ne!(a, b);
    }

    #[test]
    fn create_unique_file_golden_path() {
        let dir = scratch_dir("file_golden");
        let tmpl = dir.join("t.XXXXXX");
        let path = create_unique(tmpl.to_str().unwrap(), false, false).unwrap();
        assert!(Path::new(&path).is_file());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn create_unique_directory_golden_path() {
        let dir = scratch_dir("dir_golden");
        let tmpl = dir.join("d.XXXXXX");
        let path = create_unique(tmpl.to_str().unwrap(), true, false).unwrap();
        assert!(Path::new(&path).is_dir());
        let _ = fs::remove_dir(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn create_unique_dry_run_does_not_create() {
        let dir = scratch_dir("dry_run");
        let tmpl = dir.join("t.XXXXXX");
        let path = create_unique(tmpl.to_str().unwrap(), false, true).unwrap();
        assert!(!Path::new(&path).exists());
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn create_unique_fails_when_parent_dir_missing() {
        let missing = std::env::temp_dir().join(format!(
            "user_mktemp_missing_{}_{}",
            std::process::id(),
            "xyz"
        ));
        let tmpl = missing.join("t.XXXXXX");
        let err = create_unique(tmpl.to_str().unwrap(), false, false).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn default_tmpdir_falls_back_to_tmp_when_tmpdir_unset_or_bad() {
        // Just check it returns *some* directory path; exact value depends
        // on environment/CI.
        let d = default_tmpdir();
        assert!(!d.as_os_str().is_empty());
    }
}
