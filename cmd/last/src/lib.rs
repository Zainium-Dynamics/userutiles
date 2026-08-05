//! user last — show a listing of last logged-in users, reconstructed from
//! wtmp-format login records (default `/var/log/wtmp`).
//!
//! Session end-state detection (normal logout / clean shutdown ("down") /
//! abrupt power loss ("crash")) and the exact row formatting for each
//! (including the `--time-format notime` and `-t/--until` "gone - no
//! logout" special cases) were reverse-engineered by diffing this
//! implementation's output against the real `last(1)` binary on real wtmp
//! history — see `checklist/last.md` for the verification log.
mod utmpx;

use std::collections::HashMap;

use utmpx::{BOOT_TIME, DEAD_PROCESS, RUN_LVL, Record, USER_PROCESS};
use usercore::Ui;

const HELP: &str = "Usage: last [options] [<username>...] [<tty>...]\n\
Show a listing of last logged in users.\n\n\
  -a, --hostlast        display hostnames in the last column\n\
  -d, --dns             translate the IP number back into a hostname\n\
  -f, --file <file>     use a specific file instead of /var/log/wtmp\n\
  -F, --fulltimes       print full login and logout times and dates\n\
  -n, --limit <number>  how many lines to show\n\
  -R, --nohostname       don't display the hostname field\n\
  -s, --since <time>    display the lines since the specified time\n\
  -t, --until <time>    display the lines until the specified time\n\
  -x, --system          display system shutdown entries and run level changes\n\
      --time-format <format>  notime|short|full|iso\n\
  -h, --help            display this help and exit\n\
      --version         output version information and exit\n";

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimeFormat {
    NoTime,
    Short,
    Full,
    Iso,
}

struct Options {
    file: String,
    system: bool,
    dns: bool,
    hostlast: bool,
    nohostname: bool,
    limit: Option<usize>,
    time_format: TimeFormat,
    since: Option<i64>,
    until: Option<i64>,
    filters: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            file: "/var/log/wtmp".to_string(),
            system: false,
            dns: false,
            hostlast: false,
            nohostname: false,
            limit: None,
            time_format: TimeFormat::Short,
            since: None,
            until: None,
            filters: Vec::new(),
        }
    }
}

pub fn run() -> i32 {
    let ui = Ui::new("last");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut opts = Options::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("last (user_utils) 0.1.0");
                return 0;
            }
            "-x" | "--system" => opts.system = true,
            "-d" | "--dns" => opts.dns = true,
            "-a" | "--hostlast" => opts.hostlast = true,
            "-R" | "--nohostname" => opts.nohostname = true,
            "-F" | "--fulltimes" => opts.time_format = TimeFormat::Full,
            "-f" | "--file" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                opts.file = v.clone();
            }
            "-n" | "--limit" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                match v.parse::<usize>() {
                    Ok(n) => opts.limit = Some(n),
                    Err(_) => {
                        ui.err(&format!("invalid limit: '{v}'"));
                        return 1;
                    }
                }
            }
            "-s" | "--since" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                match parse_datetime(v) {
                    Some(t) => opts.since = Some(t),
                    None => {
                        ui.err(&format!("invalid time: '{v}'"));
                        return 1;
                    }
                }
            }
            "-t" | "--until" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                match parse_datetime(v) {
                    Some(t) => opts.until = Some(t),
                    None => {
                        ui.err(&format!("invalid time: '{v}'"));
                        return 1;
                    }
                }
            }
            "--time-format" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                match v.as_str() {
                    "notime" => opts.time_format = TimeFormat::NoTime,
                    "short" => opts.time_format = TimeFormat::Short,
                    "full" => opts.time_format = TimeFormat::Full,
                    "iso" => opts.time_format = TimeFormat::Iso,
                    other => {
                        ui.err(&format!("invalid time format: {other}"));
                        return 1;
                    }
                }
            }
            s if s.len() > 1 && s.starts_with('-') && s[1..].chars().all(|c| c.is_ascii_digit()) => {
                opts.limit = s[1..].parse().ok();
            }
            s if s.starts_with('-') && !s.starts_with("--") && s.len() > 1 => {
                for c in s[1..].chars() {
                    match c {
                        'x' => opts.system = true,
                        'd' => opts.dns = true,
                        'a' => opts.hostlast = true,
                        'R' => opts.nohostname = true,
                        'F' => opts.time_format = TimeFormat::Full,
                        other => {
                            ui.err(&format!("invalid option -- '{other}'"));
                            return 1;
                        }
                    }
                }
            }
            other if !other.starts_with('-') => opts.filters.push(other.to_string()),
            other => {
                ui.err(&format!("unknown option -- '{other}'"));
                return 1;
            }
        }
        i += 1;
    }

    let records = match utmpx::read_all(&opts.file) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    let sessions = build_sessions(&records);

    let mut rows: Vec<&Session> = sessions
        .iter()
        .filter(|s| opts.system || s.kind != Kind::Shutdown)
        .filter(|s| opts.since.map_or(true, |since| s.start >= since))
        .filter(|s| opts.until.map_or(true, |until| s.start <= until))
        .filter(|s| matches_filters(s, &opts.filters))
        .collect();
    rows.reverse(); // sessions are built oldest-first; display newest-first.
    if let Some(n) = opts.limit {
        rows.truncate(n);
    }

    for s in &rows {
        println!("{}", format_row(s, &opts));
    }

    // The trailer is omitted entirely for `notime`; for `short` it still
    // uses the `full` format (not `short`) — verified against the real
    // binary, which shows the same trailer regardless of `short` vs `full`
    // but switches to `iso` when that's the requested format.
    let trailer_format = match opts.time_format {
        TimeFormat::NoTime => None,
        TimeFormat::Short => Some(TimeFormat::Full),
        other => Some(other),
    };
    if let (Some(first), Some(format)) = (records.first(), trailer_format) {
        println!();
        println!("wtmp begins {}", format_datetime(format, first.time));
    }

    0
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    User,
    Reboot,
    Shutdown,
}

