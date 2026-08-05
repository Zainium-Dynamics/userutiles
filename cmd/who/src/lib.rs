//! user who — show who is logged on (utmpx).
use std::io::{self, Write};

pub fn run() -> i32 {
    let mut short = false;
    let mut count = false;
    let mut heading = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: who [OPTION]...\nShow who is logged on.\n -q, --count print only login names and number of users\n -H, --heading print column headings\n -s, --short short form\n");
                return 0;
            }
            "--version" => {
                println!("who (user_utils) 0.1.0");
                return 0;
            }
            "-q" | "--count" => count = true,
            "-H" | "--heading" => heading = true,
            "-s" | "--short" => short = true,
            s if s.starts_with('-') => {
                eprintln!("who: invalid option -- '{s}'");
                return 1;
            }
            _ => {}
        }
    }
    let entries = read_utmp();
    if count {
        let names: Vec<_> = entries.iter().map(|e| e.user.clone()).collect();
        if !names.is_empty() {
            println!("{}", names.join(" "));
        }
        println!("# users={}", names.len());
        return 0;
    }
    let mut out = io::stdout().lock();
    if heading {
        let _ = writeln!(out, "NAME\tLINE\tTIME\tCOMMENT");
    }
    for e in entries {
        if short {
            let _ = writeln!(out, "{:<8} {:<12}", e.user, e.line);
        } else {
            let _ = writeln!(out, "{:<8} {:<12} {} {}", e.user, e.line, e.time, e.host);
        }
    }
    0
}

struct Entry {
    user: String,
    line: String,
    host: String,
    time: String,
}

fn read_utmp() -> Vec<Entry> {
    let mut v = Vec::new();
    // SAFETY: `setutxent`/`getutxent`/`endutxent` form the standard utmpx
    // iteration protocol: `setutxent` (re)starts the cursor, each
    // `getutxent` call returns either NULL (checked before deref) or a
    // pointer to a libc-owned record valid until the next utmpx call — we
    // only dereference `u` within this same loop iteration and copy out
    // owned `String`s (via the bounds-checked `cstr` helper below) and a
    // plain `i64`, never retaining the pointer itself. `endutxent` closes
    // the database. Not called concurrently with other utmpx users in this
    // single-threaded CLI.
    unsafe {
        libc::setutxent();
        loop {
            let u = libc::getutxent();
            if u.is_null() {
                break;
            }
            if (*u).ut_type != libc::USER_PROCESS {
                continue;
            }
            let user = cstr(&(*u).ut_user);
            let line = cstr(&(*u).ut_line);
            let host = cstr(&(*u).ut_host);
            let t = (*u).ut_tv.tv_sec as i64;
            v.push(Entry {
                user,
                line,
                host,
                time: format_time(t),
            });
        }
        libc::endutxent();
    }
    v
}

/// Decode a fixed-size utmpx `c_char` field into a `String`, stopping at the
/// first NUL byte or the end of the array — whichever comes first.
///
/// `ut_user`/`ut_line`/`ut_host` are fixed-size arrays that are only
/// NUL-terminated when the value is *shorter* than the field (see `man
/// utmp`); a value that exactly fills the array is not guaranteed to have a
/// trailing NUL. Using `CStr::from_ptr` directly on such a field would risk
/// scanning past the end of the array looking for a terminator. This helper
/// takes a slice (so the length is known and bounds-checked by the
/// language) instead of a raw pointer, so it never reads out of bounds and
/// requires no `unsafe` at all.
fn cstr(buf: &[libc::c_char]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn format_time(epoch: i64) -> String {
    // SAFETY: `libc::tm` (glibc) consists only of integer fields plus a
    // `tm_zone: *const c_char` pointer field; the all-zero bit pattern is
    // valid for every integer field and is a valid (null) value for the
    // pointer field, so `mem::zeroed` cannot produce an invalid value here.
    // `localtime_r` below fully populates the fields we read before we use
    // them.
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let mut t = epoch as libc::time_t;
    // SAFETY: `&t` and `&mut tm` are valid, non-null, properly aligned
    // pointers to a live `time_t` and a live `libc::tm`. `localtime_r` is
    // the reentrant variant so it touches no shared global state, and it
    // only writes into `tm`.
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    const M: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {:2} {:02}:{:02}",
        M[tm.tm_mon.clamp(0, 11) as usize],
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}
