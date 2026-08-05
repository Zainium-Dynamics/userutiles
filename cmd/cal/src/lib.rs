//! user cal — display a calendar.
use std::io::IsTerminal;

use chrono::{Datelike, Local, Months, NaiveDate, Weekday};
use usercore::Ui;

/// How many months (and in what layout) to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayMode {
    ThreeMonths,
    Year,
    NMonths(u32),
}

/// Parsed, validated `cal` invocation.
#[derive(Debug, Clone)]
struct CalOptions {
    /// The month (day is normalized to 1) to anchor rendering at.
    date: NaiveDate,
    /// The exact day to visually highlight (only meaningful when a full
    /// `day month year` was given on the command line).
    highlight_date: NaiveDate,
    display_mode: DisplayMode,
    monday_first: bool,
    julian: bool,
    week_numbers: bool,
    color: bool,
}

const NUM_CALENDAR_LINES: usize = 8;
const NUM_SPACES_BETWEEN_CALENDARS: usize = 3;
const MAX_CALENDARS_SIDE_BY_SIDE: usize = 3;

const HELP: &str = "Usage: cal [options] [[[day] month] year]\n\
Display a calendar.\n\n\
  -3, --three          show previous, current and next month\n\
  -n, --months=NUMBER  show NUMBER months, starting with current month\n\
  -y, --year           show the whole current year\n\
  -Y, --twelve         show the next twelve months\n\
  -m, --monday         Monday as first day of week\n\
  -j, --julian         use day-of-year numbering\n\
  -w, --week           show ISO week numbers\n\
      --color[=WHEN]   colorize the output (WHEN: always, auto, never)\n\
  -h, --help           display this help and exit\n\
      --version        output version information and exit\n";

/// Entry point: parse `std::env::args()`, render the requested calendar to
/// stdout. Returns 0 on success, 1 on a bad argument (unknown option,
/// missing value, or a date that fails to parse).
pub fn run() -> i32 {
    let ui = Ui::new("cal");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("cal (user_utils) 0.1.0");
        return 0;
    }

    match parse_args(&args) {
        Ok(options) => {
            print!("{}", render(&options));
            0
        }
        Err(e) => {
            ui.err(&e);
            1
        }
    }
}

/// Parse `cal`'s options and positional date arguments (`[[[day] month]
/// year]`) out of `args` (already stripped of `argv[0]`, `--help`/
/// `--version` handled by the caller).
fn parse_args(args: &[String]) -> Result<CalOptions, String> {
    let mut year_flag = false;
    let mut three_flag = false;
    let mut twelve_flag = false;
    let mut months_flag: Option<u32> = None;
    let mut monday_first = false;
    let mut julian = false;
    let mut week_numbers = false;
    let mut color_opt: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-y" | "--year" => year_flag = true,
            "-3" | "--three" => three_flag = true,
            "-Y" | "--twelve" => twelve_flag = true,
            "-m" | "--monday" => monday_first = true,
            "-j" | "--julian" => julian = true,
            "-w" | "--week" => week_numbers = true,
            "-n" | "--months" => {
                i += 1;
                let v = args.get(i).ok_or("option '-n' requires an argument")?;
                months_flag =
                    Some(v.parse::<u32>().map_err(|_| format!("bad usage: invalid months value '{v}'"))?);
            }
            s if s.starts_with("--months=") => {
                let v = &s["--months=".len()..];
                months_flag =
                    Some(v.parse::<u32>().map_err(|_| format!("bad usage: invalid months value '{v}'"))?);
            }
            "--color" => color_opt = Some("auto".to_string()),
            s if s.starts_with("--color=") => color_opt = Some(s["--color=".len()..].to_string()),
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("invalid option -- '{s}'"));
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if positional.len() > 3 {
        return Err(format!("extra operand '{}'", positional[3]));
    }

    if [year_flag, twelve_flag, months_flag.is_some()].iter().filter(|&&set| set).count() > 1 {
        return Err("not all of -y, -Y, and -n may be used at once".to_string());
    }

    let now = Local::now().date_naive();
    let mut year_mode = false;
    let mut full_date_provided = false;

    let date = match positional.len() {
        0 => now,
        1 => {
            if positional[0].chars().all(|c| c.is_ascii_digit()) && !positional[0].is_empty() {
                year_mode = true;
                try_parse_date(&positional[0], "1", "1")?
            } else {
                try_parse_date(&now.year().to_string(), &positional[0], "1")?
            }
        }
        2 => try_parse_date(&positional[1], &positional[0], "1")?,
        3 => {
            full_date_provided = true;
            try_parse_date(&positional[2], &positional[1], &positional[0])?
        }
        _ => unreachable!(),
    };

    let highlight_date = if full_date_provided { date } else { now };

    let display_mode = if year_mode || year_flag {
        DisplayMode::Year
    } else if twelve_flag {
        DisplayMode::NMonths(12)
    } else if three_flag {
        DisplayMode::ThreeMonths
    } else if let Some(count) = months_flag {
        DisplayMode::NMonths(count.max(1))
    } else {
        DisplayMode::NMonths(1)
    };

    let color = match color_opt.as_deref().unwrap_or("auto") {
        "always" => true,
        "never" => false,
        "auto" => std::io::stdout().is_terminal(),
        other => return Err(format!("invalid color option '{other}'")),
    };

    Ok(CalOptions {
        date: NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
            .ok_or_else(|| "invalid date".to_string())?,
        highlight_date,
        display_mode,
        monday_first,
        julian,
        week_numbers,
        color,
    })
}

