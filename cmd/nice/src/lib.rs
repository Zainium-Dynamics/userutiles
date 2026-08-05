//! user nice — run with modified niceness (or print current).
use std::os::unix::process::CommandExt;
use std::process::Command;

use usercore::Ui;

/// Entry point for the `nice` utility. Parses `std::env::args()` as
/// `[-n ADJUSTMENT] [COMMAND [ARG]...]`. With no COMMAND, prints the
/// process's current niceness. With a COMMAND, raises/lowers niceness by
/// ADJUSTMENT (default 10, clamped to the valid `[-20, 19]` range) and
/// `exec`s COMMAND in place.
///
/// Returns 0 when just printing niceness; otherwise this only returns (with
/// 126 or 127) if `exec` itself failed — on success the process image is
/// replaced and `run` never returns to its caller.
pub fn run() -> i32 {
    let ui = Ui::new("nice");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut adjustment: Option<i32> = None;
    let mut cmd: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: nice [OPTION] [COMMAND [ARG]...]\nRun COMMAND with an adjusted niceness, or print current niceness.\n -n, --adjustment=N add N to the niceness (default 10)\n");
                return 0;
            }
            "--version" => {
                println!("nice (user_utils) 0.1.0");
                return 0;
            }
            "-n" | "--adjustment" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option requires an argument -- 'n'");
                    return 1;
                };
                adjustment = match parse_adjustment(v) {
                    Ok(n) => Some(n),
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                };
            }
            s if s.starts_with("-n") && s.len() > 2 => match parse_adjustment(&s[2..]) {
                Ok(n) => adjustment = Some(n),
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with("--adjustment=") => {
                match parse_adjustment(&s["--adjustment=".len()..]) {
                    Ok(n) => adjustment = Some(n),
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            // "-10" style: a leading-digit (or leading-sign-then-digit)
            // argument is an inline adjustment, not the command to run.
            s if s.starts_with('-') && s.len() > 1 && looks_like_signed_number(s) => {
                match parse_adjustment(s) {
                    Ok(n) => adjustment = Some(n),
                    Err(_) => {
                        cmd.push(s.to_string());
                        cmd.extend(args[i + 1..].iter().cloned());
                        break;
                    }
                }
            }
            other => {
                cmd.push(other.to_string());
                cmd.extend(args[i + 1..].iter().cloned());
                break;
            }
        }
        i += 1;
    }

    if cmd.is_empty() {
        // SAFETY: `libc::getpriority` takes only plain integer arguments
        // (`PRIO_PROCESS` and `0`, meaning "the calling process") and has no
        // pointer arguments; it cannot cause UB regardless of process state or
        // permissions — failure is only ever signalled via `errno`.
        let n = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        println!("{n}");
        return 0;
    }

    let adj = adjustment.unwrap_or(10);
    // SAFETY: same as above — `getpriority(PRIO_PROCESS, 0)` takes only plain
    // integers and cannot cause UB.
    let current = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
    let new_nice = clamp_niceness(current, adj);
    // SAFETY: `libc::setpriority` takes only plain integer arguments
    // (`PRIO_PROCESS`, `0` for "the calling process", and `new_nice` which is
    // clamped to the valid `[-20, 19]` range above); it has no pointer
    // arguments and cannot cause UB. If the calling process lacks permission to
    // lower its niceness the call simply fails and sets `errno`, which is not
    // inspected here (matching standard `nice(1)` behaviour of proceeding to
    // exec the command regardless).
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, new_nice);
    }

    let err = Command::new(&cmd[0]).args(&cmd[1..]).exec();
    ui.err(&format!("'{}': {err}", cmd[0]));
    if err.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// Parse a niceness adjustment value (e.g. `"10"`, `"-5"`, `"+3"`). Unlike
/// the previous `unwrap_or`-based parsing, an unparsable value is a hard
/// error rather than being silently ignored.
fn parse_adjustment(s: &str) -> Result<i32, String> {
    s.parse::<i32>()
        .map_err(|_| format!("invalid adjustment '{s}'"))
}

/// True if `s` looks like a signed-integer CLI token (`-10`, `+5`), as
/// opposed to a flag or the name of a command to run.
fn looks_like_signed_number(s: &str) -> bool {
    let rest = s.strip_prefix('-').unwrap_or(s);
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

/// Combine current niceness with an adjustment, saturating (not wrapping or
/// panicking on overflow) and clamping to the kernel-valid `[-20, 19]`
/// range.
fn clamp_niceness(current: i32, adjustment: i32) -> i32 {
    current.saturating_add(adjustment).clamp(-20, 19)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_adjustment_accepts_signed_ints() {
        assert_eq!(parse_adjustment("10"), Ok(10));
        assert_eq!(parse_adjustment("-5"), Ok(-5));
        assert_eq!(parse_adjustment("+3"), Ok(3));
    }

    #[test]
    fn parse_adjustment_rejects_garbage() {
        assert!(parse_adjustment("abc").is_err());
        assert!(parse_adjustment("").is_err());
        assert!(parse_adjustment("1.5").is_err());
    }

    #[test]
    fn looks_like_signed_number_matches_digit_tokens() {
        assert!(looks_like_signed_number("-10"));
        assert!(looks_like_signed_number("-0"));
        assert!(!looks_like_signed_number("-n"));
        assert!(!looks_like_signed_number("-"));
        assert!(!looks_like_signed_number("--adjustment"));
    }

    #[test]
    fn clamp_niceness_stays_within_kernel_range() {
        assert_eq!(clamp_niceness(0, 10), 10);
        assert_eq!(clamp_niceness(19, 10), 19);
        assert_eq!(clamp_niceness(-20, -10), -20);
    }

    #[test]
    fn clamp_niceness_does_not_overflow_on_extreme_adjustment() {
        // i32::MAX + i32::MAX would overflow a raw `+`; must saturate, not panic.
        assert_eq!(clamp_niceness(i32::MAX, i32::MAX), 19);
        assert_eq!(clamp_niceness(i32::MIN, i32::MIN), -20);
    }
}
