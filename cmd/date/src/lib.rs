//! user date — print or set the system date and time.
//!
//! FORMAT goes through the C library's strftime, with GNU's additions (`%N`, `%:z`,
//! `%::z`, `%q`, `%P`, and the `-` `_` `0` `^` `#` flags and widths). `-d STRING` takes
//! what scripts give GNU date: `@SECONDS`, ISO 8601 dates and times (with `T`, a zone
//! and fractions), `MM/DD/YYYY`, month names, `HH:MM[:SS]` with am/pm, `now`, `today`,
//! `yesterday`, `tomorrow`, `noon`, `midnight`, weekday names, and relative items
//! (`+1 day`, `2 weeks ago`, `next month`, `last year`).
use std::ffi::CString;
use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use usercore::Ui;

pub fn run() -> i32 {
    let ui = Ui::new("date");
    let mut utc = false;
    let mut format: Option<String> = None;
    let mut date_str: Option<String> = None;
    let mut date_file: Option<String> = None;
    let mut file_ref: Option<String> = None;
    let mut set: Option<String> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let need = |i: &mut usize, o: &str| -> Option<String> {
        *i += 1;
        let v = args.get(*i).cloned();
        if v.is_none() {
            eprintln!("date: option requires an argument -- '{o}'");
        }
        v
    };
    while i < args.len() {
        let a = args[i].clone();
        match a.as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("date (user_utils) 0.2.0");
                return 0;
            }
            "-u" | "--utc" | "--universal" => utc = true,
            "-d" | "--date" => match need(&mut i, "d") {
                Some(v) => date_str = Some(v),
                None => return 1,
            },
            "-f" | "--file" => match need(&mut i, "f") {
                Some(v) => date_file = Some(v),
                None => return 1,
            },
            "-r" | "--reference" => match need(&mut i, "r") {
                Some(v) => file_ref = Some(v),
                None => return 1,
            },
            "-s" | "--set" => match need(&mut i, "s") {
                Some(v) => set = Some(v),
                None => return 1,
            },
            "-R" | "--rfc-email" | "--rfc-2822" => format = Some("%a, %d %b %Y %H:%M:%S %z".into()),
            "--debug" => {}
            s if s.starts_with("--date=") => date_str = Some(s[7..].to_string()),
            s if s.starts_with("--file=") => date_file = Some(s[7..].to_string()),
            s if s.starts_with("--reference=") => file_ref = Some(s[12..].to_string()),
            s if s.starts_with("--set=") => set = Some(s[6..].to_string()),
            s if s == "-I" || s.starts_with("--iso-8601") || (s.starts_with("-I") && s.len() > 2) => {
                let spec = s.strip_prefix("--iso-8601").map(|r| r.trim_start_matches('=')).unwrap_or(&s[2..]);
                format = Some(match spec {
                    "" | "date" => "%Y-%m-%d",
                    "hours" => "%Y-%m-%dT%H%:z",
                    "minutes" => "%Y-%m-%dT%H:%M%:z",
                    "seconds" => "%Y-%m-%dT%H:%M:%S%:z",
                    "ns" => "%Y-%m-%dT%H:%M:%S,%N%:z",
                    other => {
                        ui.err(&format!("invalid argument '{other}' for '--iso-8601'"));
                        return 1;
                    }
                }
                .into());
            }
            s if s.starts_with("--rfc-3339=") => {
                format = Some(match &s[11..] {
                    "date" => "%Y-%m-%d",
                    "seconds" => "%Y-%m-%d %H:%M:%S%:z",
                    "ns" => "%Y-%m-%d %H:%M:%S.%N%:z",
                    other => {
                        ui.err(&format!("invalid argument '{other}' for '--rfc-3339'"));
                        return 1;
                    }
                }
                .into());
            }
            s if s.starts_with('+') => format = Some(s[1..].to_string()),
            s if s.starts_with('-') && s.len() > 1 => {
                // bundled short options: -ud, -Ru
                let mut ok = true;
                for c in s[1..].chars() {
                    match c {
                        'u' => utc = true,
                        'R' => format = Some("%a, %d %b %Y %H:%M:%S %z".into()),
                        _ => ok = false,
                    }
                }
                if !ok {
                    ui.err(&format!("invalid option -- '{}'", &s[1..]));
                    return 1;
                }
            }
            // MMDDhhmm[[CC]YY][.ss]: set the clock
            other => set = Some(other.to_string()),
        }
        i += 1;
    }
    if utc {
        // SAFETY: single-threaded here; TZ is read by the time functions below.
        unsafe { std::env::set_var("TZ", "UTC0") };
    }
    let fmt = format.unwrap_or_else(|| "%a %b %e %H:%M:%S %Z %Y".into());
    let (now, now_ns) = SystemTime::now().duration_since(UNIX_EPOCH).map_or((0, 0), |d| (d.as_secs() as i64, d.subsec_nanos()));
    let mut out = io::stdout().lock();

    if let Some(path) = date_file {
        let reader: Box<dyn BufRead> = if path == "-" {
            Box::new(io::BufReader::new(io::stdin()))
        } else {
            match std::fs::File::open(&path) {
                Ok(f) => Box::new(io::BufReader::new(f)),
                Err(e) => {
                    ui.err(&format!("{path}: {e}"));
                    return 1;
                }
            }
        };
        let mut status = 0;
        for line in reader.lines().map_while(Result::ok) {
            match parse_date(&line, now, now_ns, utc) {
                Some((t, ns)) => {
                    let _ = writeln!(out, "{}", format_time_ns(t, ns, &fmt, utc));
                }
                None => {
                    ui.err(&format!("invalid date '{line}'"));
                    status = 1;
                }
            }
        }
        return status;
    }

    let (secs, nsec) = if let Some(f) = &file_ref {
        match std::fs::metadata(f) {
            Ok(m) => {
                use std::os::unix::fs::MetadataExt;
                (m.mtime(), m.mtime_nsec() as u32)
            }
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                return 1;
            }
        }
    } else if let Some(d) = &date_str {
        match parse_date(d, now, now_ns, utc) {
            Some(v) => v,
            None => {
                ui.err(&format!("invalid date '{d}'"));
                return 1;
            }
        }
    } else if let Some(s) = &set {
        let parsed = parse_date(s, now, now_ns, utc).or_else(|| parse_posix_set(s, now, utc));
        let Some((t, ns)) = parsed else {
            ui.err(&format!("invalid date '{s}'"));
            return 1;
        };
        let ts = libc::timespec { tv_sec: t as libc::time_t, tv_nsec: ns as _ };
        // SAFETY: ts is a valid timespec.
        if unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) } != 0 {
            ui.err(&format!("cannot set date: {}", io::Error::last_os_error()));
            let _ = writeln!(out, "{}", format_time_ns(t, ns, &fmt, utc));
            return 1;
        }
        (t, ns)
    } else {
        (now, now_ns)
    };
    let _ = writeln!(out, "{}", format_time_ns(secs, nsec, &fmt, utc));
    0
}

