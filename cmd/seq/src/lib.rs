//! user seq — print a sequence of numbers.

use usercore::Ui;

/// Entry point for the `seq` utility. Parses `std::env::args()` and prints
/// a sequence of numbers to stdout, one per `separator` (default newline).
///
/// Supports `seq LAST`, `seq FIRST LAST`, and `seq FIRST INCREMENT LAST`.
///
/// Returns 0 on success, 1 on a usage error (missing/extra operands,
/// unparsable numbers, a zero or non-finite increment).
pub fn run() -> i32 {
    let ui = Ui::new("seq");
    let mut separator = "\n".to_string();
    let mut equal_width = false;
    let mut nums: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-h" || a == "--help" {
            print!(
                "Usage: seq [OPTION]... LAST\n\
 seq [OPTION]... FIRST LAST\n\
 seq [OPTION]... FIRST INCREMENT LAST\n\
 -s, --separator=STRING separator (default: newline)\n\
 -w, --equal-width pad with leading zeroes\n"
            );
            return 0;
        }
        if a == "--version" {
            println!("seq (user_utils) 0.1.0");
            return 0;
        }
        if a == "-s" || a == "--separator" {
            i += 1;
            let Some(sep) = args.get(i) else {
                ui.err("option requires an argument -- 's'");
                return 1;
            };
            separator = sep.clone();
            i += 1;
            continue;
        }
        if a == "-w" || a == "--equal-width" {
            equal_width = true;
            i += 1;
            continue;
        }
        if let Some(rest) = a.strip_prefix("-s") {
            if !rest.is_empty() {
                separator = rest.to_string();
                i += 1;
                continue;
            }
        }
        // numeric or signed number can start with -
        nums.push(a.clone());
        i += 1;
    }
    if nums.is_empty() {
        ui.err("missing operand");
        return 1;
    }

    let (first, step, last) = match parse_operands(&nums) {
        Ok(triple) => triple,
        Err(msg) => {
            ui.err(&msg);
            return 1;
        }
    };

    let values = generate_sequence(first, step, last);
    let width = if equal_width {
        values.iter().map(|s| s.len()).max().unwrap_or(0)
    } else {
        0
    };
    for (idx, v) in values.iter().enumerate() {
        if idx > 0 {
            print!("{separator}");
        }
        if width > 0 {
            print!("{v:0>width$}");
        } else {
            print!("{v}");
        }
    }
    println!();
    0
}

/// Parse 1–3 numeric operands into `(first, increment, last)`, applying
/// `seq`'s defaults (`first` = 1, `increment` = 1 when omitted).
///
/// Rejects operand counts outside 1..=3, unparsable floats, and increments
/// that are zero or non-finite (NaN/±inf would otherwise never terminate
/// the sequence-generation loop, or terminate after a meaningless number
/// of steps).
fn parse_operands(nums: &[String]) -> Result<(f64, f64, f64), String> {
    let parse = |s: &str| -> Result<f64, String> {
        s.parse::<f64>()
            .map_err(|_| format!("invalid floating point argument: '{s}'"))
            .and_then(|v| {
                if v.is_finite() {
                    Ok(v)
                } else {
                    Err(format!("invalid floating point argument: '{s}'"))
                }
            })
    };
    let (first, step, last) = match nums.len() {
        1 => (1.0, 1.0, parse(&nums[0])?),
        2 => (parse(&nums[0])?, 1.0, parse(&nums[1])?),
        3 => (parse(&nums[0])?, parse(&nums[1])?, parse(&nums[2])?),
        0 => return Err("missing operand".to_string()),
        _ => return Err(format!("extra operand '{}'", nums[3])),
    };
    if step == 0.0 {
        return Err("invalid zero increment value: '0'".to_string());
    }
    Ok((first, step, last))
}

/// Convert a sequence value to `seq`'s output form: integral-looking values
/// (within `1e-12` of an integer and under `1e15` in magnitude) print as
/// plain integers; everything else keeps its default float formatting.
fn format_number(n: f64) -> String {
    if n.fract().abs() < 1e-12 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Generate the formatted sequence from `first` to `last` stepping by
/// `step`. Bounded at 10,000,000 elements as a hard stop against runaway
/// output; callers are expected to have already rejected zero/non-finite
/// `step` via [`parse_operands`].
fn generate_sequence(first: f64, step: f64, last: f64) -> Vec<String> {
    let mut values: Vec<String> = Vec::new();
    let mut cur = first;
    for _ in 0..10_000_000 {
        if step > 0.0 && cur > last + 1e-9 {
            break;
        }
        if step < 0.0 && cur < last - 1e-9 {
            break;
        }
        values.push(format_number(cur));
        let next = cur + step;
        if next == cur {
            break;
        }
        cur = next;
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_operand_defaults_first_and_step() {
        let (f, s, l) = parse_operands(&["5".to_string()]).unwrap();
        assert_eq!((f, s, l), (1.0, 1.0, 5.0));
    }

    #[test]
    fn two_operands_default_step() {
        let (f, s, l) = parse_operands(&["2".to_string(), "8".to_string()]).unwrap();
        assert_eq!((f, s, l), (2.0, 1.0, 8.0));
    }

    #[test]
    fn three_operands_all_explicit() {
        let (f, s, l) =
            parse_operands(&["1".to_string(), "2".to_string(), "9".to_string()]).unwrap();
        assert_eq!((f, s, l), (1.0, 2.0, 9.0));
    }

    #[test]
    fn zero_increment_is_rejected() {
        let err = parse_operands(&["1".to_string(), "0".to_string(), "9".to_string()]).unwrap_err();
        assert!(err.contains("zero increment"));
    }

    #[test]
    fn nan_increment_is_rejected_not_looped_forever() {
        let err =
            parse_operands(&["1".to_string(), "nan".to_string(), "9".to_string()]).unwrap_err();
        assert!(err.contains("invalid floating point argument"));
    }

    #[test]
    fn infinite_increment_is_rejected() {
        let err =
            parse_operands(&["1".to_string(), "inf".to_string(), "9".to_string()]).unwrap_err();
        assert!(err.contains("invalid floating point argument"));
    }

    #[test]
    fn unparsable_operand_is_rejected() {
        let err = parse_operands(&["abc".to_string()]).unwrap_err();
        assert!(err.contains("invalid floating point argument"));
    }

    #[test]
    fn too_many_operands_is_rejected() {
        let err = parse_operands(&[
            "1".to_string(),
            "2".to_string(),
            "3".to_string(),
            "4".to_string(),
        ])
        .unwrap_err();
        assert!(err.contains("extra operand"));
    }

    #[test]
    fn generate_sequence_ascending() {
        let v = generate_sequence(1.0, 1.0, 5.0);
        assert_eq!(v, vec!["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn generate_sequence_descending() {
        let v = generate_sequence(5.0, -2.0, 1.0);
        assert_eq!(v, vec!["5", "3", "1"]);
    }

    #[test]
    fn generate_sequence_empty_when_first_past_last() {
        let v = generate_sequence(5.0, 1.0, 1.0);
        assert!(v.is_empty());
    }

    #[test]
    fn format_number_integral() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(-4.0), "-4");
    }

    #[test]
    fn format_number_fractional() {
        assert_eq!(format_number(2.5), "2.5");
    }
}
