//! user pgrep — look up or signal processes by name.
use std::fs;

use usercore::Ui;

/// Entry point for the `pgrep` utility (also serves as `pkill` when
/// invoked under that name via [`run_as_pkill`]). Parses
/// `std::env::args()` as `[OPTION]... PATTERN` and scans `/proc` for
/// processes whose name (or, with `-f`, full command line) contains
/// PATTERN.
///
/// Returns 0 if at least one process matched, 1 if none did, 2 on a usage
/// error.
pub fn run() -> i32 {
    let invoked = std::env::args().next().unwrap_or_default();
    run_inner(invoked.ends_with("pkill"))
}

/// Entry point forced into `pkill` mode (signal matching processes instead
/// of just listing them) regardless of argv\[0\].
pub fn run_as_pkill() -> i32 {
    run_inner(true)
}

fn run_inner(is_pkill: bool) -> i32 {
    let ui = Ui::new(if is_pkill { "pkill" } else { "pgrep" });
    let mut full = false;
    let mut list_name = false;
    let mut ignore_case = false;
    let mut signal = libc::SIGTERM;
    let mut pattern = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                if is_pkill {
                    print!("Usage: pkill [OPTION]... PATTERN\nSignal processes based on name.\n -f full command line\n -i ignore case\n -SIGNAL signal number/name\n");
                } else {
                    print!("Usage: pgrep [OPTION]... PATTERN\nLook up processes based on name.\n -f full command line\n -l list name\n -i ignore case\n");
                }
                return 0;
            }
            "--version" => {
                println!(
                    "{} (user_utils) 0.1.0",
                    if is_pkill { "pkill" } else { "pgrep" }
                );
                return 0;
            }
            "-f" | "--full" => full = true,
            "-l" | "--list-name" => list_name = true,
            "-i" | "--ignore-case" => ignore_case = true,
            s if s.starts_with('-')
                && s.len() > 1
                && s[1..].chars().all(|c| c.is_ascii_digit()) =>
            {
                match s[1..].parse::<i32>() {
                    Ok(n) => signal = n,
                    Err(_) => {
                        ui.err(&format!("invalid signal number '{}'", &s[1..]));
                        return 2;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 && s.as_bytes()[1].is_ascii_alphabetic() => {
                match parse_sig(&s[1..]) {
                    Some(n) => signal = n,
                    None => {
                        ui.err(&format!("unknown signal '{}'", &s[1..]));
                        return 2;
                    }
                }
            }
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 2;
            }
            other => pattern = Some(other.to_string()),
        }
        i += 1;
    }
    let pattern = match pattern {
        Some(p) => p,
        None => {
            ui.err("missing pattern");
            return 2;
        }
    };
    let me = std::process::id() as i32;
    let mut found = 0;
    for p in iter_procs() {
        if p.pid == me {
            continue;
        }
        let hay = if full { &p.cmd } else { &p.name };
        if !matches_pattern(hay, &pattern, ignore_case) {
            continue;
        }
        found += 1;
        if is_pkill {
            // SAFETY: `libc::kill` takes only plain integer arguments — `p.pid`
            // (parsed from a `/proc/<pid>` directory name, so it names a process
            // that existed at scan time) and `signal` (either `SIGTERM` or a
            // value parsed/validated from CLI input above) — with no pointer
            // arguments, so it cannot cause UB. If the target process has since
            // exited or the signal number is invalid, the call simply fails and
            // sets `errno` (`ESRCH`/`EINVAL`), which matches standard `kill(1)`/
            // `pkill(1)` behaviour of not treating that as fatal.
            unsafe {
                libc::kill(p.pid, signal);
            }
        } else if list_name {
            println!("{} {}", p.pid, p.name);
        } else {
            println!("{}", p.pid);
        }
    }
    if found == 0 {
        1
    } else {
        0
    }
}

/// True if `pattern` occurs anywhere in `haystack`, comparing case-
/// insensitively when `ignore_case` is set.
fn matches_pattern(haystack: &str, pattern: &str, ignore_case: bool) -> bool {
    if ignore_case {
        haystack
            .to_ascii_lowercase()
            .contains(&pattern.to_ascii_lowercase())
    } else {
        haystack.contains(pattern)
    }
}

struct P {
    pid: i32,
    name: String,
    cmd: String,
}

/// Scan `/proc` for running processes, reading each one's short name (from
/// `/proc/<pid>/status`'s `Name:` field) and full command line (from
/// `/proc/<pid>/cmdline`, falling back to `[name]` if that's empty, as for
/// kernel threads). Processes that disappear mid-scan or have unreadable
/// status files are simply skipped rather than aborting the whole scan.
fn iter_procs() -> Vec<P> {
    let mut v = Vec::new();
    let Ok(rd) = fs::read_dir("/proc") else {
        return v;
    };
    for ent in rd.flatten() {
        let s = ent.file_name().to_string_lossy().into_owned();
        if !s.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = s.parse() else { continue };
        let path = ent.path();
        let status = fs::read_to_string(path.join("status")).unwrap_or_default();
        let name = status
            .lines()
            .find(|l| l.starts_with("Name:"))
            .map(|l| l[5..].trim().to_string())
            .unwrap_or_default();
        let cmdline = fs::read(path.join("cmdline")).unwrap_or_default();
        let cmd = if cmdline.is_empty() {
            format!("[{name}]")
        } else {
            String::from_utf8_lossy(&cmdline)
                .replace('\0', " ")
                .trim()
                .to_string()
        };
        v.push(P { pid, name, cmd });
    }
    v
}

/// Parse a signal name or number (with or without a `SIG` prefix, e.g.
/// `"9"`, `"KILL"`, `"SIGKILL"`) into its numeric value. Returns `None` for
/// an unrecognized name (deliberately, unlike the previous
/// `unwrap_or(SIGTERM)` behaviour, so callers can report a clear error
/// instead of silently signalling with the wrong signal).
fn parse_sig(s: &str) -> Option<i32> {
    let s = s.strip_prefix("SIG").unwrap_or(s);
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    Some(match s.to_ascii_uppercase().as_str() {
        "TERM" => libc::SIGTERM,
        "KILL" => libc::SIGKILL,
        "HUP" => libc::SIGHUP,
        "INT" => libc::SIGINT,
        "USR1" => libc::SIGUSR1,
        "USR2" => libc::SIGUSR2,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pattern_case_sensitive_by_default() {
        assert!(matches_pattern("firefox", "fire", false));
        assert!(!matches_pattern("Firefox", "fire", false));
    }

    #[test]
    fn matches_pattern_ignore_case() {
        assert!(matches_pattern("Firefox", "FIRE", true));
    }

    #[test]
    fn matches_pattern_empty_pattern_matches_everything() {
        assert!(matches_pattern("anything", "", false));
    }

    #[test]
    fn parse_sig_accepts_numeric() {
        assert_eq!(parse_sig("9"), Some(9));
    }

    #[test]
    fn parse_sig_accepts_name_with_and_without_prefix() {
        assert_eq!(parse_sig("KILL"), Some(libc::SIGKILL));
        assert_eq!(parse_sig("SIGKILL"), Some(libc::SIGKILL));
        assert_eq!(parse_sig("term"), Some(libc::SIGTERM));
    }

    #[test]
    fn parse_sig_rejects_unknown_name() {
        assert_eq!(parse_sig("NOTASIGNAL"), None);
    }

    #[test]
    fn iter_procs_finds_self() {
        let me = std::process::id() as i32;
        let procs = iter_procs();
        assert!(procs.iter().any(|p| p.pid == me));
    }
}