/// Parse a `year`/`month`/`day` triple (each a decimal string, or `month`
/// possibly an English month name) into a [`NaiveDate`]. Accepts either
/// `%Y-%m-%d` (numeric month) or `%Y-%B-%d` (full month name).
fn try_parse_date(year: &str, month: &str, day: &str) -> Result<NaiveDate, String> {
    let date_str = format!("{year}-{month}-{day}");
    let formats = ["%Y-%m-%d", "%Y-%B-%d"];

    for format in formats {
        if let Ok(date) = NaiveDate::parse_from_str(&date_str, format) {
            return Ok(date);
        }
    }

    Err(format!("invalid date: '{year} {month} {day}'"))
}

/// Compute (day-column width, single-month line width) for the given
/// options — julian day-of-year numbers need 3 columns, plain days need 2;
/// showing week numbers adds a 3-character gutter.
fn calculate_field_widths(options: &CalOptions) -> (usize, usize) {
    let day_width = if options.julian { 3 } else { 2 };
    let mut line_width = 7 * (day_width + 1) - 1;
    if options.week_numbers {
        line_width += 3;
    }
    (day_width, line_width)
}

/// US-style week number: the first Sunday-starting week of the year is
/// week 1 (days before it, if any, are considered week 0/52).
fn us_week_number(date: NaiveDate) -> i64 {
    let jan1 = NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap();
    let days_before = jan1.weekday().num_days_from_sunday();
    let first_sunday = jan1 - chrono::Duration::days(days_before as i64);
    (date - first_sunday).num_days() / 7 + 1
}

/// Return the 7 weekday abbreviations starting at `start_weekday`, each
/// truncated to `length` characters (2 for normal, 3 for julian mode).
fn get_weekday_abbreviations(start_weekday: Weekday, length: usize) -> Vec<String> {
    let mut weekday = start_weekday;
    let mut ret = vec![];
    for _ in 0..7 {
        ret.push(weekday.to_string()[..length].to_string());
        weekday = weekday.succ();
    }
    ret
}

/// Render a single month (anchored at `date`, any day-of-month) as exactly
/// [`NUM_CALENDAR_LINES`] lines, padded so multiple months can be laid out
/// side by side.
fn generate_month_lines(date: NaiveDate, options: &CalOptions) -> Vec<String> {
    let (day_width, line_width) = calculate_field_widths(options);

    let fmt = if options.display_mode == DisplayMode::Year {
        "%B"
    } else {
        "%B %Y"
    };
    let mut lines = vec![format!("{:^width$}", date.format(fmt).to_string(), width = line_width)];

    let week_start = if options.monday_first {
        Weekday::Mon
    } else {
        Weekday::Sun
    };

    lines.push(format!(
        "{}{}",
        if options.week_numbers { "   " } else { "" },
        get_weekday_abbreviations(week_start, day_width).join(" ")
    ));

    let mut d = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap();
    let mut current_line = String::new();
    while d.month() == date.month() {
        if options.week_numbers && current_line.is_empty() {
            if options.monday_first {
                current_line.push_str(&format!("{:2} ", d.iso_week().week()));
            } else {
                current_line.push_str(&format!("{:2} ", us_week_number(d)));
            }
        }

        // Space-pad the days that belong to this week but fall in the
        // previous month.
        if d.day() == 1 {
            let num_padding_days = (d - d.week(week_start).first_day()).num_days() as usize;
            current_line.push_str(&" ".repeat(num_padding_days * (day_width + 1)));
        }

        let day_str = if options.julian {
            format!("{:width$}", d.ordinal(), width = day_width)
        } else {
            format!("{:width$}", d.day(), width = day_width)
        };

        let formatted_day = if options.color && options.highlight_date == d {
            format!("\x1b[7m{day_str}\x1b[0m")
        } else {
            day_str
        };

        current_line.push_str(&format!("{formatted_day} "));

        d += chrono::Duration::days(1);

        if d.weekday() == week_start {
            lines.push(current_line.trim_end().to_string());
            current_line.clear();
        }
    }

    if !current_line.is_empty() {
        lines.push(format!(
            "{:<width$}",
            current_line.trim_end(),
            width = line_width
        ));
    }
    while lines.len() < NUM_CALENDAR_LINES {
        lines.push(" ".repeat(line_width));
    }

    lines
}

