//! user findmnt — list or search mounted filesystems.
use std::fs;

use usercore::Ui;

struct Mount {
    source: String,
    target: String,
    fstype: String,
    options: String,
}

/// Undo the kernel's octal escapes (`\040` space, `\011` tab, `\012` newline, `\134` backslash).
fn unescape(f: &str) -> String {
    let b = f.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse `/proc/self/mounts`-format lines: `source target fstype options dump pass`.
fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some(Mount {
                source: unescape(f.next()?),
                target: unescape(f.next()?),
                fstype: f.next()?.to_string(),
                options: f.next()?.to_string(),
            })
        })
        .collect()
}

/// The mount whose target is the longest prefix of `path` — i.e. the
/// filesystem `path` actually resides on.
fn find_covering<'a>(mounts: &'a [Mount], path: &str) -> Option<&'a Mount> {
    mounts
        .iter()
        .filter(|m| {
            let prefix = if m.target == "/" {
                "/".to_string()
            } else {
                format!("{}/", m.target)
            };
            path == m.target || path.starts_with(&prefix)
        })
        .max_by_key(|m| m.target.len())
}

const COLUMNS: &[&str] = &["TARGET", "SOURCE", "FSTYPE", "OPTIONS", "SIZE", "USED", "AVAIL", "USE%"];

fn print_help() {
    print!(
        "Usage: findmnt [OPTION]... [SOURCE|TARGET]\n\
 List or search mounted filesystems, from /proc/self/mounts.\n\
 -t, --types LIST      only these filesystem types (comma list, \"no\" prefix negates)\n\
 -S, --source SRC      only this mount source\n\
 -T, --target PATH     the filesystem PATH resides on\n\
 -M, --mountpoint DIR  only the mount at exactly DIR\n\
 -O, --options LIST    only mounts with these options\n\
 -o, --output LIST     columns: TARGET,SOURCE,FSTYPE,OPTIONS,SIZE,USED,AVAIL,USE%\n\
 -D, --df              df-like output\n\
 -n, --noheadings      no header line\n\
 -r, --raw             single-space separated\n\
 -P, --pairs           KEY=\"value\" output\n\
 -f, --first-only      print the first match only\n\
 --help / --version\n"
    );
}

fn df(target: &str) -> Option<(u64, u64, u64)> {
    let c = std::ffi::CString::new(target).ok()?;
    // SAFETY: valid C string, zeroed out-buffer read only on success.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let bs = st.f_frsize as u64;
    let size = st.f_blocks as u64 * bs;
    let avail = st.f_bavail as u64 * bs;
    let used = size.saturating_sub(st.f_bfree as u64 * bs);
    Some((size, used, avail))
}