fn print_help() {
    print!(
        "Usage: date [OPTION]... [+FORMAT]\n\
 or:  date [-u|--utc|--universal] [MMDDhhmm[[CC]YY][.ss]]\n\
 Display date and time in the given FORMAT, or set the system date.\n\n\
 -d, --date=STRING          display time described by STRING, not 'now'\n\
 -f, --file=DATEFILE        like --date; once for each line of DATEFILE\n\
 -I[FMT], --iso-8601[=FMT]  ISO 8601 (date, hours, minutes, seconds, ns)\n\
 -R, --rfc-email            RFC 5322 format\n\
     --rfc-3339=FMT         RFC 3339 (date, seconds, ns)\n\
 -r, --reference=FILE       display the last modification time of FILE\n\
 -s, --set=STRING           set time described by STRING\n\
 -u, --utc, --universal     print or set Coordinated Universal Time (UTC)\n"
    );
}

/// For the tests and callers without nanoseconds.
#[cfg(test)]
fn format_time(secs: i64, fmt: &str, utc: bool) -> String {
    format_time_ns(secs, 0, fmt, utc)
}

fn broken_down(secs: i64, utc: bool) -> libc::tm {
    let t = secs as libc::time_t;
    // SAFETY: tm is plain old data, filled in by gmtime_r/localtime_r before use.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if utc {
            libc::gmtime_r(&t, &mut tm);
        } else {
            libc::localtime_r(&t, &mut tm);
        }
        tm
    }
}

