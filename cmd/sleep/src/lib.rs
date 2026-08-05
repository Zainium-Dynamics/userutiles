//! user sleep — pause for a given amount of time.
use std::thread;
use std::time::Duration;

use usercore::Ui;

/// Entry point for the `sleep` utility. Parses `std::env::args()` as one
/// or more `NUMBER[SUFFIX]` durations (`s`econds, `m`inutes, `h`ours,
/// `d`ays; seconds if no suffix), sums them, and sleeps for the total.
///
/// Returns 0 after sleeping, 1 on a usage error (missing operand,
/// unparsable/negative/non-finite duration).
pub fn run() -> i32 {
    let ui = Ui::new("sleep");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        ui.err("missing operand");
        eprintln!("Try 'sleep --help' for more information.");
        return 1;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!(
            "Usage: sleep NUMBER[SUFFIX]...\nPause for NUMBER seconds. SUFFIX may be s m h d.\n"
        );
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("sleep (user_utils) 0.1.0");
        return 0;
    }

    let total = match total_duration_secs(&args) {
        Ok(t) => t,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    sleep_seconds(total);
    0
}

/// Sum the parsed duration (in seconds) of every argument in `args`.
/// Returns an error naming the first argument that fails to parse, or if
/// the total is negative or non-finite.
fn total_duration_secs(args: &[String]) -> Result<f64, String> {
    let mut total = 0.0f64;
    for a in args {
        total += parse_duration(a)?;
    }
    if total < 0.0 || !total.is_finite() {
        return Err("invalid time interval".to_string());
    }
    Ok(total)
}

/// Sleep for `total` seconds, chunked to 24h increments so pathologically
/// large values don't hand a single multi-year `Duration` to the OS.
fn sleep_seconds(total: f64) {
    let mut remaining = total;
    while remaining > 0.0 {
        let chunk = remaining.min(86400.0);
        thread::sleep(Duration::from_secs_f64(chunk));
        remaining -= chunk;
    }
}

/// Parse a single `NUMBER[SUFFIX]` duration argument into seconds.
/// `SUFFIX` is one of `s`/`m`/`h`/`d` (case-insensitive; seconds assumed
/// if absent). Negative numbers and anything that doesn't fully parse as
/// a non-negative `f64` are rejected.
fn parse_duration(s: &str) -> Result<f64, String> {
    if s.is_empty() {
        return Err(format!("invalid time interval '{s}'"));
    }
    let (num, mult) = match s.as_bytes().last().copied() {
        Some(b's') | Some(b'S') => (&s[..s.len() - 1], 1.0),
        Some(b'm') | Some(b'M') => (&s[..s.len() - 1], 60.0),
        Some(b'h') | Some(b'H') => (&s[..s.len() - 1], 3600.0),
        Some(b'd') | Some(b'D') => (&s[..s.len() - 1], 86400.0),
        Some(c) if c.is_ascii_digit() || c == b'.' => (s, 1.0),
        _ => return Err(format!("invalid time interval '{s}'")),
    };
    if num.is_empty() {
        return Err(format!("invalid time interval '{s}'"));
    }
    let v: f64 = num
        .parse()
        .map_err(|_| format!("invalid time interval '{s}'"))?;
    if v < 0.0 || !v.is_finite() {
        return Err(format!("invalid time interval '{s}'"));
    }
    Ok(v * mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_seconds() {
        assert_eq!(parse_duration("5").unwrap(), 5.0);
    }

    #[test]
    fn parse_seconds_suffix() {
        assert_eq!(parse_duration("5s").unwrap(), 5.0);
    }

    #[test]
    fn parse_minutes_suffix() {
        assert_eq!(parse_duration("2m").unwrap(), 120.0);
    }

    #[test]
    fn parse_hours_suffix() {
        assert_eq!(parse_duration("1h").unwrap(), 3600.0);
    }

    #[test]
    fn parse_days_suffix() {
        assert_eq!(parse_duration("1d").unwrap(), 86400.0);
    }

    #[test]
    fn parse_fractional_seconds() {
        assert_eq!(parse_duration("0.5").unwrap(), 0.5);
    }

    #[test]
    fn parse_uppercase_suffix() {
        assert_eq!(parse_duration("1H").unwrap(), 3600.0);
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn parse_rejects_negative() {
        assert!(parse_duration("-1").is_err());
    }

    #[test]
    fn parse_rejects_bare_suffix() {
        assert!(parse_duration("s").is_err());
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_duration("abc").is_err());
    }

    #[test]
    fn parse_rejects_infinite_and_nan() {
        assert!(parse_duration("inf").is_err());
        assert!(parse_duration("nan").is_err());
    }

    #[test]
    fn total_duration_sums_multiple_args() {
        let args = vec!["1".to_string(), "2s".to_string(), "1m".to_string()];
        assert_eq!(total_duration_secs(&args).unwrap(), 63.0);
    }

    #[test]
    fn total_duration_errors_on_bad_arg() {
        let args = vec!["1".to_string(), "bogus".to_string()];
        assert!(total_duration_secs(&args).is_err());
    }
}
