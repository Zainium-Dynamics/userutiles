//! user pinky — lightweight finger (utmpx based).
use std::io::{self, Write};

use usercore::Ui;

/// Entry point for the `pinky` utility. Parses `std::env::args()` and prints
/// a summary of logged-in users (from the system `utmpx` database) in either
/// short (`-s`, default) or long (`-l`) format, optionally filtered to a
/// list of `USER` names.
///
/// Returns 0 on success, 1 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("pinky");
    let mut short = true;
    let mut users: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: pinky [OPTION]... [USER]...\nLightweight finger.\n -l produce long format\n -s short format (default)\n");
                return 0;
            }
            "--version" => {
                println!("pinky (user_utils) 0.1.0");
                return 0;
            }
            "-l" => short = false,
            "-s" => short = true,
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => users.push(other.to_string()),
        }
    }
    let entries = read_utmp();
    let mut out = io::stdout().lock();
    if short {
        let _ = writeln!(out, "Login Name TTY Idle When Where");
        for e in entries {
            if !users.is_empty() && !users.iter().any(|u| u == &e.user) {
                continue;
            }
            let _ = writeln!(
                out,
                "{:<8} {:<20} {:<8} {} {}",
                e.user, e.user, e.line, e.time, e.host
            );
        }
    } else {
        for e in entries {
            if !users.is_empty() && !users.iter().any(|u| u == &e.user) {
                continue;
            }
            let _ = writeln!(out, "Login name: {:<10} In real life: {}", e.user, e.user);
            let _ = writeln!(out, "Directory: (unknown) Shell: (unknown)");
            let _ = writeln!(out, "On since {} on {} from {}", e.time, e.line, e.host);
            let _ = writeln!(out);
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

/// Decode a fixed-size `ut_user`/`ut_line`/`ut_host`-style `[c_char; N]` field into
/// an owned `String`, stopping at the first NUL byte or the end of the array,
/// whichever comes first.
///
/// POSIX explicitly does *not* guarantee these fields are NUL-terminated when the
/// value fills the entire array (e.g. a maximum-length username), so treating them
/// as a NUL-terminated C string via `CStr::from_ptr` risks reading past the end of
/// the field (and potentially past the end of the `utmpx` struct) looking for a
/// terminator that may not exist. Iterating the fixed-size array itself is always
/// in-bounds regardless of whether a NUL is present, so this is both correct and
/// entirely safe.
fn fixed_cstr(buf: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn read_utmp() -> Vec<Entry> {
    let mut v = Vec::new();
    // SAFETY: `setutxent`/`getutxent`/`endutxent` are the standard utmpx-iteration
    // triple, called with no arguments and used here in matched pairs on what is a
    // single-threaded call path (they operate on process-global/thread-local
    // state, not per-call-safe, but we don't call them concurrently). `getutxent`
    // returns either NULL (checked before use) or a pointer to a `utmpx` owned by
    // libc's internal static buffer, valid until the next `getutxent`/`setutxent`/
    // `endutxent` call — we only read through `u` before advancing the iterator,
    // and copy out owned `String`s (via `fixed_cstr`, which itself performs no
    // unsafe/out-of-bounds access) before doing so. `(*u).ut_tv.tv_sec` is a plain
    // integer field read, not a pointer.
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
            let user = fixed_cstr(&(*u).ut_user);
            let line = fixed_cstr(&(*u).ut_line);
            let host = fixed_cstr(&(*u).ut_host);
            let t = (*u).ut_tv.tv_sec as i64;
            v.push(Entry {
                user,
                line,
                host,
                time: fmt(t),
            });
        }
        libc::endutxent();
    }
    v
}

fn fmt(epoch: i64) -> String {
    // `libc::tm` has no `Default` impl, but every field is a public primitive
    // (int/long/pointer), so we build a zeroed value with a plain struct literal
    // instead of `mem::zeroed`, avoiding an unsafe block here.
    let mut tm = libc::tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    let t = epoch as libc::time_t;
    // SAFETY: `t` points to a valid, initialized `libc::time_t` on the stack and
    // `tm` points to a valid, initialized `libc::tm` on the stack that outlives
    // this call. `localtime_r` only reads through the first pointer and writes
    // through the second, both for the duration of the call only.
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_cstr_stops_at_nul() {
        let buf: [libc::c_char; 8] = [b'a' as _, b'b' as _, 0, b'c' as _, 0, 0, 0, 0];
        assert_eq!(fixed_cstr(&buf), "ab");
    }

    #[test]
    fn fixed_cstr_full_array_without_nul() {
        // POSIX does not guarantee NUL-termination when the value fills the
        // whole fixed-size field; iterating the array must not read past it.
        let buf: [libc::c_char; 4] = [b'w' as _, b'x' as _, b'y' as _, b'z' as _];
        assert_eq!(fixed_cstr(&buf), "wxyz");
    }

    #[test]
    fn fixed_cstr_empty_array() {
        let buf: [libc::c_char; 0] = [];
        assert_eq!(fixed_cstr(&buf), "");
    }

    #[test]
    fn fmt_produces_hh_mm_shape() {
        let s = fmt(0);
        assert_eq!(s.len(), 5);
        assert_eq!(s.as_bytes()[2], b':');
        assert!(s[..2].parse::<u32>().is_ok());
        assert!(s[3..].parse::<u32>().is_ok());
    }
}