fn c_strftime(spec: &str, tm: &libc::tm) -> String {
    let Ok(f) = CString::new(spec) else { return String::new() };
    let mut buf = [0u8; 512];
    // SAFETY: buf has buf.len() bytes; f is NUL-terminated; tm is initialised.
    let n = unsafe { libc::strftime(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), f.as_ptr(), tm) };
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn utc_offset(tm: &libc::tm, utc: bool) -> i64 {
    if utc { 0 } else { tm.tm_gmtoff as i64 }
}

/// FORMAT for `secs`.`nsec`, as GNU date writes it.
fn format_time_ns(secs: i64, nsec: u32, fmt: &str, utc: bool) -> String {
    let tm = broken_down(secs, utc);
    let mut out = String::new();
    let b: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] != '%' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        let mut flag: Option<char> = None;
        while i < b.len() && "-_0^#".contains(b[i]) {
            flag = Some(b[i]);
            i += 1;
        }
        let mut width = String::new();
        while i < b.len() && b[i].is_ascii_digit() {
            width.push(b[i]);
            i += 1;
        }
        let mut colons = 0;
        while i < b.len() && b[i] == ':' {
            colons += 1;
            i += 1;
        }
        let Some(&c) = b.get(i) else { break };
        i += 1;
        let off = utc_offset(&tm, utc);
        let (sign, a) = if off < 0 { ('-', -off) } else { ('+', off) };
        let mut val = match c {
            'N' => {
                let w = width.parse::<usize>().unwrap_or(9).clamp(1, 9);
                width.clear();
                format!("{nsec:09}")[..w].to_string()
            }
            'z' => match colons {
                0 => format!("{sign}{:02}{:02}", a / 3600, a / 60 % 60),
                1 => format!("{sign}{:02}:{:02}", a / 3600, a / 60 % 60),
                2 => format!("{sign}{:02}:{:02}:{:02}", a / 3600, a / 60 % 60, a % 60),
                _ if a % 3600 == 0 => format!("{sign}{:02}", a / 3600),
                _ => format!("{sign}{:02}:{:02}", a / 3600, a / 60 % 60),
            },
            'Z' if utc => "UTC".into(),
            'q' => ((tm.tm_mon / 3) + 1).to_string(),
            'P' => c_strftime("%p", &tm).to_lowercase(),
            's' => secs.to_string(),
            '%' => "%".into(),
            'n' => "\n".into(),
            't' => "\t".into(),
            other => c_strftime(&format!("%{other}"), &tm),
        };
        // GNU flags and widths on top of what strftime gave
        let numeric = val.chars().all(|ch| ch.is_ascii_digit() || ch == ' ' || ch == '-' || ch == '+');
        match flag {
            Some('-') if numeric => {
                let t = val.trim_start_matches([' ', '0']);
                val = if t.is_empty() { "0".into() } else { t.to_string() };
            }
            Some('_') if numeric => {
                let digits = val.trim_start_matches([' ', '0']);
                let d = if digits.is_empty() { "0" } else { digits };
                val = format!("{d:>w$}", w = val.len());
            }
            Some('0') if numeric => val = val.replace(' ', "0"),
            Some('^') => val = val.to_uppercase(),
            Some('#') => {
                val = if val.chars().any(|ch| ch.is_lowercase()) { val.to_uppercase() } else { val.to_lowercase() };
            }
            _ => {}
        }
        if let Ok(w) = width.parse::<usize>() {
            if val.len() < w {
                let pad = if flag == Some('_') || !numeric { ' ' } else { '0' };
                val = format!("{}{val}", pad.to_string().repeat(w - val.len()));
            }
        }
        out.push_str(&val);
    }
    out
}

