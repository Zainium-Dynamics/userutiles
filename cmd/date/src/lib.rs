//! user date — print or set the system date and time.
use std::ffi::CString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use usercore::Ui;

/// Entry point for the `date` utility. Parses `std::env::args()` and
/// prints the current time (or the mtime of `--reference=FILE`, or a
/// literal Unix timestamp given via `--date`) formatted per `+FORMAT`,
/// defaulting to `%a %b %e %H:%M:%S %Z %Y`.
///
/// Returns 0 on success, 1 on a usage or I/O error. Actually setting the
/// system clock is not implemented.
pub fn run() -> i32 {
    let ui = Ui::new("date");
    let mut utc = false;
    let mut format: Option<String> = None;
    let mut set_time: Option<String> = None;
    let mut file_ref: Option<String> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: date [OPTION]... [+FORMAT]\n\
 Display the current time in the given FORMAT, or set the system date.\n\n\
 -u, --utc, --universal print or set Coordinated Universal Time (UTC)\n\
 -d, --date=STRING display time described by STRING\n\
 -r, --reference=FILE display the last modification time of FILE\n\
 --help display this help and exit\n\
 --version output version information and exit\n\n\
 FORMAT controls the output. Interpreted sequences:\n\
 %%n newline %%t tab\n\
 %%Y year %%m month (01-12) %%d day\n\
 %%H hour %%M minute %%S second\n\
 %%s seconds since 1970-01-01\n\
 %%a abbreviated weekday name %%A full weekday\n\
 %%b abbreviated month name %%B full month\n\
 %%c locale date and time %%F full date (%%Y-%%m-%%d)\n\
 %%T time (%%H:%%M:%%S) %%R %%H:%%M\n"
                );
                return 0;
            }
            "--version" => {
                println!("date (user_utils) 0.1.0");
                return 0;
            }
            "-u" | "--utc" | "--universal" => utc = true,
            "-d" | "--date" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'd'");
                    return 1;
                }
                set_time = Some(args[i].clone()); // interpret as display date string
            }
            "-r" | "--reference" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'r'");
                    return 1;
                }
                file_ref = Some(args[i].clone());
            }
            s if s.starts_with('+') => format = Some(s[1..].to_string()),
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => {
                // try set date (requires privileges) — not implemented fully
                ui.err(&format!("invalid date '{other}'"));
                return 1;
            }
        }
        i += 1;
    }

    let secs = if let Some(ref f) = file_ref {
        match std::fs::metadata(f) {
            Ok(m) => {
                use std::os::unix::fs::MetadataExt;
                m.mtime() as i64
            }
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                return 1;
            }
        }
    } else if let Some(ref d) = set_time {
        // Minimal parse: unix timestamp or ISO-ish YYYY-MM-DD
        if let Ok(n) = d.parse::<i64>() {
            n
        } else {
            ui.err(&format!("invalid date '{d}'"));
            return 1;
        }
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    };

    let fmt = format.as_deref().unwrap_or("%a %b %e %H:%M:%S %Z %Y");
    let out = format_time(secs, fmt, utc);
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{out}");
    0
}