#[derive(Clone, Copy)]
enum End {
    Open,
    Normal(i64),
    Down(i64),
    Crash(i64),
}

struct Session {
    kind: Kind,
    user: String,
    line: String,
    host: String,
    start: i64,
    end: End,
}

/// Reconstructs login/reboot/shutdown sessions from raw wtmp records, in
/// file (chronological) order. Mirrors real `last(1)`'s state machine:
/// - a `USER_PROCESS` opens a session, closed normally by a matching
///   `DEAD_PROCESS` (correlated by pid);
/// - a shutdown marker (`RUN_LVL` with `ut_user == "shutdown"`) closes any
///   still-open user sessions as "down" (clean shutdown, no explicit
///   logout) and closes the current "system boot" pseudo-session normally;
/// - a `BOOT_TIME` record closes any still-open user sessions *and* the
///   current reboot pseudo-session as "crash" (abrupt loss — no shutdown
///   marker was seen first), and closes any open shutdown pseudo-session
///   normally (its "downtime" duration just ended).
fn build_sessions(records: &[Record]) -> Vec<Session> {
    let mut sessions = Vec::new();
    let mut open_user: HashMap<i32, usize> = HashMap::new();
    let mut open_reboot: Option<usize> = None;
    let mut open_shutdown: Option<usize> = None;

    for r in records {
        if r.rec_type == USER_PROCESS {
            sessions.push(Session {
                kind: Kind::User,
                user: r.user.clone(),
                line: r.line.clone(),
                host: r.host.clone(),
                start: r.time,
                end: End::Open,
            });
            open_user.insert(r.pid, sessions.len() - 1);
        } else if r.rec_type == DEAD_PROCESS {
            if let Some(idx) = open_user.remove(&r.pid) {
                sessions[idx].end = End::Normal(r.time);
            }
        } else if r.rec_type == RUN_LVL && r.user == "shutdown" && r.line == "~" {
            for (_, idx) in open_user.drain() {
                sessions[idx].end = End::Down(r.time);
            }
            if let Some(idx) = open_reboot.take() {
                sessions[idx].end = End::Normal(r.time);
            }
            sessions.push(Session {
                kind: Kind::Shutdown,
                user: "shutdown".to_string(),
                line: "system down".to_string(),
                host: r.host.clone(),
                start: r.time,
                end: End::Open,
            });
            open_shutdown = Some(sessions.len() - 1);
        } else if r.rec_type == BOOT_TIME {
            for (_, idx) in open_user.drain() {
                sessions[idx].end = End::Crash(r.time);
            }
            if let Some(idx) = open_reboot.take() {
                sessions[idx].end = End::Crash(r.time);
            }
            if let Some(idx) = open_shutdown.take() {
                sessions[idx].end = End::Normal(r.time);
            }
            sessions.push(Session {
                kind: Kind::Reboot,
                user: "reboot".to_string(),
                line: "system boot".to_string(),
                host: r.host.clone(),
                start: r.time,
                end: End::Open,
            });
            open_reboot = Some(sessions.len() - 1);
        }
    }

    sessions
}