/// `MMDDhhmm[[CC]YY][.ss]` for setting the clock.
fn parse_posix_set(s: &str, now: i64, utc: bool) -> Option<(i64, u32)> {
    let (main, ss) = match s.split_once('.') {
        Some((m, ss)) => (m, ss.parse::<i32>().ok()?),
        None => (s, 0),
    };
    if !main.bytes().all(|c| c.is_ascii_digit()) || !matches!(main.len(), 8 | 10 | 12) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| main[r].parse::<i32>().ok();
    let mut tm = broken_down(now, utc);
    tm.tm_mon = n(0..2)? - 1;
    tm.tm_mday = n(2..4)?;
    tm.tm_hour = n(4..6)?;
    tm.tm_min = n(6..8)?;
    tm.tm_sec = ss;
    match main.len() {
        10 => {
            let yy = n(8..10)?;
            tm.tm_year = if yy < 69 { yy + 100 } else { yy };
        }
        12 => tm.tm_year = n(8..12)? - 1900,
        _ => {}
    }
    to_epoch(&mut tm, utc, None).map(|t| (t, 0))
}

/// tm (normalised: day 32 is the next month's 1st) to seconds, in UTC, the local zone,
/// or with an explicit offset east of UTC.
fn to_epoch(tm: &mut libc::tm, utc: bool, offset: Option<i64>) -> Option<i64> {
    tm.tm_isdst = -1;
    // SAFETY: tm is a valid, initialised struct tm.
    let t = unsafe {
        if utc || offset.is_some() {
            libc::timegm(tm)
        } else {
            libc::mktime(tm)
        }
    };
    if t == -1 && !(tm.tm_year == 69 && tm.tm_mon == 11 && tm.tm_mday == 31) {
        return None;
    }
    Some(t as i64 - offset.unwrap_or(0))
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

fn month_of(w: &str) -> Option<i32> {
    let w = w.trim_end_matches('.');
    if w.len() < 3 {
        return None;
    }
    MONTHS.iter().position(|m| w.starts_with(m) && (w.len() == 3 || "january february march april may june july august september october november december sept".split(' ').any(|full| full.starts_with(w)))).map(|i| i as i32)
}

fn day_of(w: &str) -> Option<i32> {
    let w = w.trim_end_matches([',', '.']);
    if w.len() < 3 {
        return None;
    }
    DAYS.iter().position(|d| w.starts_with(d) && "sunday monday tuesday wednesday thursday friday saturday tues wednes thur thurs".split(' ').any(|full| full.starts_with(w))).map(|i| i as i32)
}

/// Seconds (or months, for month/year) in one relative unit.
fn unit_of(w: &str) -> Option<(bool, i64)> {
    let w = w.trim_end_matches('s');
    Some(match w {
        "year" => (true, 12),
        "month" => (true, 1),
        "fortnight" => (false, 14 * 86400),
        "week" => (false, 7 * 86400),
        "day" => (false, 86400),
        "hour" => (false, 3600),
        "minute" | "min" => (false, 60),
        "second" | "sec" => (false, 1),
        _ => return None,
    })
}

/// A `-d` STRING: seconds and nanoseconds since the epoch.
pub fn parse_date(input: &str, now: i64, now_ns: u32, utc: bool) -> Option<(i64, u32)> {
    let s = input.trim().to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix('@') {
        let (sec, frac) = match rest.split_once('.') {
            Some((a, f)) => (a, f),
            None => (rest, ""),
        };
        let secs: i64 = sec.parse().ok()?;
        let ns = if frac.is_empty() { 0 } else { format!("{frac:0<9}")[..9].parse().ok()? };
        return Some((secs, ns));
    }

    let mut tm = broken_down(now, utc);
    let mut have_date = false;
    let mut have_time = false;
    let mut nsec = now_ns;
    let mut offset: Option<i64> = None;
    let mut rel_months = 0i64;
    let mut rel_secs = 0i64;
    let mut weekday: Option<(i32, i64)> = None;

    // ISO 8601 "T" between date and time, and "," / "Z" glued on
    let s = s.replace(',', " ");
    let mut tokens: Vec<String> = Vec::new();
    for w in s.split_whitespace() {
        // 2020-01-01t10:00:00+02:00 → date, time
        if let Some(pos) = w.find('t').filter(|p| *p == 10 && w.as_bytes()[4] == b'-') {
            tokens.push(w[..pos].to_string());
            tokens.push(w[pos + 1..].to_string());
        } else {
            tokens.push(w.to_string());
        }
    }

    let set_time = |tm: &mut libc::tm, h: i32, m: i32, sec: i32| {
        tm.tm_hour = h;
        tm.tm_min = m;
        tm.tm_sec = sec;
    };
    let mut i = 0;
    let mut pending_sign: Option<i64> = None;
    while i < tokens.len() {
        let w = tokens[i].clone();
        let next = tokens.get(i + 1).cloned();
        i += 1;
        let num = w.parse::<i64>().ok();
        // YYYY-MM-DD
        let dash: Vec<&str> = w.split('-').collect();
        if dash.len() == 3 && dash.iter().all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())) {
            tm.tm_year = dash[0].parse::<i32>().ok()? - 1900;
            tm.tm_mon = dash[1].parse::<i32>().ok()? - 1;
            tm.tm_mday = dash[2].parse().ok()?;
            have_date = true;
            continue;
        }
        // MM/DD/YYYY, MM/DD
        let slash: Vec<&str> = w.split('/').collect();
        if (slash.len() == 2 || slash.len() == 3) && slash.iter().all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())) {
            tm.tm_mon = slash[0].parse::<i32>().ok()? - 1;
            tm.tm_mday = slash[1].parse().ok()?;
            if let Some(y) = slash.get(2) {
                let y: i32 = y.parse().ok()?;
                tm.tm_year = if y < 100 { if y < 69 { y + 100 } else { y } } else { y - 1900 };
            }
            have_date = true;
            continue;
        }
        // HH:MM[:SS[.frac]][zone][am|pm]
        if w.contains(':') && w.as_bytes()[0].is_ascii_digit() {
            let mut t = w.as_str();
            let mut ampm: Option<bool> = None;
            for (suf, pm) in [("am", false), ("pm", true), ("a.m.", false), ("p.m.", true)] {
                if let Some(x) = t.strip_suffix(suf) {
                    t = x;
                    ampm = Some(pm);
                }
            }
            if let Some(x) = t.strip_suffix('z') {
                t = x;
                offset = Some(0);
            }
            let zone_at = t.find(['+', '-']);
            if let Some(z) = zone_at {
                offset = Some(parse_offset(&t[z..])?);
                t = &t[..z];
            }
            let parts: Vec<&str> = t.split(':').collect();
            let h: i32 = parts.first()?.parse().ok()?;
            let m: i32 = parts.get(1)?.parse().ok()?;
            let (sec, frac) = match parts.get(2) {
                Some(sx) => match sx.split_once('.') {
                    Some((a, f)) => (a.parse().ok()?, format!("{f:0<9}")[..9].parse().ok()?),
                    None => (sx.parse().ok()?, 0),
                },
                None => (0, 0),
            };
            let h = match ampm {
                Some(true) if h < 12 => h + 12,
                Some(false) if h == 12 => 0,
                _ => h,
            };
            set_time(&mut tm, h, m, sec);
            nsec = frac;
            have_time = true;
            if next.as_deref().is_some_and(|n| n == "am" || n == "pm") {
                let pm = next.as_deref() == Some("pm");
                if pm && tm.tm_hour < 12 {
                    tm.tm_hour += 12;
                } else if !pm && tm.tm_hour == 12 {
                    tm.tm_hour = 0;
                }
                i += 1;
            }
            continue;
        }
        // 3pm, 11am
        if let Some(h) = w.strip_suffix("pm").or_else(|| w.strip_suffix("am")).and_then(|h| h.parse::<i32>().ok()) {
            let pm = w.ends_with("pm");
            set_time(&mut tm, if pm && h < 12 { h + 12 } else if !pm && h == 12 { 0 } else { h }, 0, 0);
            nsec = 0;
            have_time = true;
            continue;
        }
        match w.as_str() {
            "now" => continue,
            "today" => continue,
            "yesterday" => {
                rel_secs -= 86400;
                continue;
            }
            "tomorrow" => {
                rel_secs += 86400;
                continue;
            }
            "noon" => {
                set_time(&mut tm, 12, 0, 0);
                nsec = 0;
                have_time = true;
                continue;
            }
            "midnight" => {
                set_time(&mut tm, 0, 0, 0);
                nsec = 0;
                have_time = true;
                continue;
            }
            "utc" | "gmt" | "z" | "ut" => {
                offset = Some(0);
                continue;
            }
            "ago" => {
                rel_months = -rel_months;
                rel_secs = -rel_secs;
                continue;
            }
            "next" | "last" | "this" => {
                let n = match w.as_str() {
                    "next" => 1,
                    "last" => -1,
                    _ => 0,
                };
                let target = next.clone()?;
                i += 1;
                if let Some((months, size)) = unit_of(&target) {
                    if months { rel_months += n * size } else { rel_secs += n * size }
                } else if let Some(d) = day_of(&target) {
                    weekday = Some((d, n));
                } else {
                    return None;
                }
                continue;
            }
            "+" | "-" => {
                pending_sign = Some(if w == "+" { 1 } else { -1 });
                continue;
            }
            _ => {}
        }
        if w.starts_with(['+', '-']) && w.len() > 1 && w[1..].bytes().all(|c| c.is_ascii_digit()) && !have_time && offset.is_none() && next.as_deref().and_then(unit_of).is_none() && w.len() == 5 {
            // +0200 zone
            offset = Some(parse_offset(&w)?);
            continue;
        }
        if let Some(mut n) = num.or_else(|| w.strip_prefix('+').and_then(|x| x.parse().ok())) {
            if let Some(sign) = pending_sign.take() {
                n *= sign;
            }
            match next.as_deref().and_then(unit_of) {
                Some((months, size)) => {
                    if months { rel_months += n * size } else { rel_secs += n * size }
                    i += 1;
                }
                None if w.len() == 8 && !have_date && n > 0 => {
                    // YYYYMMDD
                    tm.tm_year = (n / 10000) as i32 - 1900;
                    tm.tm_mon = (n / 100 % 100) as i32 - 1;
                    tm.tm_mday = (n % 100) as i32;
                    have_date = true;
                }
                None if (1000..=9999).contains(&n) && have_date => tm.tm_year = n as i32 - 1900,
                None if (1..=31).contains(&n) && tokens.iter().any(|t| month_of(t).is_some()) => tm.tm_mday = n as i32,
                None if (1000..=9999).contains(&n) => tm.tm_year = n as i32 - 1900,
                None => return None,
            }
            continue;
        }
        if let Some(m) = month_of(&w) {
            tm.tm_mon = m;
            have_date = true;
            continue;
        }
        if let Some(d) = day_of(&w) {
            weekday = Some((d, 0));
            continue;
        }
        if let Some((months, size)) = unit_of(&w) {
            // "day" alone is "1 day"
            if months { rel_months += size } else { rel_secs += size }
            continue;
        }
        return None;
    }

    if have_date && !have_time {
        set_time(&mut tm, 0, 0, 0);
        nsec = 0;
    }
    if weekday.is_some() && !have_time {
        set_time(&mut tm, 0, 0, 0);
        nsec = 0;
    }
    tm.tm_year += (rel_months / 12) as i32;
    tm.tm_mon += (rel_months % 12) as i32;
    let mut t = to_epoch(&mut tm, utc, offset)?;
    if let Some((d, n)) = weekday {
        let cur = broken_down(t, utc || offset.is_some()).tm_wday;
        let mut diff = (d - cur).rem_euclid(7) as i64;
        if n > 0 && diff == 0 {
            diff = 7;
        }
        if n < 0 {
            diff -= 7;
        }
        t += diff * 86400;
    }
    Some((t + rel_secs, nsec))
}

