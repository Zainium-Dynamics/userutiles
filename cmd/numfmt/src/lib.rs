//! user numfmt — reformat numbers.
use usercore::Ui;

/// Entry point for the `numfmt` utility. Parses `std::env::args()` as
/// `[--to=si|iec|iec-i] [--from=si|iec|iec-i] [NUMBER]...` (reading
/// whitespace-separated tokens from stdin if no NUMBERs are given) and
/// reformats each NUMBER: expanding a human-readable suffix (`--from`, or
/// auto-detected) to a plain integer, or scaling a plain integer up to a
/// human-readable IEC form (`--to`).
///
/// Returns 0 on success, 1 on a usage or parse error.
pub fn run() -> i32 {
    let ui = Ui::new("numfmt");
    let mut to_human = false;
    let mut from_human = false;
    let mut values: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: numfmt [OPTION]... [NUMBER]...\n\
 Reformat NUMBER(s).\n\
 --to=si|iec auto-scale output to human form\n\
 --from=si|iec auto-scale input from human form\n"
                );
                return 0;
            }
            "--version" => {
                println!("numfmt (user_utils) 0.1.0");
                return 0;
            }
            "--to=si" | "--to=iec" | "--to=iec-i" => to_human = true,
            "--from=si" | "--from=iec" | "--from=iec-i" => from_human = true,
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => values.push(other.to_string()),
        }
    }
    if values.is_empty() {
        use std::io::{self, BufRead};
        for line in io::stdin().lock().lines() {
            match line {
                Ok(l) => values.push(l),
                Err(e) => {
                    ui.err(&format!("{e}"));
                    return 1;
                }
            }
        }
    }
    for v in values {
        let tok = v.split_whitespace().next().unwrap_or("");
        if tok.is_empty() {
            println!();
            continue;
        }
        if from_human || (!to_human && tok.chars().any(|c| c.is_ascii_alphabetic())) {
            match parse_human(tok) {
                Ok(n) => println!("{n}"),
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            }
        } else if to_human {
            match tok.parse::<u64>() {
                Ok(n) => println!("{}", to_iec(n)),
                Err(_) => {
                    ui.err(&format!("invalid number: {tok}"));
                    return 1;
                }
            }
        } else {
            println!("{tok}");
        }
    }
    0
}

/// Parse a human-readable size like `"1.5K"`, `"4Gi"`, `"10GB"` into a raw
/// `u64` byte count. Rejects non-finite/negative magnitudes and results
/// that don't fit in `u64` (rather than silently saturating), and rejects
/// an unrecognized suffix.
fn parse_human(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let mut num = String::new();
    let mut suf = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            num.push(c);
        } else {
            suf.push(c);
        }
    }
    let base: f64 = num.parse().map_err(|_| format!("invalid number: '{s}'"))?;
    if !base.is_finite() || base < 0.0 {
        return Err(format!("invalid number: '{s}'"));
    }
    let mult: f64 = match suf.to_ascii_uppercase().as_str() {
        "" => 1.0,
        "K" | "KI" | "KIB" => 1024.0,
        "M" | "MI" | "MIB" => 1024f64.powi(2),
        "G" | "GI" | "GIB" => 1024f64.powi(3),
        "T" | "TI" | "TIB" => 1024f64.powi(4),
        "KB" => 1000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        _ => return Err(format!("invalid suffix in '{s}'")),
    };
    let scaled = base * mult;
    // `u64::MAX as f64` rounds up past the true max representable u64, so
    // compare against it directly rather than casting first: an `as u64`
    // cast on an out-of-range float silently saturates instead of erroring.
    if !scaled.is_finite() || scaled > u64::MAX as f64 {
        return Err(format!("number too large: '{s}'"));
    }
    Ok(scaled as u64)
}

/// Scale a raw byte count `n` up to the largest IEC unit (K/M/G/T/P) for
/// which the scaled value is still `>= 1.0`, formatted to one decimal
/// place; values under 1024 are printed as plain integers.
fn to_iec(n: u64) -> String {
    const U: [&str; 6] = ["", "K", "M", "G", "T", "P"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n}")
    } else {
        format!("{v:.1}{}", U[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_human_plain_integer() {
        assert_eq!(parse_human("42"), Ok(42));
    }

    #[test]
    fn parse_human_binary_suffixes() {
        assert_eq!(parse_human("1K"), Ok(1024));
        assert_eq!(parse_human("1Ki"), Ok(1024));
        assert_eq!(parse_human("1KiB"), Ok(1024));
        assert_eq!(parse_human("2M"), Ok(2 * 1024 * 1024));
    }

    #[test]
    fn parse_human_decimal_suffixes() {
        assert_eq!(parse_human("1KB"), Ok(1000));
        assert_eq!(parse_human("2MB"), Ok(2_000_000));
    }

    #[test]
    fn parse_human_fractional_value() {
        assert_eq!(parse_human("1.5K"), Ok(1536));
    }

    #[test]
    fn parse_human_rejects_bad_number() {
        assert!(parse_human("abc").is_err());
        assert!(parse_human("").is_err());
    }

    #[test]
    fn parse_human_rejects_unknown_suffix() {
        assert!(parse_human("5Q").is_err());
    }

    #[test]
    fn parse_human_rejects_negative() {
        assert!(parse_human("-5K").is_err());
    }

    #[test]
    fn parse_human_rejects_overflow_instead_of_saturating() {
        // u64::MAX bytes worth of terabytes overflows u64 when scaled; this
        // must be a hard error, not a silently saturated/wrapped value.
        let huge = format!("{}T", u64::MAX);
        assert!(parse_human(&huge).is_err());
    }

    #[test]
    fn to_iec_small_value_is_plain() {
        assert_eq!(to_iec(512), "512");
    }

    #[test]
    fn to_iec_scales_to_largest_unit() {
        assert_eq!(to_iec(1024), "1.0K");
        assert_eq!(to_iec(1536), "1.5K");
        assert_eq!(to_iec(1024 * 1024), "1.0M");
    }

    #[test]
    fn to_iec_zero_is_plain_zero() {
        assert_eq!(to_iec(0), "0");
    }

    #[test]
    fn to_iec_u64_max_does_not_panic() {
        // Must not panic/overflow while scaling down a near-maximal value.
        let s = to_iec(u64::MAX);
        assert!(s.ends_with('P'));
    }
}