fn matches_filters(s: &Session, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    filters.iter().any(|f| {
        f == &s.user
            || f == &s.line
            || s.line
                .strip_prefix("tty")
                .or_else(|| s.line.strip_prefix("pts/"))
                .is_some_and(|rest| rest == f)
    })
}

/// The end state actually displayed, accounting for `-t/--until`: a
/// session whose *real* end (in the full, untruncated wtmp history) falls
/// after the queried `until` boundary hasn't observably ended as of that
/// point. Real `last` shows that as `"gone - no logout"` for a login
/// session (there's no logout to report yet, but unlike a session that's
/// genuinely still open today, we know it later did end) — verified
/// against the real binary with `-t`. Reboot/shutdown pseudo-sessions have
/// no "logout" concept, so they just show as still-open instead.
enum Display {
    Open,
    Gone,
    Normal(i64),
    Down(i64),
    Crash(i64),
}

fn effective_end(s: &Session, until: Option<i64>) -> Display {
    let cutoff = |t: i64| until.is_some_and(|u| t > u);
    match s.end {
        End::Open => Display::Open,
        End::Normal(t) if cutoff(t) => cutoff_display(s.kind),
        End::Down(t) if cutoff(t) => cutoff_display(s.kind),
        End::Crash(t) if cutoff(t) => cutoff_display(s.kind),
        End::Normal(t) => Display::Normal(t),
        End::Down(t) => Display::Down(t),
        End::Crash(t) => Display::Crash(t),
    }
}

fn cutoff_display(kind: Kind) -> Display {
    if kind == Kind::User { Display::Gone } else { Display::Open }
}

fn format_row(s: &Session, opts: &Options) -> String {
    let host_resolved = if opts.dns { resolve_host(&s.host) } else { s.host.clone() };

    let mut row = format!("{:<8} {:<12} ", s.user, s.line);
    if !opts.nohostname && !opts.hostlast {
        row.push_str(&format!("{:<16} ", truncate_host(&host_resolved)));
    }

    let disp = effective_end(s, opts.until);
    row.push_str(&format_time_part(s, &disp, opts.time_format));

    if !opts.nohostname && opts.hostlast {
        // The row (everything before the host) is padded to a fixed total
        // width of 60 columns, then the untruncated host is appended
        // directly — verified against the real binary across several
        // differing row lengths, all of which put the host at column 60.
        row = format!("{row:<60}{host_resolved}");
    }

    row
}

fn format_time_part(s: &Session, disp: &Display, format: TimeFormat) -> String {
    let running_word = if s.kind == Kind::User { "logged in" } else { "running" };

    if format == TimeFormat::NoTime {
        return match disp {
            Display::Open => format!("  {running_word}"),
            Display::Gone => "  gone - no logout".to_string(),
            Display::Normal(_) | Display::Down(_) | Display::Crash(_) => {
                let duration = format_duration(duration_secs(s.start, disp));
                format!(" {}({duration})", duration_paren_gap(&duration))
            }
        };
    }

    let start = format_datetime(format, s.start);
    match disp {
        Display::Open => format!("{start}   still {running_word}"),
        Display::Gone => format!("{start}    gone - no logout"),
        Display::Normal(end) => {
            let duration = format_duration(end - s.start);
            if format == TimeFormat::Short {
                let end_str = short_time_of_day(*end);
                let pad = duration_paren_gap(&duration);
                format!("{start} - {end_str}{pad}({duration})")
            } else {
                let end_str = format_datetime(format, *end);
                let width = end_state_field_width(&start, &duration);
                format!("{start} - {end_str:<width$}({duration})")
            }
        }
        Display::Down(end) => {
            let duration = format_duration(end - s.start);
            if format == TimeFormat::Short {
                let pad = duration_paren_gap(&duration);
                format!("{start} - down {pad}({duration})")
            } else {
                let width = end_state_field_width(&start, &duration);
                format!("{start} - {:<width$}({duration})", "down")
            }
        }
        Display::Crash(end) => {
            let duration = format_duration(end - s.start);
            if format == TimeFormat::Short {
                let pad = duration_paren_gap(&duration);
                format!("{start} - crash{pad}({duration})")
            } else {
                let width = end_state_field_width(&start, &duration);
                format!("{start} - {:<width$}({duration})", "crash")
            }
        }
    }
}