fn human(n: u64) -> String {
    let units = ["B", "K", "M", "G", "T", "P"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{n}B") } else if v < 10.0 { format!("{v:.1}{}", units[u]) } else { format!("{v:.0}{}", units[u]) }
}

fn cell(m: &Mount, col: &str) -> String {
    match col {
        "TARGET" => m.target.clone(),
        "SOURCE" => m.source.clone(),
        "FSTYPE" => m.fstype.clone(),
        "OPTIONS" => m.options.clone(),
        "SIZE" | "USED" | "AVAIL" | "USE%" => match df(&m.target) {
            Some((size, used, avail)) => match col {
                "SIZE" => human(size),
                "USED" => human(used),
                "AVAIL" => human(avail),
                _ => {
                    if size == 0 { "-".into() } else { format!("{}%", (used * 100).div_ceil(used + avail).min(100)) }
                }
            },
            None => "-".into(),
        },
        _ => String::new(),
    }
}

fn types_match(list: &str, t: &str) -> bool {
    let neg = list.starts_with("no");
    let items: Vec<&str> = list.split(',').map(|x| x.trim_start_matches("no")).collect();
    items.contains(&t) != neg
}

/// Entry point: list or look up mounted filesystems from `/proc/self/mounts`.
/// Returns 0 when something was printed, 1 otherwise.
pub fn run() -> i32 {
    // SAFETY: restoring the default SIGPIPE action has no preconditions.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let ui = Ui::new("findmnt");
    // split combined flags such as -rn into -r -n
    let args: Vec<String> = std::env::args()
        .skip(1)
        .flat_map(|a| {
            if a.len() > 2 && a.starts_with('-') && !a.starts_with("--") && a[1..].chars().all(|c| "nrPfDlkmvAu".contains(c)) {
                a[1..].chars().map(|c| format!("-{c}")).collect()
            } else {
                vec![a]
            }
        })
        .collect();

    let mut fstype: Option<String> = None;
    let mut source: Option<String> = None;
    let mut target: Option<String> = None;
    let mut mountpoint: Option<String> = None;
    let mut options: Option<String> = None;
    let mut columns: Vec<String> = ["TARGET", "SOURCE", "FSTYPE", "OPTIONS"].map(String::from).to_vec();
    let mut headings = true;
    let mut raw = false;
    let mut pairs = false;
    let mut first = false;
    let mut positional: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        let (key, inline) = match a.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k.to_string(), Some(v.to_string())),
            _ => (a.clone(), None),
        };
        let mut val = |name: &str| -> Option<String> {
            if let Some(v) = &inline {
                return Some(v.clone());
            }
            if key.len() > 2 && !key.starts_with("--") {
                return Some(key[2..].to_string());
            }
            i += 1;
            let v = args.get(i).cloned();
            if v.is_none() {
                ui.err(&format!("option requires an argument -- '{name}'"));
            }
            v
        };
        let short = |c: char| key.starts_with(&format!("-{c}")) && !key.starts_with("--");
        match key.as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" | "-V" => {
                println!("findmnt (user_utils) 0.2.0");
                return 0;
            }
            "-n" | "--noheadings" => headings = false,
            "-r" | "--raw" => raw = true,
            "-P" | "--pairs" => pairs = true,
            "-f" | "--first-only" => first = true,
            "-D" | "--df" => columns = ["SOURCE", "FSTYPE", "SIZE", "USED", "AVAIL", "USE%", "TARGET"].map(String::from).to_vec(),
            "-l" | "--list" | "-k" | "--kernel" | "-m" | "--mtab" | "-v" | "--nofsroot" | "-A" | "--all" | "-u" | "--notruncate" => {}
            k if k == "--types" || short('t') => match val("t") { Some(v) => fstype = Some(v), None => return 1 },
            k if k == "--source" || short('S') => match val("S") { Some(v) => source = Some(v), None => return 1 },
            k if k == "--target" || short('T') => match val("T") { Some(v) => target = Some(v), None => return 1 },
            k if k == "--mountpoint" || short('M') => match val("M") { Some(v) => mountpoint = Some(v), None => return 1 },
            k if k == "--options" || short('O') => match val("O") { Some(v) => options = Some(v), None => return 1 },
            k if k == "--output" || short('o') => match val("o") {
                Some(v) => {
                    columns = v.split(',').map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()).collect();
                    if let Some(bad) = columns.iter().find(|c| !COLUMNS.contains(&c.as_str())) {
                        ui.err(&format!("unknown column: {bad}"));
                        return 1;
                    }
                }
                None => return 1,
            },
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => positional = Some(other.to_string()),
        }
        i += 1;
    }

    let text = match fs::read_to_string("/proc/self/mounts") {
        Ok(t) => t,
        Err(e) => {
            ui.err(&format!("/proc/self/mounts: {e}"));
            return 1;
        }
    };
    let mounts = parse_mounts(&text);

    // a bare argument is a source if it names one, else a target path
    if let Some(p) = positional {
        if mounts.iter().any(|m| m.source == p) {
            source = Some(p);
        } else {
            target = Some(p);
        }
    }

    let mut rows: Vec<&Mount> = match &target {
        Some(t) => {
            let abs = std::fs::canonicalize(t).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| t.clone());
            find_covering(&mounts, &abs).into_iter().collect()
        }
        None => mounts.iter().collect(),
    };
    rows.retain(|m| fstype.as_deref().is_none_or(|t| types_match(t, &m.fstype)));
    rows.retain(|m| source.as_deref().is_none_or(|s| s == m.source));
    rows.retain(|m| mountpoint.as_deref().is_none_or(|p| p.trim_end_matches('/') == m.target.trim_end_matches('/') || (p == "/" && m.target == "/")));
    rows.retain(|m| {
        options.as_deref().is_none_or(|o| {
            let have: Vec<&str> = m.options.split(',').collect();
            o.split(',').all(|want| match want.strip_prefix("no") {
                Some(n) if !have.contains(&want) => !have.contains(&n),
                _ => have.contains(&want),
            })
        })
    });
    if first {
        rows.truncate(1);
    }
    if rows.is_empty() {
        return 1;
    }

    let table: Vec<Vec<String>> = rows.iter().map(|m| columns.iter().map(|c| cell(m, c)).collect()).collect();
    let mut out = String::new();
    if pairs {
        for r in &table {
            let line: Vec<String> = columns.iter().zip(r).map(|(c, v)| format!("{}=\"{}\"", c.replace('%', "_"), v.replace('"', "\\\""))).collect();
            out.push_str(&line.join(" "));
            out.push('\n');
        }
    } else {
        let widths: Vec<usize> = (0..columns.len())
            .map(|k| table.iter().map(|r| r[k].chars().count()).chain(std::iter::once(if headings { columns[k].len() } else { 0 })).max().unwrap_or(0))
            .collect();
        let mut line = |cells: &[String]| {
            let last = cells.len().saturating_sub(1);
            let parts: Vec<String> = cells
                .iter()
                .enumerate()
                .map(|(k, c)| if raw || k == last { c.clone() } else { format!("{c:<w$}", w = widths[k]) })
                .collect();
            out.push_str(&parts.join(" "));
            out.push('\n');
        };
        if headings {
            line(&columns);
        }
        for r in &table {
            line(r);
        }
    }
    if let Err(e) = usercore::ui::write_stdout(out.as_bytes()) {
        ui.err(&format!("{e}"));
        return 1;
    }
    match usercore::ui::flush_stdout() {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!("{e}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Mount> {
        parse_mounts(
            "\
/dev/sda1 / ext4 rw,relatime 0 0
tmpfs /dev/shm tmpfs rw,nosuid,nodev 0 0
/dev/sda2 /home ext4 rw,relatime 0 0
",
        )
    }

    #[test]
    fn parse_mounts_reads_all_fields() {
        let m = sample();
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].source, "/dev/sda1");
        assert_eq!(m[0].target, "/");
        assert_eq!(m[0].fstype, "ext4");
        assert_eq!(m[0].options, "rw,relatime");
    }

    #[test]
    fn find_covering_picks_the_longest_matching_prefix() {
        let m = sample();
        let hit = find_covering(&m, "/home/user/file.txt").unwrap();
        assert_eq!(hit.target, "/home");
    }

    #[test]
    fn find_covering_falls_back_to_root() {
        let m = sample();
        let hit = find_covering(&m, "/etc/passwd").unwrap();
        assert_eq!(hit.target, "/");
    }

    #[test]
    fn escapes_and_type_lists() {
        let m = parse_mounts("/dev/x /mnt/My\\040Drive vfat rw 0 0\n");
        assert_eq!(m[0].target, "/mnt/My Drive");
        assert!(types_match("ext4,btrfs", "btrfs"));
        assert!(!types_match("notmpfs,proc", "tmpfs"));
        assert!(types_match("notmpfs", "ext4"));
    }

    #[test]
    fn find_covering_none_for_no_mounts() {
        assert!(find_covering(&[], "/anything").is_none());
    }
}
