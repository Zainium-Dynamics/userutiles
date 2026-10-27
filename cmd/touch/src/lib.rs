//! user touch — update access/modification times; create empty files.
//!
//! For Zainium *structured* create (mkdir-p + no-overwrite file create), prefer
//! `struct` — see `struct --help`. This `touch` is the traditional GNU-compatible tool.
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run() -> i32 {
    let mut no_create = false;
    let mut access_only = false;
    let mut modify_only = false;
    let mut date_spec: Option<String> = None;
    let mut ref_file: Option<PathBuf> = None;
    let mut stamp: Option<String> = None;
    let mut no_deref = false;
    let mut files: Vec<PathBuf> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => {
                print!(
                    "Usage: touch [OPTION]... FILE...\n\
 Update the access and modification times of each FILE to the current time.\n\
 A FILE argument that does not exist is created empty (unless -c).\n\n\
 -a change only the access time\n\
 -c, --no-create do not create any files\n\
 -d, --date=STRING parse STRING and use it instead of current time\n\
 -h, --no-dereference affect each symbolic link instead of any referenced file\n\
 -m change only the modification time\n\
 -t STAMP use [[CC]YY]MMDDhhmm[.ss] instead of current time\n\
 --time=WORD change the specified time: access/atime/use or modify/mtime\n\
 -r, --reference=FILE use this file's times instead of current time\n\
 --help display this help and exit\n\
 --version output version information and exit\n\n\
 Zainium note: for safe create-with-parents / no-overwrite workflows,\n\
 use `struct` (see `struct --help`).\n"
                );
                return 0;
            }
            "--version" => {
                println!("touch (user_utils) 0.1.0");
                return 0;
            }
            "-a" => access_only = true,
            "-h" | "--no-dereference" => no_deref = true,
            "-f" => {}
            "-t" => {
                i += 1;
                stamp = args.get(i).cloned();
            }
            s if s.starts_with("--time=") => match &s[7..] {
                "access" | "atime" | "use" => access_only = true,
                "modify" | "mtime" => modify_only = true,
                other => {
                    eprintln!("touch: invalid argument '{other}' for '--time'");
                    return 1;
                }
            },
            "-m" => modify_only = true,
            "-c" | "--no-create" => no_create = true,
            "-d" | "--date" => {
                i += 1;
                date_spec = args.get(i).cloned();
            }
            s if s.starts_with("--date=") => date_spec = Some(s["--date=".len()..].to_string()),
            "-r" | "--reference" => {
                i += 1;
                ref_file = args.get(i).map(PathBuf::from);
            }
            s if s.starts_with("--reference=") => {
                ref_file = Some(PathBuf::from(&s["--reference=".len()..]));
            }
            s if s.starts_with('-') && s != "-" => {
                for ch in s.chars().skip(1) {
                    match ch {
                        'a' => access_only = true,
                        'm' => modify_only = true,
                        'c' => no_create = true,
                        'h' => no_deref = true,
                        'f' => {}
                        _ => {
                            eprintln!("touch: invalid option -- '{ch}'");
                            return 1;
                        }
                    }
                }
            }
            other => files.push(PathBuf::from(other)),
        }
        i += 1;
    }

    if files.is_empty() {
        eprintln!("touch: missing file operand");
        eprintln!("Try 'touch --help' for more information.");
        return 1;
    }

    // Both unset → update both (GNU default)
    if !access_only && !modify_only {
        access_only = true;
        modify_only = true;
    }

    // -t [[CC]YY]MMDDhhmm[.ss] is a date in the local zone
    if let Some(t) = &stamp {
        let (main, ss) = t.split_once('.').unwrap_or((t.as_str(), "00"));
        let ok = main.bytes().all(|c| c.is_ascii_digit()) && matches!(main.len(), 8 | 10 | 12) && ss.len() == 2;
        if !ok {
            eprintln!("touch: invalid date format '{t}'");
            return 1;
        }
        let (year, rest) = match main.len() {
            12 => (main[..4].to_string(), &main[4..]),
            10 => {
                let yy: u32 = main[..2].parse().unwrap_or(0);
                ((if yy < 69 { 2000 + yy } else { 1900 + yy }).to_string(), &main[2..])
            }
            _ => (String::new(), main),
        };
        // MMDDhhmm alone is this year: MM/DD takes the current year
        date_spec = Some(if year.is_empty() {
            format!("{}/{} {}:{}:{ss}", &rest[..2], &rest[2..4], &rest[4..6], &rest[6..8])
        } else {
            format!("{year}-{}-{} {}:{}:{ss}", &rest[..2], &rest[2..4], &rest[4..6], &rest[6..8])
        });
    }

    let times = match resolve_times(date_spec.as_deref(), ref_file.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("touch: {e}");
            return 1;
        }
    };

    let mut status = 0;
    for path in &files {
        if let Err(e) = touch_one(path, no_create || no_deref, access_only, modify_only, times, no_deref) {
            eprintln!("touch: cannot touch '{}': {e}", path.display());
            status = 1;
        }
    }
    status
}