/// The gap before the `(duration)` parenthesis is one space narrower when
/// the duration spans multiple days (`"D+HH:MM"`) than when it's a plain
/// `"HH:MM"` — verified against the real binary across both single- and
/// multi-day sessions.
fn duration_paren_gap(duration: &str) -> &'static str {
    if duration.contains('+') { " " } else { "  " }
}

/// In `full`/`iso` mode, the "down"/"crash"/end-datetime field is padded to
/// a fixed width (the same width regardless of which of the three fills
/// it) equal to the login-time string's own length plus the same 1-or-2
/// gap `duration_paren_gap` uses — verified against the real binary: e.g.
/// `"down"` and a full end-datetime both pad out to width 26 for a
/// single-day duration, 25 for a multi-day one.
fn end_state_field_width(start: &str, duration: &str) -> usize {
    start.chars().count() + duration_paren_gap(duration).len()
}

fn duration_secs(start: i64, disp: &Display) -> i64 {
    match disp {
        Display::Normal(end) | Display::Down(end) | Display::Crash(end) => end - start,
        _ => 0,
    }
}

fn format_duration(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    if days > 0 {
        format!("{days}+{hh:02}:{mm:02}")
    } else {
        format!("{hh:02}:{mm:02}")
    }
}

/// Truncates a hostname/kernel-version string to 16 display columns,
/// replacing the last character with `*` when it doesn't fit — matches
/// real `last(1)`'s host column exactly (verified: `7.0.13-200.fc44.x86_64`
/// truncates to `7.0.13-200.fc44*`).
fn truncate_host(s: &str) -> String {
    const WIDTH: usize = 16;
    let count = s.chars().count();
    if count <= WIDTH {
        format!("{s:<WIDTH$}")
    } else {
        let mut truncated: String = s.chars().take(WIDTH - 1).collect();
        truncated.push('*');
        truncated
    }
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn local_time(t: i64) -> libc::tm {
    // SAFETY: `localtime_r` writes into a zero-initialized `tm` (a valid
    // all-zero bit pattern for every field, including the `tm_zone`
    // pointer) and fully populates it before we read any field.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let time = t as libc::time_t;
        libc::localtime_r(&time, &mut tm);
        tm
    }
}

