//! user findmnt — list or search mounted filesystems.
use std::fs;

use usercore::Ui;

struct Mount {
    source: String,
    target: String,
    fstype: String,
    options: String,
}

/// Parse `/proc/self/mounts`-format lines: `source target fstype options
/// dump pass`. `\040`/`\011`/etc octal escapes (the kernel's encoding for
/// spaces/tabs embedded in a path) are left as-is — rare in practice and
/// not worth the complexity for a first pass.
fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some(Mount {
                source: f.next()?.to_string(),
                target: f.next()?.to_string(),
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

fn print_help() {
    print!(
        "Usage: findmnt [-t fstype] [-S source] [-n] [TARGET]\n\
 List or search mounted filesystems, from /proc/self/mounts.\n\
 With TARGET, show the filesystem that path resides on.\n\
 -t, --types fstype only show this filesystem type\n\
 -S, --source src only show this mount source\n\
 -n, --noheadings don't print the column header\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `findmnt` utility. Parses `std::env::args()` and
/// lists (or, given `TARGET`, looks up) mounted filesystems from
/// `/proc/self/mounts` as a `TARGET SOURCE FSTYPE OPTIONS` table.
///
/// Returns 0 on a successful lookup/listing, 1 if `/proc/self/mounts`
/// couldn't be read or (given `TARGET`) nothing matched.
pub fn run() -> i32 {
    let ui = Ui::new("findmnt");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut fstype: Option<String> = None;
    let mut source: Option<String> = None;
    let mut headings = true;
    let mut target: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("findmnt (user_utils) 0.1.0");
                return 0;
            }
            "-n" | "--noheadings" => headings = false,
            "-t" | "--types" => {
                i += 1;
                match args.get(i) {
                    Some(v) => fstype = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 't'");
                        return 1;
                    }
                }
            }
            "-T" | "-S" | "--source" => {
                i += 1;
                match args.get(i) {
                    Some(v) => source = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument");
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => target = Some(other.to_string()),
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

    let rows: Vec<&Mount> = if let Some(t) = &target {
        match find_covering(&mounts, t) {
            Some(m) => vec![m],
            None => {
                ui.err(&format!("{t}: not found"));
                return 1;
            }
        }
    } else {
        mounts
            .iter()
            .filter(|m| fstype.as_deref().map_or(true, |t| t == m.fstype))
            .filter(|m| source.as_deref().map_or(true, |s| s == m.source))
            .collect()
    };

    let mut out = String::new();
    if headings {
        out.push_str("TARGET SOURCE FSTYPE OPTIONS\n");
    }
    for m in &rows {
        out.push_str(&format!(
            "{} {} {} {}\n",
            m.target, m.source, m.fstype, m.options
        ));
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
    fn find_covering_none_for_no_mounts() {
        assert!(find_covering(&[], "/anything").is_none());
    }
}