/// Format the Unix timestamp `secs` per `fmt`, a `strftime`-style pattern
/// using a subset of `%`-conversions (`%Y %m %d %H %M %S %s %F %T %R %a
/// %A %b %B %Z %c %n %t %%`). When `utc` is true, conversion uses UTC via
/// `gmtime_r`; otherwise the local timezone via `localtime_r`.
fn format_time(secs: i64, fmt: &str, utc: bool) -> String {
    // SAFETY: `libc::tm` has no `Default` impl (it's a plain-old-data FFI
    // struct: all `c_int`/`c_long` fields plus, on glibc, a `tm_gmtoff`
    // integer and a `tm_zone: *const c_char`). An all-zero bit pattern is
    // a valid value for every one of those field types (integers accept
    // zero, and a null raw pointer is valid — it is never dereferenced
    // unless explicitly read, which this code never does), so
    // `mem::zeroed` is sound here. The value is immediately fully
    // overwritten by `gmtime_r`/`localtime_r` below before any field is
    // used.
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let t = secs as libc::time_t;
    // SAFETY: `&t` and `&mut tm` are valid, non-null, properly aligned
    // references to a local `time_t` and `tm` for the duration of the
    // call. `gmtime_r`/`localtime_r` only read `*t` and write into `*tm`
    // (converting an out-of-range `t` to a best-effort/clamped `tm`
    // rather than causing UB), so this is sound regardless of `secs`.
    unsafe {
        if utc {
            libc::gmtime_r(&t, &mut tm);
        } else {
            libc::localtime_r(&t, &mut tm);
        }
    }

    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(spec) = chars.next() else {
            out.push('%');
            break;
        };
        match spec {
            '%' => out.push('%'),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'Y' => out.push_str(&format!("{:04}", tm.tm_year + 1900)),
            'y' => out.push_str(&format!("{:02}", (tm.tm_year + 1900) % 100)),
            'm' => out.push_str(&format!("{:02}", tm.tm_mon + 1)),
            'd' => out.push_str(&format!("{:02}", tm.tm_mday)),
            'e' => out.push_str(&format!("{:2}", tm.tm_mday)),
            'H' => out.push_str(&format!("{:02}", tm.tm_hour)),
            'M' => out.push_str(&format!("{:02}", tm.tm_min)),
            'S' => out.push_str(&format!("{:02}", tm.tm_sec)),
            's' => out.push_str(&format!("{secs}")),
            'F' => out.push_str(&format!(
                "{:04}-{:02}-{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday
            )),
            'T' => out.push_str(&format!(
                "{:02}:{:02}:{:02}",
                tm.tm_hour, tm.tm_min, tm.tm_sec
            )),
            'R' => out.push_str(&format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)),
            'a' => out.push_str(WEEKDAY_ABBR[tm.tm_wday.clamp(0, 6) as usize]),
            'A' => out.push_str(WEEKDAY_FULL[tm.tm_wday.clamp(0, 6) as usize]),
            'b' | 'h' => out.push_str(MONTH_ABBR[tm.tm_mon.clamp(0, 11) as usize]),
            'B' => out.push_str(MONTH_FULL[tm.tm_mon.clamp(0, 11) as usize]),
            'Z' => {
                if utc {
                    out.push_str("UTC");
                } else {
                    // best-effort zone via strftime
                    let mut buf = [0i8; 64];
                    // unwrap: "%Z" is a static literal with no interior
                    // NUL byte, so `CString::new` cannot fail here.
                    let cfmt = CString::new("%Z").expect("static format string has no NUL bytes");
                    // SAFETY: `buf` is a stack-allocated `[i8; 64]`, and
                    // `buf.len()` (64) is passed as the max size, so
                    // `strftime` cannot write past the buffer; on success
                    // it NUL-terminates within that bound, and on
                    // truncation/failure it writes 0 and leaves `buf`
                    // untouched (already zero-initialized), so
                    // `CStr::from_ptr` always finds a NUL within `buf`.
                    // `cfmt` is a live, NUL-terminated `CString`, and
                    // `&tm` is a valid, fully-initialized `libc::tm`
                    // (populated by `gmtime_r`/`localtime_r` above), so
                    // both FFI arguments are sound. `CStr::from_ptr` then
                    // borrows `buf` only for the immediate
                    // `to_string_lossy` copy, not past this block.
                    unsafe {
                        libc::strftime(buf.as_mut_ptr(), buf.len(), cfmt.as_ptr(), &tm);
                        let s = std::ffi::CStr::from_ptr(buf.as_ptr());
                        out.push_str(&s.to_string_lossy());
                    }
                }
            }
            'c' => {
                out.push_str(&format!(
                    "{} {} {:2} {:02}:{:02}:{:02} {} {:04}",
                    WEEKDAY_ABBR[tm.tm_wday.clamp(0, 6) as usize],
                    MONTH_ABBR[tm.tm_mon.clamp(0, 11) as usize],
                    tm.tm_mday,
                    tm.tm_hour,
                    tm.tm_min,
                    tm.tm_sec,
                    if utc { "UTC" } else { "" },
                    tm.tm_year + 1900
                ));
            }
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

const WEEKDAY_ABBR: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAY_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

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
    fn unknown_conversion_is_passed_through() {
        assert_eq!(format_time(0, "%q", true), "%q");
    }

    #[test]
    fn weekday_and_month_names() {
        // 1970-01-01 was a Thursday.
        assert_eq!(format_time(0, "%a %b", true), "Thu Jan");
        assert_eq!(format_time(0, "%A %B", true), "Thursday January");
    }
}