/// `+HH:MM`, `+HHMM`, `+HH`: seconds east of UTC.
fn parse_offset(z: &str) -> Option<i64> {
    let sign = if z.starts_with('-') { -1 } else { 1 };
    let d: String = z[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    let (h, m) = match d.len() {
        1 | 2 => (d.parse::<i64>().ok()?, 0),
        3 | 4 => (d[..d.len() - 2].parse::<i64>().ok()?, d[d.len() - 2..].parse::<i64>().ok()?),
        _ => return None,
    };
    Some(sign * (h * 3600 + m * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_formats_as_1970_utc() {
        assert_eq!(format_time(0, "%Y-%m-%d", true), "1970-01-01");
        assert_eq!(format_time(0, "%T", true), "00:00:00");
        assert_eq!(format_time(0, "%Z", true), "UTC");
    }

    #[test]
    fn known_timestamp_formats_correctly_utc() {
        // 2024-01-15 12:34:56 UTC
        let secs = 1_705_322_096;
        assert_eq!(format_time(secs, "%F %T", true), "2024-01-15 12:34:56");
        assert_eq!(format_time(secs, "%Y", true), "2024");
    }

    #[test]
    fn literal_and_escape_sequences() {
        assert_eq!(format_time(0, "%%", true), "%");
        assert_eq!(format_time(0, "a%nb", true), "a\nb");
        assert_eq!(format_time(0, "a%tb", true), "a\tb");
        assert_eq!(format_time(0, "plain text", true), "plain text");
    }

    #[test]
    fn gnu_extensions() {
        assert_eq!(format_time(0, "%q", true), "1");
        assert_eq!(format_time(0, "%:z %N", true), "+00:00 000000000");
        assert_eq!(format_time(86400 * 5, "%-d|%_m|%^a", true), "6| 1|TUE");
    }

    #[test]
    fn parses_dates() {
        let p = |s: &str| parse_date(s, 1_700_000_000, 0, true).map(|(t, _)| t);
        assert_eq!(p("@0"), Some(0));
        assert_eq!(p("2020-02-29 12:34:56"), Some(1_582_979_696));
        assert_eq!(p("1970-01-02"), Some(86400));
        assert_eq!(p("2020-01-01 +1 day"), Some(1_577_923_200));
        assert_eq!(p("2020-01-31 1 month"), Some(1_583_107_200));
        assert_eq!(p("2020-01-01T10:00:00+02:00"), Some(1_577_865_600));
        assert_eq!(p("Jan 2 2020"), Some(1_577_923_200));
        assert_eq!(p("01/02/2020 3pm"), Some(1_577_977_200));
        assert_eq!(p("1970-01-01 2 hours ago"), Some(-7200));
        assert!(p("bogus").is_none());
    }

    #[test]
    fn weekday_and_month_names() {
        // 1970-01-01 was a Thursday.
        assert_eq!(format_time(0, "%a %b", true), "Thu Jan");
        assert_eq!(format_time(0, "%A %B", true), "Thursday January");
    }
}
