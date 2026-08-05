use std::time::Duration;

use crate::error::{PrioError, Result};

// -- Duration Parsing ---------------------------------------------------------

/// Parse a duration string with a suffix: `30s`, `10m`, `2h`.
/// Fractional values are **not** supported by design; keep the interface simple.
///
/// # Examples
/// ```
/// # use prio::utils::timebound::parse_duration;
/// assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
/// assert_eq!(parse_duration("10m").unwrap().as_secs(), 600);
/// assert_eq!(parse_duration("2h").unwrap().as_secs(), 7200);
/// ```
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();

    let (digits, mult) = if let Some(d) = s.strip_suffix('s') {
        (d, 1u64)
    } else if let Some(d) = s.strip_suffix('m') {
        (d, 60)
    } else if let Some(d) = s.strip_suffix('h') {
        (d, 3600)
    } else {
        return Err(PrioError::DurationParseError(s.to_string()));
    };

    let n: u64 = digits
        .parse()
        .map_err(|_| PrioError::DurationParseError(s.to_string()))?;

    if n == 0 {
        return Err(PrioError::DurationParseError(format!(
            "{}: duration must be > 0",
            s
        )));
    }

    Ok(Duration::from_secs(n * mult))
}

/// Format a [`Duration`] as a user-readable string (e.g. "30 minutes").
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..=59 => format!("{} second{}", secs, if secs == 1 { "" } else { "s" }),
        60..=3599 => {
            let m = secs / 60;
            format!("{} minute{}", m, if m == 1 { "" } else { "s" })
        }
        _ => {
            let h = secs / 3600;
            format!("{} hour{}", h, if h == 1 { "" } else { "s" })
        }
    }
}

// -- Time-Bound Priority Reset -------------------------------------------------

/// Spawn a background thread that sleeps for `duration`, then resets `pid`'s
/// niceness back to `original_nice`.
///
/// The thread is detached (fire-and-forget). If the process has already
/// exited when the timer fires, `setpriority` will silently fail — which is
/// the correct behaviour.
pub fn schedule_reset(pid: u32, original_nice: i32, duration: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        // SAFETY: `setpriority(2)` takes only plain integers, no
        // pointers involved. `pid as libc::id_t` is a lossless `u32` ->
        // `u32` widening on Linux, and `original_nice` is the caller's
        // previously-resolved nice value which was already validated
        // when it was first applied, so it lies within the kernel's
        // legal `[-20, 19]` range. If the process has since exited, the
        // call just fails with ESRCH, which is intentionally ignored.
        unsafe {
            // Ignore the return code: if the PID is gone, nothing to do.
            libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, original_nice);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_seconds() {
        assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
    }

    #[test]
    fn parse_minutes() {
        assert_eq!(parse_duration("10m").unwrap().as_secs(), 600);
    }

    #[test]
    fn parse_hours() {
        assert_eq!(parse_duration("2h").unwrap().as_secs(), 7200);
    }

    #[test]
    fn invalid_suffix() {
        assert!(parse_duration("10d").is_err());
        assert!(parse_duration("abc").is_err());
    }

    #[test]
    fn zero_rejected() {
        assert!(parse_duration("0m").is_err());
    }

    #[test]
    fn format_round_trip() {
        assert_eq!(format_duration(Duration::from_secs(30)), "30 seconds");
        assert_eq!(format_duration(Duration::from_secs(60)), "1 minute");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2 hours");
    }
}
