//! user pgrep — look up or signal processes by name.
use std::fs;

use usercore::posix_regex::Regex;
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
    // SAFETY: restoring the default SIGPIPE action has no preconditions.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let name = if is_pkill { "pkill" } else { "pgrep" };
    let ui = Ui::new(name);
    let (mut full, mut list_name, mut list_full, mut ignore_case) = (false, false, false, false);
    let (mut exact, mut count, mut newest, mut oldest, mut invert) = (false, false, false, false, false);
    let mut delim = "\n".to_string();
    let mut uids: Vec<u32> = Vec::new();
    let mut ppids: Vec<i32> = Vec::new();
    let mut signal = libc::SIGTERM;
    let mut pattern: Option<String> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        i += 1;
        let mut take = |what: &str| -> Option<String> {
            let v = args.get(i).cloned();
            i += 1;
            if v.is_none() {
                ui.err(&format!("option requires an argument -- '{what}'"));
            }
            v
        };
        match a.as_str() {
            "-h" | "--help" => {
                print!("Usage: {name} [OPTION]... PATTERN\n -f full command line\n -x exact match\n -i ignore case\n -l list name, -a list full command line\n -c count, -n newest, -o oldest, -v invert\n -d DELIM, -u USER, -P PPID{}\n", if is_pkill { "\n -SIGNAL, --signal SIG" } else { "" });
                return 0;
            }
            "--version" | "-V" => {
                println!("{name} (user_utils) 0.2.0");
                return 0;
            }
            "-f" | "--full" => full = true,
            "-l" | "--list-name" => list_name = true,
            "-a" | "--list-full" => list_full = true,
            "-i" | "--ignore-case" => ignore_case = true,
            "-x" | "--exact" => exact = true,
            "-c" | "--count" => count = true,
            "-n" | "--newest" => newest = true,
            "-o" | "--oldest" => oldest = true,
            "-v" | "--inverse" => invert = true,
            "-d" | "--delimiter" => match take("d") { Some(v) => delim = v, None => return 2 },
            "-u" | "-U" | "--euid" | "--uid" => match take("u") {
                Some(v) => {
                    for u in v.split(',') {
                        match u.parse::<u32>().ok().or_else(|| usercore::nss::user_by_name(u).map(|p| p.uid)) {
                            Some(id) => uids.push(id),
                            None => {
                                ui.err(&format!("invalid user name: {u}"));
                                return 2;
                            }
                        }
                    }
                }
                None => return 2,
            },
            "-P" | "--parent" => match take("P") {
                Some(v) => ppids.extend(v.split(',').filter_map(|x| x.parse::<i32>().ok())),
                None => return 2,
            },
            "--signal" if is_pkill => match take("signal").and_then(|v| v.parse().ok().or_else(|| parse_sig(&v))) {
                Some(n) => signal = n,
                None => {
                    ui.err("invalid signal");
                    return 2;
                }
            },
            s if is_pkill && s.starts_with('-') && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit()) => {
                signal = s[1..].parse().unwrap_or(libc::SIGTERM);
            }
            s if is_pkill && s.starts_with('-') && s.len() > 1 && parse_sig(&s[1..]).is_some() => {
                signal = parse_sig(&s[1..]).unwrap_or(libc::SIGTERM);
            }
            // combined flags such as -fx or -la
            s if s.starts_with('-') && s.len() > 2 && !s.starts_with("--") && s[1..].chars().all(|c| "flaixcnov".contains(c)) => {
                for c in s[1..].chars() {
                    match c {
                        'f' => full = true,
                        'l' => list_name = true,
                        'a' => list_full = true,
                        'i' => ignore_case = true,
                        'x' => exact = true,
                        'c' => count = true,
                        'n' => newest = true,
                        'o' => oldest = true,
                        _ => invert = true,
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("invalid option -- '{}'", s.trim_start_matches('-')));
                return 2;
            }
            other => pattern = Some(other.to_string()),
        }
    }
    if pattern.is_none() && uids.is_empty() && ppids.is_empty() {
        ui.err("no matching criteria specified");
        return 2;
    }
    let re = match &pattern {
        Some(p) => {
            let p = if exact { format!("^({p})$") } else { p.clone() };
            match Regex::new(&p, true, ignore_case) {
                Ok(r) => Some(r),
                Err(e) => {
                    ui.err(&e);
                    return 2;
                }
            }
        }
        None => None,
    };
    let me = std::process::id() as i32;
    let mut hits: Vec<P> = iter_procs()
        .into_iter()
        .filter(|p| p.pid != me)
        .filter(|p| {
            let m = re.as_ref().is_none_or(|r| r.is_match(if full { &p.cmd } else { &p.name }))
                && (uids.is_empty() || uids.contains(&p.uid))
                && (ppids.is_empty() || ppids.contains(&p.ppid));
            m != invert
        })
        .collect();
    if newest || oldest {
        hits.sort_by_key(|p| p.start);
        let pick = if newest { hits.pop() } else { hits.into_iter().next() };
        hits = pick.into_iter().collect();
    }
    if hits.is_empty() {
        if count {
            println!("0");
        }
        return 1;
    }
    if is_pkill {
        for p in &hits {
            // SAFETY: kill takes plain integers; a vanished pid just fails with ESRCH.
            unsafe { libc::kill(p.pid, signal) };
        }
        if count {
            println!("{}", hits.len());
        }
        return 0;
    }
    if count {
        println!("{}", hits.len());
        return 0;
    }
    let out: Vec<String> = hits
        .iter()
        .map(|p| {
            if list_full {
                format!("{} {}", p.pid, p.cmd)
            } else if list_name {
                format!("{} {}", p.pid, p.name)
            } else {
                p.pid.to_string()
            }
        })
        .collect();
    println!("{}", out.join(&delim));
    0
}

struct P {
    pid: i32,
    ppid: i32,
    uid: u32,
    start: u64,
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
        let field = |k: &str| status.lines().find(|l| l.starts_with(k)).and_then(|l| l[k.len()..].split_whitespace().next()).and_then(|x| x.parse().ok());
        let ppid = field("PPid:").map(|x: i64| x as i32).unwrap_or(0);
        let uid = field("Uid:").map(|x: i64| x as u32).unwrap_or(u32::MAX);
        // starttime is field 22 of /proc/<pid>/stat, after the ")" that ends the name
        let start = fs::read_to_string(path.join("stat"))
            .ok()
            .and_then(|st| st.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(19).and_then(|x| x.parse().ok())))
            .unwrap_or(0);
        v.push(P { pid, ppid, uid, start, name, cmd });
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
    fn patterns_are_extended_regex() {
        assert!(Regex::new("fire", true, false).unwrap().is_match("firefox"));
        assert!(!Regex::new("fire", true, false).unwrap().is_match("Firefox"));
        assert!(Regex::new("FIRE", true, true).unwrap().is_match("Firefox"));
        assert!(Regex::new("^(cosmic-comp)$", true, false).unwrap().is_match("cosmic-comp"));
        assert!(!Regex::new("^(cosmic)$", true, false).unwrap().is_match("cosmic-comp"));
        assert!(Regex::new("sshd|cron", true, false).unwrap().is_match("crond"));
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
