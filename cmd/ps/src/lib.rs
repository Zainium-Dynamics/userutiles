//! user ps — process snapshot from /proc (common options).
use std::fs;
use std::io::{self, Write};

use usercore::Ui;

/// One output column: its key (`pid`, `pcpu`, ...) and the header to print.
#[derive(Clone)]
struct Col {
    key: &'static str,
    header: String,
}

/// Every column `-o` understands, as (accepted names, canonical key, default header).
const COLUMNS: &[(&[&str], &str, &str)] = &[
    (&["pid", "tgid"], "pid", "PID"),
    (&["ppid"], "ppid", "PPID"),
    (&["pgid", "pgrp"], "pgid", "PGID"),
    (&["sid", "sess", "session"], "sid", "SID"),
    (&["uid", "euid"], "uid", "UID"),
    (&["user", "euser", "uname", "ruser"], "user", "USER"),
    (&["gid", "egid"], "gid", "GID"),
    (&["comm", "ucomm", "ucmd", "fname"], "comm", "COMMAND"),
    (&["args", "cmd", "command"], "args", "COMMAND"),
    (&["stat", "state", "s"], "stat", "STAT"),
    (&["tty", "tt", "tname"], "tty", "TT"),
    (&["etime"], "etime", "ELAPSED"),
    (&["etimes"], "etimes", "ELAPSED"),
    (&["time", "cputime"], "time", "TIME"),
    (&["times", "cputimes"], "times", "TIME"),
    (&["pcpu", "%cpu", "c"], "pcpu", "%CPU"),
    (&["pmem", "%mem"], "pmem", "%MEM"),
    (&["vsz", "vsize"], "vsz", "VSZ"),
    (&["rss", "rssize", "rsz"], "rss", "RSS"),
    (&["nice", "ni"], "nice", "NI"),
    (&["pri", "priority"], "pri", "PRI"),
    (&["nlwp", "thcount"], "nlwp", "NLWP"),
    (&["start", "stime", "start_time"], "start", "START"),
    (&["psr"], "psr", "PSR"),
];

fn column(name: &str) -> Option<(&'static str, &'static str)> {
    let n = name.to_ascii_lowercase();
    COLUMNS
        .iter()
        .find(|(names, _, _)| names.contains(&n.as_str()))
        .map(|(_, k, h)| (*k, *h))
}

/// `-o` lists: comma or space separated; `col=` drops the header, `col=Text` renames it.
/// As in procps, everything after the first `=` belongs to the header.
fn parse_format(spec: &str, cols: &mut Vec<Col>) -> Result<(), String> {
    let mut rest = spec.trim();
    while !rest.is_empty() {
        let (item, tail) = match rest.find([',', ' ']) {
            Some(i) if !rest[..i].contains('=') => (&rest[..i], &rest[i + 1..]),
            _ => (rest, ""),
        };
        let (name, header) = match item.split_once('=') {
            Some((n, h)) => (n, Some(h.to_string())),
            None => (item, None),
        };
        if !name.is_empty() {
            let (key, default) =
                column(name).ok_or_else(|| format!("unknown user-defined format specifier \"{name}\""))?;
            cols.push(Col {
                key,
                header: header.unwrap_or_else(|| default.to_string()),
            });
        }
        rest = tail.trim_start_matches([',', ' ']);
    }
    Ok(())
}

fn cols(keys: &[&str]) -> Vec<Col> {
    keys.iter()
        .map(|k| {
            let (key, h) = column(k).expect("built-in column");
            Col {
                key,
                header: h.to_string(),
            }
        })
        .collect()
}

fn split_list(v: &str) -> impl Iterator<Item = String> + '_ {
    v.split([',', ' ']).filter(|s| !s.is_empty()).map(str::to_string)
}

fn sort_keys(v: &str) -> Vec<(bool, String)> {
    split_list(v)
        .map(|k| match k.strip_prefix('-') {
            Some(k) => (true, k.to_string()),
            None => (false, k.trim_start_matches('+').to_string()),
        })
        .collect()
}

