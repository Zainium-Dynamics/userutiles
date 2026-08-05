//! user ps — process snapshot from /proc (common options).
use std::fs;
use std::io::{self, Write};

use usercore::Ui;

/// Entry point for the `ps` utility. Parses `std::env::args()` for a subset
/// of common BSD/GNU `ps` option spellings (`-e`/`-A`, `-a`, `-x`, `-f`,
/// `-u`, and the unhyphenated `aux`/`ax` forms) and prints a snapshot of
/// running processes read from `/proc`.
///
/// Returns 0 on success, 1 if `/proc` could not be read.
pub fn run() -> i32 {
    let ui = Ui::new("ps");
    let mut all = false;
    let mut full = false;
    let mut user_fmt = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: ps [OPTION]\nReport process status.\n -e, -A all processes\n -f full format\n -a all with tty except session leaders (approx all)\n -u user-oriented format\n -x include processes without controlling ttys\n");
                return 0;
            }
            "--version" => {
                println!("ps (user_utils) 0.1.0");
                return 0;
            }
            "-e" | "-A" | "-ax" | "ax" | "-a" | "a" | "-x" | "x" => all = true,
            "-f" | "f" => full = true,
            "-u" | "u" | "-uf" => {
                user_fmt = true;
                all = true;
            }
            "aux" | "-aux" => {
                all = true;
                user_fmt = true;
            }
            s if s.starts_with('-') => {
                for c in s.chars().skip(1) {
                    match c {
                        'e' | 'A' | 'a' | 'x' => all = true,
                        'f' => full = true,
                        'u' => {
                            user_fmt = true;
                            all = true;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let mut procs = match list_procs() {
        Ok(v) => v,
        Err(e) => {
            ui.err(&format!("/proc: {e}"));
            return 1;
        }
    };
    procs.sort_by_key(|p| p.pid);
    let mut out = io::stdout().lock();
    if user_fmt {
        let _ = writeln!(
            out,
            "USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND"
        );
    } else if full {
        let _ = writeln!(out, "UID PID PPID C STIME TTY TIME CMD");
    } else {
        let _ = writeln!(out, " PID TTY TIME CMD");
    }
    let my_tty = read_self_tty();
    for p in procs {
        if !all {
            // default: same tty as caller
            if p.tty != my_tty && my_tty != "?" {
                continue;
            }
        }
        if user_fmt {
            let _ = writeln!(
                out,
                "{:<8} {:>5} 0.0 0.0 {:>6} {:>5} {:<8} {:<4} {:<6} {:>8} {}",
                p.user,
                p.pid,
                p.vsz / 1024,
                p.rss,
                p.tty,
                p.state,
                "?",
                "0:00",
                p.cmd
            );
        } else if full {
            let _ = writeln!(
                out,
                "{:<8} {:>5} {:>5} 0 {:<5} {:<8} {:>8} {}",
                p.uid, p.pid, p.ppid, "?", p.tty, "00:00:00", p.cmd
            );
        } else {
            let _ = writeln!(out, "{:>5} {:<8} {:>8} {}", p.pid, p.tty, "00:00:00", p.cmd);
        }
    }
    0
}

struct Proc {
    pid: i32,
    ppid: i32,
    uid: u32,
    user: String,
    cmd: String,
    state: String,
    tty: String,
    vsz: u64,
    rss: u64,
}

/// Enumerate every numerically-named entry of `/proc` (i.e. every process)
/// and read its `stat`/`status`/`cmdline` files into a [`Proc`] snapshot.
/// Per-process files that vanish mid-read (the process exited) or are
/// malformed simply yield default/placeholder fields for that process
/// rather than failing the whole listing — this mirrors real `ps`, which
/// tolerates processes disappearing during a scan.
///
/// Only a failure to open `/proc` itself is treated as fatal.
fn list_procs() -> io::Result<Vec<Proc>> {
    let mut v = Vec::new();
    for ent in fs::read_dir("/proc")?.flatten() {
        let name = ent.file_name();
        let s = name.to_string_lossy();
        if !s.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = s.parse::<i32>() else {
            continue;
        };
        let path = ent.path();
        let stat = fs::read_to_string(path.join("stat")).unwrap_or_default();
        let status = fs::read_to_string(path.join("status")).unwrap_or_default();
        let cmdline = fs::read(path.join("cmdline")).unwrap_or_default();
        let cmd = if cmdline.is_empty() {
            // kernel thread
            let comm = status
                .lines()
                .find(|l| l.starts_with("Name:"))
                .map(|l| l[5..].trim())
                .unwrap_or("?");
            format!("[{comm}]")
        } else {
            String::from_utf8_lossy(&cmdline)
                .replace('\0', " ")
                .trim()
                .to_string()
        };
        let (ppid, state, tty_nr) = parse_stat(&stat);
        let uid = status
            .lines()
            .find(|l| l.starts_with("Uid:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|u| u.parse().ok())
            .unwrap_or(0);
        let (vsz, rss) = {
            let mut vsz = 0u64;
            let mut rss = 0u64;
            for l in status.lines() {
                if l.starts_with("VmSize:") {
                    vsz = l
                        .split_whitespace()
                        .nth(1)
                        .and_then(|x| x.parse().ok())
                        .unwrap_or(0)
                        * 1024;
                }
                if l.starts_with("VmRSS:") {
                    rss = l
                        .split_whitespace()
                        .nth(1)
                        .and_then(|x| x.parse().ok())
                        .unwrap_or(0);
                }
            }
            (vsz, rss)
        };
        v.push(Proc {
            pid,
            ppid,
            uid,
            user: uid_name(uid),
            cmd,
            state: state.to_string(),
            tty: tty_name(tty_nr),
            vsz,
            rss,
        });
    }
    Ok(v)
}

/// Parse the fields of a `/proc/<pid>/stat` line that follow `pid (comm)`,
/// returning `(ppid, state, tty_nr)`. `comm` (the second field) is skipped
/// by scanning for the *last* `)` rather than the first, since the process
/// name itself may legitimately contain `)` characters.
fn parse_stat(stat: &str) -> (i32, char, i32) {
    // pid (comm) state ppid ... tty_nr
    let rparen = stat.rfind(')').unwrap_or(0);
    let rest = stat.get(rparen + 2..).unwrap_or("");
    let mut it = rest.split_whitespace();
    let state = it.next().and_then(|s| s.chars().next()).unwrap_or('?');
    let ppid: i32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let _pgrp = it.next();
    let _sid = it.next();
    let tty_nr: i32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (ppid, state, tty_nr)
}

/// Decode a kernel `tty_nr` (as found in `/proc/<pid>/stat`) into a display
/// name like `tty4` or `pts/2`, or `"?"` if there is no controlling
/// terminal (`tty_nr == 0`) or the major number isn't one this simplified
/// implementation recognizes.
fn tty_name(nr: i32) -> String {
    if nr == 0 {
        return "?".into();
    }
    // major minor from kernel encoding
    let major = (nr >> 8) & 0xff;
    let minor = (nr & 0xff) | ((nr >> 12) & 0xfff00);
    if major == 4 {
        format!("tty{minor}")
    } else if major == 136 {
        format!("pts/{minor}")
    } else {
        format!("tty?{nr}")
    }
}

/// Resolve `uid` to a username via `getpwuid(3)`, falling back to the
/// decimal uid itself if there is no matching passwd entry.
fn uid_name(uid: u32) -> String {
    // SAFETY: `getpwuid` takes a plain integer and returns either a null
    // pointer (checked below before use) or a pointer into an internal
    // static buffer owned by libc that stays valid until the next call
    // to `getpwuid`/`getpwnam`/etc. on this thread. We only read from it
    // once, synchronously, before any other libc passwd-database call can
    // invalidate it. `(*pw).pw_name` is a non-null, NUL-terminated C
    // string owned by that same static buffer, so `CStr::from_ptr` on it
    // is valid, and `to_string_lossy().into_owned()` copies the data out
    // before the buffer could be reused.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            uid.to_string()
        } else {
            std::ffi::CStr::from_ptr((*pw).pw_name)
                .to_string_lossy()
                .into_owned()
        }
    }
}

/// Return the display name of the calling process's controlling terminal
/// (see [`tty_name`]), or `"?"` if `/proc/self/stat` can't be read.
fn read_self_tty() -> String {
    let stat = fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let (_, _, tty) = parse_stat(&stat);
    tty_name(tty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stat_extracts_ppid_state_tty() {
        // Realistic line, comm containing a space to exercise the
        // rfind(')')-based skip past `(comm)`.
        let line = "123 (my proc) S 1 456 456 34816 -1 4194560 ...";
        let (ppid, state, tty_nr) = parse_stat(line);
        assert_eq!(ppid, 1);
        assert_eq!(state, 'S');
        assert_eq!(tty_nr, 34816);
    }

    #[test]
    fn parse_stat_comm_with_close_paren_uses_last_paren() {
        let line = "7 (weird)name) R 3 0 0 0 -1 0";
        let (ppid, state, _tty) = parse_stat(line);
        assert_eq!(ppid, 3);
        assert_eq!(state, 'R');
    }

    #[test]
    fn parse_stat_empty_input_does_not_panic() {
        // No `)` at all: `rfind` yields `None`, treated as position 0, and
        // slicing from `0 + 2` on an empty string is out of range and
        // falls back to `""` — must not panic, and every field defaults.
        let (ppid, state, tty_nr) = parse_stat("");
        assert_eq!(ppid, 0);
        assert_eq!(state, '?');
        assert_eq!(tty_nr, 0);
    }

    #[test]
    fn parse_stat_no_close_paren_does_not_panic() {
        // Still no `)`, but long enough that `2..` is in-bounds; must not
        // panic even though there's no real `(comm)` field to skip.
        let (ppid, _state, tty_nr) = parse_stat("garbage with no parens");
        assert_eq!(ppid, 0);
        assert_eq!(tty_nr, 0);
    }

    #[test]
    fn tty_name_zero_is_question_mark() {
        assert_eq!(tty_name(0), "?");
    }

    #[test]
    fn tty_name_decodes_pts_major() {
        // major 136, minor 5 -> pts/5
        let nr = (136 << 8) | 5;
        assert_eq!(tty_name(nr), "pts/5");
    }

    #[test]
    fn tty_name_decodes_tty_major() {
        let nr = (4 << 8) | 2;
        assert_eq!(tty_name(nr), "tty2");
    }

    #[test]
    fn uid_name_root_is_root() {
        // uid 0 is always "root" on any real Linux system.
        assert_eq!(uid_name(0), "root");
    }

    #[test]
    fn list_procs_includes_self() {
        let procs = list_procs().expect("list /proc");
        let mypid = std::process::id() as i32;
        assert!(procs.iter().any(|p| p.pid == mypid));
    }

    #[test]
    fn read_self_tty_does_not_panic() {
        // No assertion on the value (varies: "?" when there's no
        // controlling terminal, e.g. under a test harness), just that it
        // runs and returns a string.
        let _ = read_self_tty();
    }
}