#[derive(Clone, Copy)]
struct Times {
    atime: libc::timespec,
    mtime: libc::timespec,
}

fn resolve_times(date_spec: Option<&str>, ref_file: Option<&Path>) -> Result<Times, String> {
    if let Some(r) = ref_file {
        let meta = fs::metadata(r).map_err(|e| format!("{}: {e}", r.display()))?;
        use std::os::unix::fs::MetadataExt;
        return Ok(Times {
            atime: libc::timespec {
                tv_sec: meta.atime(),
                tv_nsec: meta.atime_nsec(),
            },
            mtime: libc::timespec {
                tv_sec: meta.mtime(),
                tv_nsec: meta.mtime_nsec(),
            },
        });
    }
    if let Some(spec) = date_spec {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?;
        let (secs, nsec) = user_date::parse_date(spec, now.as_secs() as i64, now.subsec_nanos(), false)
            .ok_or_else(|| format!("invalid date format '{spec}'"))?;
        let ts = libc::timespec {
            tv_sec: secs as libc::time_t,
            tv_nsec: nsec as _,
        };
        return Ok(Times {
            atime: ts,
            mtime: ts,
        });
    }
    // current time
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let ts = libc::timespec {
        tv_sec: now.as_secs() as libc::time_t,
        tv_nsec: now.subsec_nanos() as i64,
    };
    Ok(Times {
        atime: ts,
        mtime: ts,
    })
}

fn touch_one(
    path: &Path,
    no_create: bool,
    access_only: bool,
    modify_only: bool,
    times: Times,
    no_deref: bool,
) -> io::Result<()> {
    if path.symlink_metadata().is_err() {
        if no_create {
            return Ok(());
        }
        // create empty file (and parents are NOT auto-created — GNU touch does not mkdir -p)
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
    }

    let mut at = times.atime;
    let mut mt = times.mtime;
    // UTIME_OMIT = max i64 on some; use libc::UTIME_OMIT if available
    const UTIME_OMIT: i64 = libc::UTIME_OMIT as i64;
    if !access_only {
        at.tv_nsec = UTIME_OMIT;
    }
    if !modify_only {
        mt.tv_nsec = UTIME_OMIT;
    }
    // If only one of a/m requested, the other is OMIT
    // Wait: if access_only true and modify_only false, mtime OMIT
    // if both true, both set
    if access_only && !modify_only {
        mt.tv_nsec = UTIME_OMIT;
        at = times.atime;
    } else if modify_only && !access_only {
        at.tv_nsec = UTIME_OMIT;
        mt = times.mtime;
    } else {
        at = times.atime;
        mt = times.mtime;
    }

    let ts = [at, mt];
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains interior NUL"))?;
    // SAFETY: `c` is a live `CString` (still in scope, not dropped) so
    // `c.as_ptr()` is a valid, null-terminated C string for the duration of
    // the call. `ts` is a local `[libc::timespec; 2]`, matching the
    // `const struct timespec times[2]` array `utimensat` requires, and
    // `ts.as_ptr()` points at both valid elements. `libc::AT_FDCWD` is the
    // documented sentinel telling the kernel to resolve `c` relative to the
    // current working directory rather than treating it as an fd, and flags
    // flags: AT_SYMLINK_NOFOLLOW for -h (the link itself), else 0 (what it points at).
    let flags = if no_deref { libc::AT_SYMLINK_NOFOLLOW } else { 0 };
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), ts.as_ptr(), flags) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let _ = File::open(path); // ensure readable for some FS
    Ok(())
}
