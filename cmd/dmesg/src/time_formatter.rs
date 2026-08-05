//! Timestamp formatting for `dmesg` records. Every kmsg record carries a
//! monotonic microsecond timestamp *since boot*; these helpers convert
//! that into the handful of display formats `dmesg --time-format` offers.
//!
//! Every formatter that needs wall-clock time takes the system boot time
//! as an explicit parameter rather than reading it globally, so the
//! formatting logic itself is fully unit-testable without touching
//! `/proc/stat`.
use chrono::{DateTime, FixedOffset, TimeDelta};

/// `[   12.345678]`-style raw seconds-since-boot, right-aligned to match
/// util-linux's column width.
pub fn raw(timestamp_us: i64) -> String {
    let seconds = timestamp_us / 1_000_000;
    let sub_seconds = timestamp_us.rem_euclid(1_000_000);
    format!("{seconds:>5}.{sub_seconds:0>6}")
}

/// Wall-clock time in `ctime(3)`-style (`Mon Nov 18 19:34:12 2024`).
pub fn ctime(boot: DateTime<FixedOffset>, timestamp_us: i64) -> String {
    datetime_from_us(boot, timestamp_us)
        .format("%a %b %d %H:%M:%S %Y")
        .to_string()
}

/// Wall-clock time in ISO-8601-with-microseconds
/// (`2024-11-18T19:34:12,866807+07:00`).
pub fn iso(boot: DateTime<FixedOffset>, timestamp_us: i64) -> String {
    datetime_from_us(boot, timestamp_us)
        .format("%Y-%m-%dT%H:%M:%S,%6f%:z")
        .to_string()
}

/// Convert a boot-relative microsecond timestamp to an absolute
/// timestamp, given `boot`.
pub fn datetime_from_us(boot: DateTime<FixedOffset>, timestamp_us: i64) -> DateTime<FixedOffset> {
    boot.checked_add_signed(TimeDelta::microseconds(timestamp_us))
        .expect("timestamp_us out of range for DateTime arithmetic")
}

/// State shared by [`ReltimeFormatter`] and [`DeltaFormatter`]: both
/// print the raw boot-relative time for the very first record, then
/// switch to a delta against the previous record from then on.
enum State {
    Initial,
    AfterFirst,
    Delta,
}

/// `--time-format=reltime`: absolute `MonDD HH:MM` when the wall-clock
/// minute changes, otherwise a `+`/`-` delta against the previous line.
pub struct ReltimeFormatter {
    boot: DateTime<FixedOffset>,
    state: State,
    prev_timestamp_us: i64,
    previous_unix_timestamp: i64,
}

impl ReltimeFormatter {
    pub fn new(boot: DateTime<FixedOffset>) -> Self {
        ReltimeFormatter {
            boot,
            state: State::Initial,
            prev_timestamp_us: 0,
            previous_unix_timestamp: 0,
        }
    }

    pub fn format(&mut self, timestamp_us: i64) -> String {
        let date_time = datetime_from_us(self.boot, timestamp_us);
        let unix_timestamp = date_time.timestamp();
        let minute_changed = (unix_timestamp / 60) != (self.previous_unix_timestamp / 60);
        let result = match self.state {
            State::Initial => date_time.format("%b%d %H:%M").to_string(),
            _ if minute_changed => date_time.format("%b%d %H:%M").to_string(),
            State::AfterFirst => delta(0),
            State::Delta => delta(timestamp_us - self.prev_timestamp_us),
        };
        self.prev_timestamp_us = timestamp_us;
        self.previous_unix_timestamp = unix_timestamp;
        self.state = match self.state {
            State::Initial if timestamp_us == 0 => State::AfterFirst,
            _ => State::Delta,
        };
        result
    }
}

/// `--time-format=delta`: `<+seconds.micros>` against the previous line.
pub struct DeltaFormatter {
    state: State,
    prev_timestamp_us: i64,
}

impl DeltaFormatter {
    pub fn new() -> Self {
        DeltaFormatter {
            state: State::Initial,
            prev_timestamp_us: 0,
        }
    }