/// An option's value: `--opt=v`, the rest of a short option (`-opid`), or the next argument.
fn opt_value(args: &[String], a: &str, short: &str, long: &str, i: &mut usize) -> Option<String> {
    if let Some(v) = a.strip_prefix(long).and_then(|r| r.strip_prefix('=')) {
        return Some(v.to_string());
    }
    if a.len() > short.len() && a.starts_with(short) && !a.starts_with("--") {
        return Some(a[short.len()..].to_string());
    }
    *i += 1;
    args.get(*i).cloned()
}

const HELP: &str = "Usage: ps [options]
Report a snapshot of the current processes.
 -e, -A          all processes
 -a, -x, ax      all processes
 aux, -u         user-oriented format
 -f              full format
 -p PIDS         only these process ids (comma or space separated)
 -C NAMES        only these command names
 -u, -U USERS    only these users (names or ids)
 -o FORMAT       columns, e.g. -o pid,ppid,etime,args or -o pid= (no header)
 --no-headers    no header line
 --sort KEYS     sort by columns, -key for descending
";

/// Entry point for the `ps` utility: process selection (`-e`/`-A`, `-a`,
/// `-x`, BSD `aux`/`ax`, `-p PIDS`, `-C NAMES`, `-u`/`-U USERS`), the
/// standard, `-f` and user formats, `-o`/`--format` columns (with `col=`
/// header overrides), `--no-headers` and `--sort`. Values come from `/proc`.
///
/// Returns 0 on success, 1 on a usage error or if `/proc` could not be read.
pub fn run() -> i32 {
    let ui = Ui::new("ps");
    let args = usercore::args::expand_short(std::env::args().skip(1), "eAaxfuwhV", "opCU");
    let mut all = false;
    let mut full = false;
    let mut user_fmt = false;
    let mut custom: Vec<Col> = Vec::new();
    let mut headers = true;
    let mut pids: Vec<i32> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut users: Vec<String> = Vec::new();
    let mut sort: Vec<(bool, String)> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        let value = |short: &str, long: &str, i: &mut usize| opt_value(&args, &a, short, long, i);
        let is = |short: &str, long: &str| {
            (a.starts_with(short) && !a.starts_with("--")) || a == long || a.starts_with(&format!("{long}="))
        };
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" | "-V" => {
                println!("ps (user_utils) 0.2.0");
                return 0;
            }
            "--no-headers" | "--no-heading" | "--noheader" | "--noheaders" => headers = false,
            // a bare -u (or BSD u) is the user format; -u USERS selects users
            "-u" if args.get(i + 1).is_some_and(|n| !n.starts_with('-')) => {
                i += 1;
                users.extend(split_list(&args[i]));
            }
            _ if is("-o", "--format") || a == "o" => {
                let Some(v) = value("-o", "--format", &mut i) else {
                    ui.err("option requires an argument -- 'o'");
                    return 1;
                };
                if let Err(e) = parse_format(&v, &mut custom) {
                    ui.err(&e);
                    return 1;
                }
            }
            _ if is("-p", "--pid") => {
                let Some(v) = value("-p", "--pid", &mut i) else {
                    ui.err("option requires an argument -- 'p'");
                    return 1;
                };
                for p in split_list(&v) {
                    match p.parse() {
                        Ok(n) => pids.push(n),
                        Err(_) => {
                            ui.err(&format!("invalid process id: {p}"));
                            return 1;
                        }
                    }
                }
            }
            _ if is("-C", "--command") => {
                let Some(v) = value("-C", "--command", &mut i) else {
                    ui.err("option requires an argument -- 'C'");
                    return 1;
                };
                names.extend(split_list(&v));
            }
            _ if is("-U", "--user") => {
                let Some(v) = value("-U", "--user", &mut i) else {
                    ui.err("option requires an argument -- 'U'");
                    return 1;
                };
                users.extend(split_list(&v));
            }
            _ if a == "--sort" || a.starts_with("--sort=") => {
                let Some(v) = value("--sort", "--sort", &mut i) else {
                    ui.err("option '--sort' requires an argument");
                    return 1;
                };
                sort.extend(sort_keys(&v));
            }
            s if s.starts_with("--") => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            s => {
                // clusters: -ef, -aux, aux, ax
                let flags = s.strip_prefix('-').unwrap_or(s);
                if flags.is_empty() || !flags.chars().all(|c| "auxfeAw".contains(c)) {
                    ui.err(&format!("unrecognized option '{s}'"));
                    return 1;
                }
                for c in flags.chars() {
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
        }
        i += 1;
    }

    let mut procs = match list_procs() {
        Ok(v) => v,
        Err(e) => {
            ui.err(&format!("/proc: {e}"));
            return 1;
        }
    };
    let my_tty = read_self_tty();
    let selected = !pids.is_empty() || !names.is_empty() || !users.is_empty();
    procs.retain(|p| {
        if selected {
            pids.contains(&p.pid)
                || names.iter().any(|n| *n == p.comm)
                || users
                    .iter()
                    .any(|u| *u == p.user || u.parse::<u32>().is_ok_and(|n| n == p.uid))
        } else {
            // default: same tty as caller
            all || p.tty == my_tty || my_tty == "?"
        }
    });

    let columns = if !custom.is_empty() {
        custom
    } else if user_fmt {
        cols(&["user", "pid", "pcpu", "pmem", "vsz", "rss", "tty", "stat", "start", "time", "args"])
    } else if full {
        let mut c = cols(&["user", "pid", "ppid", "c", "stime", "tty", "time", "args"]);
        c[0].header = "UID".into();
        c[3].header = "C".into();
        c[4].header = "STIME".into();
        c[5].header = "TTY".into();
        c[7].header = "CMD".into();
        c
    } else {
        let mut c = cols(&["pid", "tty", "time", "comm"]);
        c[1].header = "TTY".into();
        c[3].header = "CMD".into();
        c
    };

    let clock = Clock::now();
    procs.sort_by_key(|p| p.pid);
    if !sort.is_empty() {
        let keys: Vec<(bool, &'static str)> = sort
            .iter()
            .filter_map(|(desc, k)| column(k).map(|(key, _)| (*desc, key)))
            .collect();
        procs.sort_by(|a, b| {
            for (desc, key) in &keys {
                let o = sort_value(a, key, &clock).cmp(&sort_value(b, key, &clock));
                let o = if *desc { o.reverse() } else { o };
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    let rows: Vec<Vec<String>> = procs
        .iter()
        .map(|p| columns.iter().map(|c| value(p, c.key, &clock)).collect())
        .collect();
    let print_header = headers && columns.iter().any(|c| !c.header.is_empty());
    let mut widths: Vec<usize> = columns
        .iter()
        .map(|c| if print_header { c.header.chars().count() } else { 0 })
        .collect();
    for r in &rows {
        for (w, v) in widths.iter_mut().zip(r) {
            *w = (*w).max(v.chars().count());
        }
    }
    let line = |vals: &[String]| -> String {
        let last = vals.len().saturating_sub(1);
        let mut s = String::new();
        for (n, (v, c)) in vals.iter().zip(&columns).enumerate() {
            if n > 0 {
                s.push(' ');
            }
            let left = matches!(c.key, "user" | "comm" | "args" | "stat" | "tty" | "start");
            if n == last && left {
                s.push_str(v);
            } else if left {
                s.push_str(&format!("{v:<w$}", w = widths[n]));
            } else {
                s.push_str(&format!("{v:>w$}", w = widths[n]));
            }
        }
        s.trim_end().to_string()
    };
    let mut out = io::stdout().lock();
    if print_header {
        let h: Vec<String> = columns.iter().map(|c| c.header.clone()).collect();
        let _ = writeln!(out, "{}", line(&h));
    }
    for r in &rows {
        let _ = writeln!(out, "{}", line(r));
    }
    0
}

/// What every time- and memory-based column is relative to.
struct Clock {
    uptime: f64,
    hz: f64,
    now: i64,
    mem_kb: f64,
}

impl Clock {
    fn now() -> Self {
        let uptime = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
            .unwrap_or(0.0);
        // SAFETY: sysconf only reads a configuration value.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
        let mem_kb = fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
            })
            .unwrap_or(0.0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Clock {
            uptime,
            hz,
            now,
            mem_kb,
        }
    }

    fn cpu_secs(&self, p: &Proc) -> f64 {
        (p.utime + p.stime) as f64 / self.hz
    }

    fn elapsed(&self, p: &Proc) -> f64 {
        (self.uptime - p.start_ticks as f64 / self.hz).max(0.0)
    }

    fn pcpu(&self, p: &Proc) -> f64 {
        let e = self.elapsed(p);
        if e > 0.0 {
            100.0 * self.cpu_secs(p) / e
        } else {
            0.0
        }
    }

    fn pmem(&self, p: &Proc) -> f64 {
        if self.mem_kb > 0.0 {
            100.0 * p.rss as f64 / self.mem_kb
        } else {
            0.0
        }
    }

    fn started(&self, p: &Proc) -> i64 {
        self.now - self.elapsed(p) as i64
    }
}

/// `[[dd-]hh:]mm:ss`, as procps prints elapsed time.
fn fmt_etime(secs: f64) -> String {
    let s = secs as u64;
    let (d, h, m, s) = (s / 86400, s % 86400 / 3600, s % 3600 / 60, s % 60);
    match (d, h) {
        (0, 0) => format!("{m:02}:{s:02}"),
        (0, h) => format!("{h:02}:{m:02}:{s:02}"),
        (d, h) => format!("{d}-{h:02}:{m:02}:{s:02}"),
    }
}

/// `[dd-]hh:mm:ss` cumulative CPU time.
fn fmt_cputime(secs: f64) -> String {
    let s = secs as u64;
    let (d, h, m, s) = (s / 86400, s % 86400 / 3600, s % 3600 / 60, s % 60);
    if d > 0 {
        format!("{d}-{h:02}:{m:02}:{s:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

/// HH:MM for a start today, else the month and day.
fn fmt_start(epoch: i64, now: i64) -> String {
    let local = |t: i64| -> libc::tm {
        let t = t as libc::time_t;
        // SAFETY: localtime_r only writes the tm we pass it.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&t, &mut tm) };
        tm
    };
    let (s, n) = (local(epoch), local(now));
    if s.tm_year == n.tm_year && s.tm_yday == n.tm_yday {
        format!("{:02}:{:02}", s.tm_hour, s.tm_min)
    } else {
        const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        format!("{}{:02}", MON[s.tm_mon.clamp(0, 11) as usize], s.tm_mday)
    }
}

fn value(p: &Proc, key: &str, c: &Clock) -> String {
    match key {
        "pid" => p.pid.to_string(),
        "ppid" => p.ppid.to_string(),
        "pgid" => p.pgid.to_string(),
        "sid" => p.sid.to_string(),
        "uid" => p.uid.to_string(),
        "user" => p.user.clone(),
        "gid" => p.gid.to_string(),
        "comm" => p.comm.clone(),
        "args" => p.cmd.clone(),
        "stat" => p.state.clone(),
        "tty" => p.tty.clone(),
        "etime" => fmt_etime(c.elapsed(p)),
        "etimes" => (c.elapsed(p) as u64).to_string(),
        "time" => fmt_cputime(c.cpu_secs(p)),
        "times" => (c.cpu_secs(p) as u64).to_string(),
        "pcpu" => format!("{:.1}", c.pcpu(p)),
        "pmem" => format!("{:.1}", c.pmem(p)),
        "vsz" => (p.vsz / 1024).to_string(),
        "rss" => p.rss.to_string(),
        "nice" => p.nice.to_string(),
        "pri" => p.pri.to_string(),
        "nlwp" => p.nlwp.to_string(),
        "start" => fmt_start(c.started(p), c.now),
        "psr" => p.psr.to_string(),
        _ => "?".into(),
    }
}

/// Numbers sort as numbers, everything else as text.
fn sort_value(p: &Proc, key: &str, c: &Clock) -> (i64, String) {
    let n = match key {
        "pid" => p.pid as i64,
        "ppid" => p.ppid as i64,
        "pgid" => p.pgid as i64,
        "sid" => p.sid as i64,
        "uid" => p.uid as i64,
        "gid" => p.gid as i64,
        "etime" | "etimes" => c.elapsed(p) as i64,
        "time" | "times" => (c.cpu_secs(p) * 100.0) as i64,
        "pcpu" => (c.pcpu(p) * 10.0) as i64,
        "pmem" => (c.pmem(p) * 10.0) as i64,
        "vsz" => p.vsz as i64,
        "rss" => p.rss as i64,
        "nice" => p.nice,
        "pri" => p.pri,
        "nlwp" => p.nlwp as i64,
        "start" => c.started(p),
        "psr" => p.psr as i64,
        _ => return (0, value(p, key, c)),
    };
    (n, String::new())
}

struct Proc {
    pid: i32,
    ppid: i32,
    pgid: i32,
    sid: i32,
    uid: u32,
    gid: u32,
    user: String,
    comm: String,
    cmd: String,
    state: String,
    tty: String,
    vsz: u64,
    rss: u64,
    utime: u64,
    stime: u64,
    start_ticks: u64,
    nice: i64,
    pri: i64,
    nlwp: u64,
    psr: u64,
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
        let comm = stat
            .find('(')
            .zip(stat.rfind(')'))
            .and_then(|(l, r)| stat.get(l + 1..r))
            .unwrap_or("?")
            .to_string();
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
        let more = parse_stat_fields(&stat);
        let id = |key: &str| -> u32 {
            status
                .lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|u| u.parse().ok())
                .unwrap_or(0)
        };
        let uid = id("Uid:");
        let gid = id("Gid:");
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
            pgid: more.pgid,
            sid: more.sid,
            uid,
            gid,
            user: uid_name(uid),
            comm,
            cmd,
            state: state.to_string(),
            tty: tty_name(tty_nr),
            vsz,
            rss,
            utime: more.utime,
            stime: more.stime,
            start_ticks: more.start,
            nice: more.nice,
            pri: more.pri,
            nlwp: more.nlwp,
            psr: more.psr,
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

#[derive(Default)]
struct StatFields {
    pgid: i32,
    sid: i32,
    utime: u64,
    stime: u64,
    pri: i64,
    nice: i64,
    nlwp: u64,
    start: u64,
    psr: u64,
}

/// The `/proc/<pid>/stat` fields after `(comm)` that the columns need, by
/// proc(5) number: pgrp 5, session 6, utime 14, stime 15, priority 18,
/// nice 19, num_threads 20, starttime 22, processor 39.
fn parse_stat_fields(stat: &str) -> StatFields {
    let rest = stat.rfind(')').and_then(|r| stat.get(r + 2..)).unwrap_or("");
    let f: Vec<&str> = rest.split_whitespace().collect();
    // f[0] is field 3 (state)
    let num = |n: usize| f.get(n - 3).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    StatFields {
        pgid: num(5) as i32,
        sid: num(6) as i32,
        utime: num(14) as u64,
        stime: num(15) as u64,
        pri: num(18),
        nice: num(19),
        nlwp: num(20) as u64,
        start: num(22) as u64,
        psr: num(39) as u64,
    }
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

/// Resolve `uid` to a user name from Zainium's passwd file, falling back to
/// the numeric id (stringified) if no matching entry exists.
fn uid_name(uid: u32) -> String {
    usercore::nss::user_by_uid(uid).map_or_else(|| uid.to_string(), |pw| pw.name)
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
    fn format_lists_and_header_overrides() {
        let mut c = Vec::new();
        parse_format("pid,ppid etime", &mut c).unwrap();
        assert_eq!(c.iter().map(|c| c.key).collect::<Vec<_>>(), ["pid", "ppid", "etime"]);
        let mut c = Vec::new();
        parse_format("pid=", &mut c).unwrap();
        assert_eq!((c[0].key, c[0].header.as_str()), ("pid", ""));
        let mut c = Vec::new();
        parse_format("comm=Program Name", &mut c).unwrap();
        assert_eq!(c[0].header, "Program Name");
        assert!(parse_format("bogus", &mut Vec::new()).is_err());
    }

    #[test]
    fn stat_fields_by_proc5_number() {
        let line = "42 (a b) S 1 5 6 0 -1 0 0 0 0 0 150 50 0 0 20 0 3 0 1234 0 0";
        let f = parse_stat_fields(line);
        assert_eq!(
            (f.pgid, f.sid, f.utime, f.stime, f.pri, f.nice, f.nlwp, f.start),
            (5, 6, 150, 50, 20, 0, 3, 1234)
        );
    }

    #[test]
    fn times_format_like_procps() {
        assert_eq!(fmt_etime(65.0), "01:05");
        assert_eq!(fmt_etime(3725.0), "01:02:05");
        assert_eq!(fmt_etime(90061.0), "1-01:01:01");
        assert_eq!(fmt_cputime(65.0), "00:01:05");
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
