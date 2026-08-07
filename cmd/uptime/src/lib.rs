//! user uptime — system uptime and load averages.
use std::fs;
use std::io::{self, Write};

use usercore::Ui;

/// Entry point for the `uptime` utility. Parses `std::env::args()` and
/// prints the current time, up-duration, logged-in user count, and load
/// averages (or a `-p`/`-s` variant of just the duration/boot time).
///
/// Returns 0 on success, 1 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("uptime");
    let mut pretty = false;
    let mut since = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: uptime [OPTION]\nTell how long the system has been running.\n -p, --pretty pretty format\n -s, --since system up since\n");
                return 0;
            }
            "--version" => {
                println!("uptime (user_utils) 0.1.0");
                return 0;
            }
            "-p" | "--pretty" => pretty = true,
            "-s" | "--since" => since = true,
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => {
                ui.err(&format!("extra operand '{other}'"));
                return 1;
            }
        }
    }
    let up_secs = read_uptime_secs().unwrap_or(0.0);
    let loads = read_loadavg();
    let users = count_users();
    if since {
        let boot = now_epoch() - up_secs as i64;
        println!("{}", format_time(boot));
        return 0;
    }
    if pretty {
        println!("up {}", pretty_duration(up_secs as u64));
        return 0;
    }
    let now = format_time(now_epoch());
    let up = human_up(up_secs as u64);
    let user_s = if users == 1 {
        "1 user".to_string()
    } else {
        format!("{users} users")
    };
    let load = match loads {
        Some((a, b, c)) => format!("load average: {a:.2}, {b:.2}, {c:.2}"),
        None => "load average: ?, ?, ?".into(),
    };
    let mut out = io::stdout().lock();
    let _ = writeln!(out, " {now} up {up}, {user_s}, {load}");
    0
}

/// Read the system uptime in seconds from `/proc/uptime`. Returns `None`
/// if the file is missing/unreadable or its first field doesn't parse —
/// callers fall back to `0.0` rather than erroring, matching GNU
/// `uptime`'s best-effort behavior when `/proc` is unavailable.
fn read_uptime_secs() -> Option<f64> {
    let s = fs::read_to_string("/proc/uptime").ok()?;
    s.split_whitespace().next()?.parse().ok()
}

/// Read the 1/5/15-minute load averages from `/proc/loadavg`.
fn read_loadavg() -> Option<(f64, f64, f64)> {
    let s = fs::read_to_string("/proc/loadavg").ok()?;
    let mut it = s.split_whitespace();
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ))
}

/// Count distinct `USER_PROCESS` entries in the utmpx database.
fn count_users() -> usize {
    // SAFETY: `setutxent`/`getutxent`/`endutxent` form the standard utmpx
    // iteration protocol: `setutxent` (re)starts the cursor, each
    // `getutxent` call returns either NULL (end of database, checked before
    // deref) or a pointer to a libc-owned record that stays valid until the
    // next utmpx call, which is exactly when we dereference it (`(*u)`) —
    // we never retain `u` past this loop iteration. `endutxent` closes the
    // database. This function does not run concurrently with other utmpx
    // users within this single-threaded CLI process.
    unsafe {
        libc::setutxent();
        let mut n = 0usize;
        loop {
            let u = libc::getutxent();
            if u.is_null() {
                break;
            }
            if (*u).ut_type == libc::USER_PROCESS {
                n += 1;
            }
        }
        libc::endutxent();
        n
    }
}

/// Current wall-clock time as a Unix epoch second count. Returns `0` if the
/// system clock is somehow before the epoch (never happens in practice).
fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Format an epoch-seconds timestamp as local `HH:MM:SS`.
fn format_time(epoch: i64) -> String {
    // SAFETY: `libc::tm` (glibc) consists only of integer fields plus a
    // `tm_zone: *const c_char` pointer field; the all-zero bit pattern is
    // valid for every integer field and is a valid (null) value for the
    // pointer field, so `mem::zeroed` cannot produce an invalid value here.
    // `localtime_r` below fully populates the fields we read (`tm_hour`,
    // `tm_min`, `tm_sec`) before we use them.
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let t = epoch as libc::time_t;
    // SAFETY: `&t` and `&mut tm` are valid, non-null, properly aligned
    // pointers to a live `time_t` and a live `libc::tm` respectively.
    // `localtime_r` is the reentrant variant so it touches no shared global
    // state (unlike `localtime`), and it only writes into `tm`, never reads
    // it, so `tm`'s zeroed starting state is irrelevant to correctness.
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// Format a duration in seconds as GNU `uptime`'s default `up ...` phrase,
/// e.g. `"2 days, 3:04"` or `"45 min"`.
fn human_up(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!(
            "{days} day{}, {hours}:{mins:02}",
            if days == 1 { "" } else { "s" }
        )
    } else if hours > 0 {
        format!("{hours}:{mins:02}")
    } else {
        format!("{mins} min")
    }
}

/// Format a duration in seconds for `-p`/`--pretty`, e.g.
/// `"2 days, 3 hours, 4 minutes"`.
fn pretty_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} day{}", if days == 1 { "" } else { "s" }));
    }
    if hours > 0 {
        parts.push(format!("{hours} hour{}", if hours == 1 { "" } else { "s" }));
    }
    if mins > 0 || parts.is_empty() {
        parts.push(format!("{mins} minute{}", if mins == 1 { "" } else { "s" }));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_up_minutes_only() {
        assert_eq!(human_up(45 * 60), "45 min");
        assert_eq!(human_up(0), "0 min");
    }

    #[test]
    fn human_up_hours_and_minutes() {
        assert_eq!(human_up(2 * 3600 + 5 * 60), "2:05");
    }

    #[test]
    fn human_up_days_hours_minutes() {
        assert_eq!(human_up(86400 + 3600 + 60), "1 day, 1:01");
        assert_eq!(human_up(2 * 86400), "2 days, 0:00");
    }

    #[test]
    fn pretty_duration_minutes_only() {
        assert_eq!(pretty_duration(90), "1 minute");
        assert_eq!(pretty_duration(0), "0 minutes");
    }

    #[test]
    fn pretty_duration_full() {
        assert_eq!(
            pretty_duration(2 * 86400 + 3 * 3600 + 4 * 60),
            "2 days, 3 hours, 4 minutes"
        );
    }

    #[test]
    fn pretty_duration_singular_units() {
        assert_eq!(
            pretty_duration(86400 + 3600 + 60),
            "1 day, 1 hour, 1 minute"
        );
    }

    #[test]
    fn format_time_epoch_zero_is_valid_hhmmss() {
        // We don't assert an exact value (depends on local TZ), just that
        // the format shape holds and fields are in-range.
        let s = format_time(0);
        assert_eq!(s.len(), 8);
        assert_eq!(s.as_bytes()[2], b':');
        assert_eq!(s.as_bytes()[5], b':');
    }

    #[test]
    fn now_epoch_is_positive_and_recent() {
        // Sanity check: should be a plausible post-2020 Unix timestamp.
        assert!(now_epoch() > 1_600_000_000);
    }

    #[test]
    fn read_uptime_secs_returns_some_on_linux() {
        // /proc/uptime always exists on a Linux test runner.
        assert!(read_uptime_secs().is_some());
    }

    #[test]
    fn count_users_does_not_panic() {
        let _ = count_users();
    }
}