/// Render the full `cal` output (one or more months, laid out up to
/// [`MAX_CALENDARS_SIDE_BY_SIDE`] at a time) as a single string ending in a
/// trailing newline per printed line.
fn render(options: &CalOptions) -> String {
    let date = NaiveDate::from_ymd_opt(options.date.year(), options.date.month(), 1).unwrap();
    let mut out = String::new();

    let months: Vec<NaiveDate> = match options.display_mode {
        DisplayMode::Year => {
            let (_, line_width) = calculate_field_widths(options);
            let total_width = MAX_CALENDARS_SIDE_BY_SIDE * line_width
                + (MAX_CALENDARS_SIDE_BY_SIDE - 1) * NUM_SPACES_BETWEEN_CALENDARS;
            out.push_str(&format!(
                "{:^width$}\n",
                options.date.year(),
                width = total_width
            ));
            out.push('\n');

            (1..=12)
                .map(|month| NaiveDate::from_ymd_opt(options.date.year(), month, 1).unwrap())
                .collect()
        }
        DisplayMode::ThreeMonths => vec![
            date - Months::new(1),
            date,
            date + Months::new(1),
        ],
        DisplayMode::NMonths(count) => (0..count).map(|x| date + Months::new(x)).collect(),
    };

    for chunk in months.chunks(MAX_CALENDARS_SIDE_BY_SIDE) {
        let all_calendars: Vec<_> = chunk
            .iter()
            .map(|&d| generate_month_lines(d, options))
            .collect();

        for line_idx in 0..NUM_CALENDAR_LINES {
            let line = all_calendars
                .iter()
                .map(|c| c[line_idx].as_str())
                .collect::<Vec<_>>()
                .join(&" ".repeat(NUM_SPACES_BETWEEN_CALENDARS));
            out.push_str(&line);
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(date: NaiveDate, mode: DisplayMode) -> CalOptions {
        CalOptions {
            date,
            highlight_date: date,
            display_mode: mode,
            monday_first: false,
            julian: false,
            week_numbers: false,
            color: false,
        }
    }

    // Expected outputs below were captured verbatim from the system
    // `cal` (util-linux 2.41.5, `cal 1 2000`, `cal -j 1 2000`,
    // `cal -m -w 1 2000`) to double-check against a second source.

    // NOTE: These expected strings use explicit `\n` on a single source
    // line rather than a `"\` continuation + multi-line literal, because
    // Rust's line-continuation escape strips leading whitespace from the
    // following source line — which would silently eat the centering
    // padding these calendars rely on.

    #[test]
    fn january_2000_default_layout() {
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let options = opts(date, DisplayMode::NMonths(1));
        // Cross-checked against system `cal` (util-linux 2.41.5) `cal 1 2000`.
        let expected = "    January 2000    \nSu Mo Tu We Th Fr Sa\n                   1\n 2  3  4  5  6  7  8\n 9 10 11 12 13 14 15\n16 17 18 19 20 21 22\n23 24 25 26 27 28 29\n30 31               \n";
        assert_eq!(render(&options), expected);
    }

    #[test]
    fn january_2000_julian() {
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let mut options = opts(date, DisplayMode::NMonths(1));
        options.julian = true;
        // Cross-checked against system `cal` (util-linux 2.41.5) `cal -j 1 2000`.
        let expected = "       January 2000        \nSun Mon Tue Wed Thu Fri Sat\n                          1\n  2   3   4   5   6   7   8\n  9  10  11  12  13  14  15\n 16  17  18  19  20  21  22\n 23  24  25  26  27  28  29\n 30  31                    \n";
        assert_eq!(render(&options), expected);
    }

    #[test]
    fn january_2000_monday_first_with_week_numbers() {
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let mut options = opts(date, DisplayMode::NMonths(1));
        options.monday_first = true;
        options.week_numbers = true;
        // Cross-checked against system `cal` (util-linux 2.41.5) `cal -m -w 1 2000`.
        let expected = "     January 2000      \n   Mo Tu We Th Fr Sa Su\n52                 1  2\n 1  3  4  5  6  7  8  9\n 2 10 11 12 13 14 15 16\n 3 17 18 19 20 21 22 23\n 4 24 25 26 27 28 29 30\n 5 31                  \n";
        assert_eq!(render(&options), expected);
    }

    #[test]
    fn three_months_around_january_2000() {
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let options = opts(date, DisplayMode::ThreeMonths);
        // This crate's `NUM_SPACES_BETWEEN_CALENDARS` (3) is ported
        // verbatim from uutils/util-linux's cal.rs; the system `cal`
        // (util-linux, a different codebase) uses a 2-space gutter here,
        // so this expected value is derived from the ported algorithm
        // itself (verified arithmetically: 20-col months, 3-space gutter,
        // width-20 center-formatted headers) rather than cross-checked
        // against the system binary.
        let expected = "   December 1999           January 2000          February 2000    \nSu Mo Tu We Th Fr Sa   Su Mo Tu We Th Fr Sa   Su Mo Tu We Th Fr Sa\n          1  2  3  4                      1          1  2  3  4  5\n 5  6  7  8  9 10 11    2  3  4  5  6  7  8    6  7  8  9 10 11 12\n12 13 14 15 16 17 18    9 10 11 12 13 14 15   13 14 15 16 17 18 19\n19 20 21 22 23 24 25   16 17 18 19 20 21 22   20 21 22 23 24 25 26\n26 27 28 29 30 31      23 24 25 26 27 28 29   27 28 29            \n                       30 31                                      \n";
        assert_eq!(render(&options), expected);
    }

    #[test]
    fn year_mode_shows_year_header_and_twelve_months() {
        let date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let options = opts(date, DisplayMode::Year);
        let out = render(&options);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0].trim(), "2000");
        assert!(lines[1].is_empty());
        // 12 months laid out 3 to a row, each 8 lines tall.
        assert_eq!(lines.len(), 2 + 4 * NUM_CALENDAR_LINES);
        assert!(lines[2].contains("January"));
        assert!(lines[2].contains("February"));
        assert!(lines[2].contains("March"));
    }

    #[test]
    fn try_parse_date_numeric() {
        assert_eq!(
            try_parse_date("2000", "1", "1").unwrap(),
            NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()
        );
    }

    #[test]
    fn try_parse_date_month_name() {
        assert_eq!(
            try_parse_date("2000", "August", "1").unwrap(),
            NaiveDate::from_ymd_opt(2000, 8, 1).unwrap()
        );
    }

    #[test]
    fn try_parse_date_invalid_year_errors() {
        assert!(try_parse_date("not-a-year", "1", "1").is_err());
    }

    #[test]
    fn parse_args_single_numeric_arg_is_a_year() {
        let options = parse_args(&["2000".to_string()]).unwrap();
        assert_eq!(options.display_mode, DisplayMode::Year);
        assert_eq!(options.date.year(), 2000);
    }

    #[test]
    fn parse_args_month_year_form() {
        let options = parse_args(&["8".to_string(), "2025".to_string()]).unwrap();
        assert_eq!(options.date.year(), 2025);
        assert_eq!(options.date.month(), 8);
        assert_eq!(options.display_mode, DisplayMode::NMonths(1));
    }

    #[test]
    fn parse_args_day_month_year_form_highlights_the_day() {
        let options = parse_args(&["15".to_string(), "8".to_string(), "2025".to_string()]).unwrap();
        assert_eq!(options.highlight_date, NaiveDate::from_ymd_opt(2025, 8, 15).unwrap());
    }

    #[test]
    fn parse_args_too_many_operands_errors() {
        assert!(parse_args(&["1".into(), "2".into(), "3".into(), "4".into()]).is_err());
    }

    #[test]
    fn parse_args_unknown_option_errors() {
        assert!(parse_args(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn parse_args_year_flag_forces_year_mode() {
        let options = parse_args(&["-y".to_string()]).unwrap();
        assert_eq!(options.display_mode, DisplayMode::Year);
    }

    #[test]
    fn parse_args_months_flag() {
        let options = parse_args(&["-n".to_string(), "5".to_string()]).unwrap();
        assert_eq!(options.display_mode, DisplayMode::NMonths(5));
    }

    #[test]
    fn parse_args_rejects_combined_y_and_n() {
        let err = parse_args(&["-y".to_string(), "-n".to_string(), "3".to_string()]).unwrap_err();
        assert!(err.contains("-y"), "{err}");
    }

    #[test]
    fn parse_args_rejects_combined_y_and_twelve() {
        assert!(parse_args(&["-y".to_string(), "-Y".to_string()]).is_err());
    }

    #[test]
    fn parse_args_invalid_months_value_errors() {
        assert!(parse_args(&["-n".to_string(), "not-a-number".to_string()]).is_err());
    }
}
