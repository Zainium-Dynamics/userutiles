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
    let mut files: Vec<PathBuf> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: touch [OPTION]... FILE...\n\
 Update the access and modification times of each FILE to the current time.\n\
 A FILE argument that does not exist is created empty (unless -c).\n\n\
 -a change only the access time\n\
 -c, --no-create do not create any files\n\
 -d, --date=STRING parse STRING and use it instead of current time\n\
 -m change only the modification time\n\
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

    let times = match resolve_times(date_spec.as_deref(), ref_file.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("touch: {e}");
            return 1;
        }
    };

    let mut status = 0;
    for path in &files {
        if let Err(e) = touch_one(path, no_create, access_only, modify_only, times) {
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
        let secs = parse_date(spec)?;
        let ts = libc::timespec {
            tv_sec: secs,
            tv_nsec: 0,
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

/// Accept unix epoch seconds, or YYYY-MM-DD, or YYYY-MM-DDTHH:MM:SS (local).
fn parse_date(s: &str) -> Result<i64, String> {
    if let Ok(n) = s.parse::<i64>() {
        return Ok(n);
    }
    // YYYY-MM-DD
    let s = s.trim();
    let (date, time) = if let Some((d, t)) = s.split_once('T') {
        (d, t)
    } else if let Some((d, t)) = s.split_once(' ') {
        (d, t)
    } else {
        (s, "00:00:00")
    };
    let dparts: Vec<_> = date.split('-').collect();
    if dparts.len() != 3 {
        return Err(format!("invalid date format '{s}'"));
    }
    let year: i32 = dparts[0]
        .parse()
        .map_err(|_| format!("invalid date '{s}'"))?;
    let mon: i32 = dparts[1]
        .parse()
        .map_err(|_| format!("invalid date '{s}'"))?;
    let day: i32 = dparts[2]
        .parse()
        .map_err(|_| format!("invalid date '{s}'"))?;
    let tparts: Vec<_> = time.split(':').collect();
    let hour: i32 = tparts.first().and_then(|x| x.parse().ok()).unwrap_or(0);
    let min: i32 = tparts.get(1).and_then(|x| x.parse().ok()).unwrap_or(0);
    let sec: i32 = tparts
        .get(2)
        .and_then(|x| x.split('.').next())
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);

    // SAFETY: `libc::tm` (glibc) is a `#[repr(C)]` struct made up only of
    // integer fields (`c_int`/`c_long`) and, on glibc, a `tm_zone: *const
    // c_char` pointer field. The all-zero bit pattern is a valid value for
    // every integer field and is a valid (null) value for the pointer field,
    // so `mem::zeroed` cannot produce an invalid bit pattern here. Every
    // field we actually rely on (`tm_year`..`tm_isdst`) is explicitly
    // overwritten immediately below before the struct is used.
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    tm.tm_year = year - 1900;
    tm.tm_mon = mon - 1;
    tm.tm_mday = day;
    tm.tm_hour = hour;
    tm.tm_min = min;
    tm.tm_sec = sec;
    tm.tm_isdst = -1;
    // SAFETY: `&mut tm` coerces to a valid, non-null, properly aligned
    // `*mut libc::tm` pointing at a fully-initialized stack value (all
    // fields set above), which is exactly what `mktime` requires; it may
    // normalize the fields in place but performs no out-of-bounds access.
    let t = unsafe { libc::mktime(&mut tm) };
    if t == -1 {
        return Err(format!("invalid date '{s}'"));
    }
    Ok(t as i64)
}

fn touch_one(
    path: &Path,
    no_create: bool,
    access_only: bool,
    modify_only: bool,
    times: Times,
) -> io::Result<()> {
    if !path.exists() {
        if no_create {
            return Ok(());
        }
        // create empty file (and parents are NOT auto-created — GNU touch does not mkdir -p)
        OpenOptions::new().create(true).write(true).open(path)?;
    }

    let mut at = times.atime;
    let mut mt = times.mtime;
    // UTIME_OMIT = max i64 on some; use libc::UTIME_OMIT if available
    const UTIME_OMIT: i64 = (1i64 << 30) - 2; // glibc UTIME_OMIT
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
    // `0` requests the default (non-symlink-following) behavior.
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), ts.as_ptr(), 0) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let _ = File::open(path); // ensure readable for some FS
    Ok(())
}