    pub fn format(&mut self, timestamp_us: i64) -> String {
        let result = match self.state {
            State::Delta => delta_bracketed(timestamp_us - self.prev_timestamp_us),
            _ => delta_bracketed(0),
        };
        self.prev_timestamp_us = timestamp_us;
        self.state = match self.state {
            State::Initial if timestamp_us == 0 => State::AfterFirst,
            _ => State::Delta,
        };
        result
    }
}

impl Default for DeltaFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// `+12.345678` / `-0.166667`, right-aligned to 11 columns (used by
/// `reltime`).
fn delta(delta_us: i64) -> String {
    let seconds = (delta_us / 1_000_000).abs();
    let sub_seconds = (delta_us % 1_000_000).abs();
    let sign = if delta_us >= 0 { '+' } else { '-' };
    let res = format!("{sign}{seconds}.{sub_seconds:0>6}");
    format!("{res:>11}")
}

/// `<   12.345678>` / `<  -0.166667>` (used by `delta`).
fn delta_bracketed(delta_us: i64) -> String {
    let seconds = (delta_us / 1_000_000).abs();
    let sub_seconds = (delta_us % 1_000_000).abs();
    let mut res = format!("{seconds}.{sub_seconds:0>6}");
    if delta_us < 0 {
        res.insert(0, '-');
    }
    format!("<{res:>12}>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    /// The exact boot time the upstream uutils test fixtures were
    /// generated against (their `fixed-boot-time` cargo feature), so we
    /// can assert byte-for-byte against real expected output.
    fn fixture_boot() -> DateTime<FixedOffset> {
        let date = NaiveDate::from_ymd_opt(2024, 11, 18).unwrap();
        let time = NaiveTime::from_hms_micro_opt(19, 34, 12, 866807).unwrap();
        let tz = FixedOffset::east_opt(7 * 3600).unwrap();
        date.and_time(time).and_local_timezone(tz).unwrap()
    }

    #[test]
    fn raw_matches_fixture() {
        assert_eq!(raw(0), "    0.000000");
        assert_eq!(raw(500_000), "    0.500000");
        assert_eq!(raw(333_333), "    0.333333");
        assert_eq!(raw(1_000_000), "    1.000000");
        assert_eq!(raw(48_000_000), "   48.000000");
    }

    #[test]
    fn ctime_matches_fixture() {
        let boot = fixture_boot();
        assert_eq!(ctime(boot, 0), "Mon Nov 18 19:34:12 2024");
        assert_eq!(ctime(boot, 500_000), "Mon Nov 18 19:34:13 2024");
        assert_eq!(ctime(boot, 333_333), "Mon Nov 18 19:34:13 2024");
        assert_eq!(ctime(boot, 1_000_000), "Mon Nov 18 19:34:13 2024");
        assert_eq!(ctime(boot, 48_000_000), "Mon Nov 18 19:35:00 2024");
    }

    #[test]
    fn iso_matches_fixture() {
        let boot = fixture_boot();
        assert_eq!(iso(boot, 0), "2024-11-18T19:34:12,866807+07:00");
        assert_eq!(iso(boot, 500_000), "2024-11-18T19:34:13,366807+07:00");
        assert_eq!(iso(boot, 333_333), "2024-11-18T19:34:13,200140+07:00");
        assert_eq!(iso(boot, 1_000_000), "2024-11-18T19:34:13,866807+07:00");
        assert_eq!(iso(boot, 48_000_000), "2024-11-18T19:35:00,866807+07:00");
    }

    #[test]
    fn delta_formatter_matches_fixture() {
        let mut f = DeltaFormatter::new();
        assert_eq!(f.format(0), "<    0.000000>");
        assert_eq!(f.format(500_000), "<    0.000000>");
        assert_eq!(f.format(333_333), "<   -0.166667>");
        assert_eq!(f.format(1_000_000), "<    0.666667>");
        assert_eq!(f.format(48_000_000), "<   47.000000>");
    }

    #[test]
    fn reltime_formatter_matches_fixture() {
        let boot = fixture_boot();
        let mut f = ReltimeFormatter::new(boot);
        assert_eq!(f.format(0), "Nov18 19:34");
        assert_eq!(f.format(500_000), "  +0.000000");
        assert_eq!(f.format(333_333), "  -0.166667");
        assert_eq!(f.format(1_000_000), "  +0.666667");
        assert_eq!(f.format(48_000_000), "Nov18 19:35");
    }
}
