//! user kill — send signals to processes.
use usercore::Ui;

/// Entry point for the `kill` utility. Parses `std::env::args()` and
/// sends a signal (default `SIGTERM`, overridable via `-s`/`-SIGNAL`/
/// `-N`) to each PID operand via `kill(2)`, or lists known signal names
/// (`-l`/`-L`).
///
/// Returns 0 if every signal was delivered successfully, 1 if any `kill`
/// call failed (e.g. no such process) or on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("kill");
    let mut signal: i32 = libc::SIGTERM;
    let mut list = false;
    let mut table = false;
    let mut pids: Vec<i32> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print!("Usage: kill [-s SIGNAL | -SIGNAL] PID...\n kill -l [SIGNAL]...\nSend a signal to processes.\n");
                return 0;
            }
            "--version" => {
                println!("kill (user_utils) 0.1.0");
                return 0;
            }
            "-l" | "--list" => list = true,
            "-L" | "--table" => table = true,
            "-s" | "--signal" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 's'");
                    return 1;
                }
                match parse_signal(&args[i]) {
                    Ok(s) => signal = s,
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                // -9 or -TERM or -n PID (negative pid)
                let rest = &s[1..];
                if rest.chars().all(|c| c.is_ascii_digit()) {
                    // could be signal number or negative pid; if only digits and small, signal
                    if let Ok(n) = rest.parse::<i32>() {
                        if n <= 64 && pids.is_empty() {
                            signal = n;
                        } else {
                            pids.push(-n);
                        }
                    }
                } else if let Ok(s) = parse_signal(rest) {
                    signal = s;
                } else if rest.starts_with('-') {
                    // -- invalid
                    ui.err(&format!("invalid option -- '{s}'"));
                    return 1;
                } else {
                    ui.err(&format!("invalid signal '{rest}'"));
                    return 1;
                }
            }
            other => match other.parse::<i32>() {
                Ok(p) => pids.push(p),
                Err(_) => {
                    ui.err(&format!("invalid argument '{other}'"));
                    return 1;
                }
            },
        }
        i += 1;
    }

    if list || table {
        print_signals(table);
        return 0;
    }
    if pids.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let mut status = 0;
    for pid in pids {
        // SAFETY: `kill(2)` takes only plain integer arguments (a pid and
        // a signal number) and performs no memory access on the caller's
        // behalf; a nonexistent pid or invalid signal is reported via a
        // normal errno/return-value failure (handled below), never
        // undefined behavior.
        let rc = unsafe { libc::kill(pid as libc::pid_t, signal) };
        if rc != 0 {
            ui.err(&format!("({pid}): {}", std::io::Error::last_os_error()));
            status = 1;
        }
    }
    status
}

/// Resolve a signal spec (`"9"`, `"TERM"`, or `"SIGTERM"`) to its numeric
/// value.
fn parse_signal(s: &str) -> Result<i32, String> {
    let s = s.strip_prefix("SIG").unwrap_or(s);
    if let Ok(n) = s.parse::<i32>() {
        return Ok(n);
    }
    let n = match s.to_ascii_uppercase().as_str() {
        "HUP" => libc::SIGHUP,
        "INT" => libc::SIGINT,
        "QUIT" => libc::SIGQUIT,
        "ILL" => libc::SIGILL,
        "TRAP" => libc::SIGTRAP,
        "ABRT" | "IOT" => libc::SIGABRT,
        "BUS" => libc::SIGBUS,
        "FPE" => libc::SIGFPE,
        "KILL" => libc::SIGKILL,
        "USR1" => libc::SIGUSR1,
        "SEGV" => libc::SIGSEGV,
        "USR2" => libc::SIGUSR2,
        "PIPE" => libc::SIGPIPE,
        "ALRM" => libc::SIGALRM,
        "TERM" => libc::SIGTERM,
        "STKFLT" => 16,
        "CHLD" | "CLD" => libc::SIGCHLD,
        "CONT" => libc::SIGCONT,
        "STOP" => libc::SIGSTOP,
        "TSTP" => libc::SIGTSTP,
        "TTIN" => libc::SIGTTIN,
        "TTOU" => libc::SIGTTOU,
        "URG" => libc::SIGURG,
        "XCPU" => libc::SIGXCPU,
        "XFSZ" => libc::SIGXFSZ,
        "VTALRM" => libc::SIGVTALRM,
        "PROF" => libc::SIGPROF,
        "WINCH" => libc::SIGWINCH,
        "IO" | "POLL" => libc::SIGIO,
        "PWR" => 30,
        "SYS" => 31,
        _ => return Err(format!("invalid signal '{s}'")),
    };
    Ok(n)
}

/// Print the known signal list: one name-per-line table (`-L`) or a
/// single space-joined line of names (`-l`).
fn print_signals(table: bool) {
    let sigs = [
        (1, "HUP"),
        (2, "INT"),
        (3, "QUIT"),
        (9, "KILL"),
        (15, "TERM"),
        (10, "USR1"),
        (12, "USR2"),
        (17, "CHLD"),
        (18, "CONT"),
        (19, "STOP"),
        (13, "PIPE"),
        (14, "ALRM"),
    ];
    if table {
        for (n, name) in sigs {
            println!("{n:>2} {name}");
        }
    } else {
        let names: Vec<_> = sigs.iter().map(|(_, n)| *n).collect();
        println!("{}", names.join(" "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_signal_accepts_numeric() {
        assert_eq!(parse_signal("9"), Ok(9));
        assert_eq!(parse_signal("15"), Ok(15));
    }

    #[test]
    fn parse_signal_accepts_bare_name() {
        assert_eq!(parse_signal("TERM"), Ok(libc::SIGTERM));
        assert_eq!(parse_signal("kill"), Ok(libc::SIGKILL));
    }

    #[test]
    fn parse_signal_accepts_sig_prefixed_name() {
        assert_eq!(parse_signal("SIGTERM"), Ok(libc::SIGTERM));
        assert_eq!(parse_signal("SIGKILL"), Ok(libc::SIGKILL));
    }

    #[test]
    fn parse_signal_rejects_unknown_name() {
        assert!(parse_signal("NOTASIGNAL").is_err());
    }
}