fn short_time_of_day(t: i64) -> String {
    let tm = local_time(t);
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

fn format_datetime(format: TimeFormat, t: i64) -> String {
    let tm = local_time(t);
    match format {
        TimeFormat::Short => format!(
            "{} {} {:2} {:02}:{:02}",
            WEEKDAYS[tm.tm_wday as usize], MONTHS[tm.tm_mon as usize], tm.tm_mday, tm.tm_hour, tm.tm_min
        ),
        TimeFormat::Full => format!(
            "{} {} {:2} {:02}:{:02}:{:02} {}",
            WEEKDAYS[tm.tm_wday as usize],
            MONTHS[tm.tm_mon as usize],
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec,
            tm.tm_year + 1900
        ),
        TimeFormat::Iso => {
            let tz_minutes = if tm.tm_isdst < 0 { 0 } else { tm.tm_gmtoff / 60 };
            let tz_hours = tz_minutes / 60;
            let tz_minutes = (tz_minutes % 60).abs();
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{tz_hours:+03}:{tz_minutes:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min,
                tm.tm_sec
            )
        }
        TimeFormat::NoTime => String::new(),
    }
}

/// Reverse-resolves `host` to a name via real `getnameinfo(3)` when it
/// parses as an IPv4/IPv6 address; anything else (an already-symbolic host
/// like `local`, or a resolution failure) is returned unchanged.
fn resolve_host(host: &str) -> String {
    use std::net::IpAddr;
    let Ok(ip) = host.parse::<IpAddr>() else {
        return host.to_string();
    };
    if ip.is_unspecified() {
        return host.to_string();
    }

    let sockaddr = match ip {
        IpAddr::V4(v4) => {
            // SAFETY: `sockaddr_in` is a plain C struct of integer/array
            // fields; the all-zero bit pattern is valid for all of them,
            // and every field is then explicitly assigned below or passed
            // to `getnameinfo` in the also-`unsafe` call further down.
            let mut sa: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            sa.sin_family = libc::AF_INET as libc::sa_family_t;
            sa.sin_addr.s_addr = u32::from_ne_bytes(v4.octets());
            let mut buf = [0u8; 256];
            // SAFETY: `sa` is a fully-initialized, correctly-sized
            // `sockaddr_in`; `buf` is passed with its exact length as the
            // output buffer bound, which `getnameinfo` respects.
            let rc = unsafe {
                libc::getnameinfo(
                    (&sa as *const libc::sockaddr_in).cast(),
                    std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                    buf.as_mut_ptr().cast(),
                    buf.len() as libc::socklen_t,
                    std::ptr::null_mut(),
                    0,
                    0,
                )
            };
            (rc, buf)
        }
        IpAddr::V6(v6) => {
            // SAFETY: same reasoning as the IPv4 branch's `sockaddr_in`
            // above — an all-zero `sockaddr_in6` is a valid initial value.
            let mut sa: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            sa.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sa.sin6_addr.s6_addr = v6.octets();
            let mut buf = [0u8; 256];
            // SAFETY: same reasoning as the IPv4 branch above, for `sockaddr_in6`.
            let rc = unsafe {
                libc::getnameinfo(
                    (&sa as *const libc::sockaddr_in6).cast(),
                    std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
                    buf.as_mut_ptr().cast(),
                    buf.len() as libc::socklen_t,
                    std::ptr::null_mut(),
                    0,
                    0,
                )
            };
            (rc, buf)
        }
    };

    let (rc, buf) = sockaddr;
    if rc != 0 {
        return host.to_string();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// A small, fixed set of accepted `-s`/`-t` date formats — not a full
/// GNU-date-like relative-time parser. Matches the same documented
/// simplification already made in `dmesg --since`/`--until` (see
/// `checklist/dmesg.md`); a fuller parser is a separate, larger effort.
fn parse_datetime(input: &str) -> Option<i64> {
    let input = input.trim();
    if let Some(t) = parse_ymd_hms(input) {
        return Some(t);
    }
    parse_ymd(input)
}

fn parse_ymd_hms(s: &str) -> Option<i64> {
    let (date, time) = s.split_once([' ', 'T'])?;
    let (y, m, d) = parse_ymd_parts(date)?;
    let mut parts = time.splitn(3, ':');
    let hh: i32 = parts.next()?.parse().ok()?;
    let mm: i32 = parts.next()?.parse().ok()?;
    let ss: i32 = parts.next().unwrap_or("0").parse().ok()?;
    to_epoch(y, m, d, hh, mm, ss)
}

fn parse_ymd(s: &str) -> Option<i64> {
    let (y, m, d) = parse_ymd_parts(s)?;
    to_epoch(y, m, d, 0, 0, 0)
}

fn parse_ymd_parts(s: &str) -> Option<(i32, i32, i32)> {
    let mut it = s.splitn(3, '-');
    let y: i32 = it.next()?.parse().ok()?;
    let m: i32 = it.next()?.parse().ok()?;
    let d: i32 = it.next()?.parse().ok()?;
    Some((y, m, d))
}

/// Converts a local calendar date/time to a Unix timestamp via
/// `mktime(3)`, matching how `last`'s own `-s`/`-t` interpret times
/// (as local time, same as the wtmp records themselves).
fn to_epoch(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> Option<i64> {
    // SAFETY: `libc::tm` (glibc) is integer fields plus a `tm_zone`
    // pointer; an all-zero bit pattern is valid for both, and every field
    // we rely on is explicitly assigned immediately below before `mktime`
    // (which fully repopulates the struct, including `tm_zone`) is called.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_hour = hour;
    tm.tm_min = minute;
    tm.tm_sec = second;
    tm.tm_isdst = -1;
    // SAFETY: `tm` has been populated with caller-supplied but
    // range-plausible calendar fields; `mktime` normalizes out-of-range
    // values itself and returns -1 on failure, which we don't specially
    // handle beyond passing it through (an obviously-invalid date will
    // just fail to match any real records, which is acceptable for a CLI
    // date filter).
    let epoch = unsafe { libc::mktime(&mut tm) };
    if epoch == -1 { None } else { Some(epoch as i64) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(rec_type: i16, pid: i32, user: &str, line: &str, host: &str, time: i64) -> Record {
        Record {
            rec_type,
            pid,
            line: line.to_string(),
            user: user.to_string(),
            host: host.to_string(),
            time,
        }
    }

    #[test]
    fn normal_login_logout_pair() {
        let records = vec![
            rec(USER_PROCESS, 100, "alice", "tty1", "local", 1000),
            rec(DEAD_PROCESS, 100, "", "tty1", "", 2000),
        ];
        let sessions = build_sessions(&records);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].user, "alice");
        assert!(matches!(sessions[0].end, End::Normal(2000)));
    }

    #[test]
    fn still_open_at_eof() {
        let records = vec![rec(USER_PROCESS, 100, "alice", "tty1", "local", 1000)];
        let sessions = build_sessions(&records);
        assert!(matches!(sessions[0].end, End::Open));
    }

    #[test]
    fn shutdown_closes_open_session_as_down() {
        let records = vec![
            rec(USER_PROCESS, 100, "alice", "tty1", "local", 1000),
            rec(RUN_LVL, 0, "shutdown", "~", "kernel", 2000),
        ];
        let sessions = build_sessions(&records);
        let alice = sessions.iter().find(|s| s.kind == Kind::User).unwrap();
        assert!(matches!(alice.end, End::Down(2000)));
        let shutdown = sessions.iter().find(|s| s.kind == Kind::Shutdown).unwrap();
        assert!(matches!(shutdown.end, End::Open));
    }

    #[test]
    fn boot_without_shutdown_marks_previous_session_crashed() {
        let records = vec![
            rec(BOOT_TIME, 0, "reboot", "~", "kernel-1", 500),
            rec(USER_PROCESS, 100, "alice", "tty1", "local", 1000),
            // No RUN_LVL shutdown marker before the next boot: crash.
            rec(BOOT_TIME, 0, "reboot", "~", "kernel-2", 2000),
        ];
        let sessions = build_sessions(&records);
        let alice = sessions.iter().find(|s| s.kind == Kind::User).unwrap();
        assert!(matches!(alice.end, End::Crash(2000)));
        let first_boot = &sessions[0];
        assert!(matches!(first_boot.end, End::Crash(2000)));
    }

    #[test]
    fn clean_shutdown_then_boot_closes_reboot_and_shutdown_normally() {
        let records = vec![
            rec(BOOT_TIME, 0, "reboot", "~", "kernel-1", 500),
            rec(RUN_LVL, 0, "shutdown", "~", "kernel-1", 1500),
            rec(BOOT_TIME, 0, "reboot", "~", "kernel-2", 2000),
        ];
        let sessions = build_sessions(&records);
        assert!(matches!(sessions[0].end, End::Normal(1500))); // reboot #1
        assert!(matches!(sessions[1].end, End::Normal(2000))); // shutdown
        assert!(matches!(sessions[2].end, End::Open)); // reboot #2, still running
    }

    #[test]
    fn duration_formats_days_when_present() {
        assert_eq!(format_duration(3600 * 5 + 60 * 24), "05:24");
        assert_eq!(format_duration(86_400 * 16 + 3600 * 14 + 60 * 32), "16+14:32");
    }

    #[test]
    fn truncate_host_adds_marker_when_over_width() {
        assert_eq!(truncate_host("local"), "local           ");
        assert_eq!(truncate_host("7.0.13-200.fc44.x86_64"), "7.0.13-200.fc44*");
    }

    #[test]
    fn matches_filters_accepts_username_or_bare_tty_number() {
        let s = Session {
            kind: Kind::User,
            user: "alice".to_string(),
            line: "tty2".to_string(),
            host: "local".to_string(),
            start: 0,
            end: End::Open,
        };
        assert!(matches_filters(&s, &["alice".to_string()]));
        assert!(matches_filters(&s, &["tty2".to_string()]));
        assert!(matches_filters(&s, &["2".to_string()]));
        assert!(!matches_filters(&s, &["bob".to_string()]));
    }

    #[test]
    fn parse_ymd_and_ymd_hms() {
        assert!(parse_datetime("2026-07-20").is_some());
        assert!(parse_datetime("2026-07-20 13:00:00").is_some());
        assert!(parse_datetime("2026-07-20T13:00").is_some());
        assert!(parse_datetime("not a date").is_none());
    }
}
