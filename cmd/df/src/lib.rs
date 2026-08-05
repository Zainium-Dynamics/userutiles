//! user df — report file system disk space usage.
use colored::Colorize;
use std::ffi::CString;
use std::io::{self, IsTerminal};
use std::mem::MaybeUninit;
use std::path::Path;

use usercore::Ui;

/// Entry point for the `df` utility. Parses `std::env::args()` and prints
/// a table of mounted (or explicitly named) filesystems with their block
/// or inode usage, reading mount info from `/proc/self/mounts` and space
/// info via `statvfs(3)`.
///
/// Returns 0 on success, 1 if any given path could not be resolved to a
/// filesystem.
pub fn run() -> i32 {
    let ui = Ui::new("df");
    let mut human = false;
    let mut inodes = false;
    let mut local = false;
    let mut paths: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("df (user_utils) 0.1.0");
                return 0;
            }
            "-h" | "--human-readable" => human = true,
            "-i" | "--inodes" => inodes = true,
            "-l" | "--local" => local = true,
            "-T" | "--print-type" => {} // always show type-ish via fstype from mount
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'h' => human = true,
                        'i' => inodes = true,
                        'l' => local = true,
                        'T' => {}
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => paths.push(other.to_string()),
        }
    }

    let mounts = read_mounts();
    let color = io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();

    if paths.is_empty() {
        print_header(human, inodes, color);
        for m in &mounts {
            if local && is_remote(&m.fstype) {
                continue;
            }
            if let Ok(st) = statvfs(&m.target) {
                print_row(&m.source, &m.fstype, &m.target, &st, human, inodes, color);
            }
        }
    } else {
        print_header(human, inodes, color);
        let mut status = 0;
        for p in &paths {
            match resolve_row(p, &mounts) {
                Ok((source, fstype, target, st)) => {
                    print_row(&source, &fstype, &target, &st, human, inodes, color)
                }
                Err(()) => {
                    ui.err(&format!("{p}: No such file or directory"));
                    status = 1;
                }
            }
        }
        return status;
    }
    0
}

fn print_help() {
    print!(
        "Usage: df [OPTION]... [FILE]...\n\
 Show information about the file system on which each FILE resides,\n\
 or all file systems by default.\n\n\
 -h, --human-readable print sizes in powers of 1024 (e.g., 1023M)\n\
 -i, --inodes list inode information instead of block usage\n\
 -l, --local limit listing to local file systems\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

struct Mount {
    source: String,
    target: String,
    fstype: String,
}

/// Parse `/proc/self/mounts` into a list of `Mount`s. Returns an empty
/// list (rather than an error) if the file cannot be read, since `df`
/// with explicit path arguments can still work via `statvfs` alone.
fn read_mounts() -> Vec<Mount> {
    let Ok(data) = std::fs::read_to_string("/proc/self/mounts") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in data.lines() {
        let mut parts = line.split_whitespace();
        let source = parts.next().unwrap_or("-").to_string();
        let target = unescape_mount(parts.next().unwrap_or("/"));
        let fstype = parts.next().unwrap_or("-").to_string();
        out.push(Mount {
            source,
            target,
            fstype,
        });
    }
    out
}

/// Undo the octal-escaping (`\040` for a space, etc.) that the kernel
/// applies to whitespace and backslashes in `/proc/self/mounts` fields.
fn unescape_mount(s: &str) -> String {
    // \040 etc.
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() {
            let oct = &s[i + 1..i + 4];
            if let Ok(v) = u8::from_str_radix(oct, 8) {
                out.push(v as char);
                i += 4;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

struct StatVfs {
    blocks: u64,
    bfree: u64,
    bavail: u64,
    bsize: u64,
    files: u64,
    ffree: u64,
}

/// Call `statvfs(3)` on `path` and return the fields `df` needs (block
/// and inode counts/free/available, block size).
fn statvfs(path: &str) -> io::Result<StatVfs> {
    let c = CString::new(path.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let mut st = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for the
    // duration of this call, so `c.as_ptr()` is a sound path argument.
    // `st.as_mut_ptr()` points to a stack-allocated, properly aligned
    // `libc::statvfs` with room for the full struct (via `MaybeUninit`);
    // `statvfs(3)` only writes into it and never reads from it, so
    // passing uninitialized memory of the right size/alignment is sound.
    let rc = unsafe { libc::statvfs(c.as_ptr(), st.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `statvfs` returned 0 above, meaning it succeeded and fully
    // populated every field of `st`, so `assume_init` is sound.
    let st = unsafe { st.assume_init() };
    Ok(StatVfs {
        blocks: st.f_blocks as u64,
        bfree: st.f_bfree as u64,
        bavail: st.f_bavail as u64,
        bsize: st.f_frsize as u64,
        files: st.f_files as u64,
        ffree: st.f_ffree as u64,
    })
}

/// Find the mount entry whose target is the longest prefix match for
/// `path` (after canonicalizing `path`), i.e. the filesystem `path`
/// actually lives on. Falls back to `path` itself if canonicalization
/// fails (e.g. the path doesn't exist).
fn find_mount_for<'a>(path: &str, mounts: &'a [Mount]) -> Option<&'a Mount> {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| Path::new(path).to_path_buf());
    let s = canon.to_string_lossy();
    mounts
        .iter()
        .filter(|m| {
            s == m.target
                || s.starts_with(&(m.target.trim_end_matches('/').to_string() + "/"))
                || m.target == "/"
        })
        .max_by_key(|m| m.target.len())
}

/// Resolve `path` to the `(source, fstype, mount-point, StatVfs)` row `df`
/// should print for it.
///
/// `find_mount_for` falls back to matching `/` for *any* path (its filter
/// has an unconditional `m.target == "/"` clause), so it returns `Some`
/// for effectively every input — including a path that doesn't exist.
/// Because of that, resolving a mount and calling `statvfs` are two
/// independent steps that can each fail, and both must be checked:
/// mount-resolution failure falls back to reporting the path verbatim
/// (fstype `"-"`), while a `statvfs` failure (e.g. the path doesn't
/// exist) is always a hard error regardless of whether a mount matched.
fn resolve_row(path: &str, mounts: &[Mount]) -> Result<(String, String, String, StatVfs), ()> {
    match find_mount_for(path, mounts) {
        Some(m) => {
            let st = statvfs(path).map_err(|_| ())?;
            Ok((m.source.clone(), m.fstype.clone(), m.target.clone(), st))
        }
        None => {
            let st = statvfs(path).map_err(|_| ())?;
            Ok((path.to_string(), "-".to_string(), path.to_string(), st))
        }
    }
}

/// Return true if `fstype` is a network/remote filesystem, used to
/// implement `-l`/`--local`.
fn is_remote(fstype: &str) -> bool {
    matches!(
        fstype,
        "nfs" | "nfs4" | "cifs" | "smb" | "smb3" | "sshfs" | "fuse.sshfs"
    )
}

fn print_header(human: bool, inodes: bool, color: bool) {
    let h = if inodes {
        format!(
            "{:<20} {:>10} {:>10} {:>10} {:>5} {}",
            "Filesystem", "Inodes", "IUsed", "IFree", "IUse%", "Mounted on"
        )
    } else if human {
        format!(
            "{:<20} {:>8} {:>8} {:>8} {:>5} {}",
            "Filesystem", "Size", "Used", "Avail", "Use%", "Mounted on"
        )
    } else {
        format!(
            "{:<20} {:>10} {:>10} {:>10} {:>5} {}",
            "Filesystem", "1K-blocks", "Used", "Available", "Use%", "Mounted on"
        )
    };
    if color {
        println!("{}", h.bright_cyan().bold());
    } else {
        println!("{h}");
    }
}

fn print_row(
    source: &str,
    _fstype: &str,
    target: &str,
    st: &StatVfs,
    human: bool,
    inodes: bool,
    color: bool,
) {
    if inodes {
        let total = st.files;
        let free = st.ffree;
        let used = total.saturating_sub(free);
        let pct = if total == 0 { 0 } else { (used * 100) / total };
        let line = format!(
            "{:<20} {:>10} {:>10} {:>10} {:>4}% {}",
            trunc(source, 20),
            total,
            used,
            free,
            pct,
            target
        );
        println!("{line}");
        return;
    }
    let total_b = st.blocks.saturating_mul(st.bsize);
    let avail_b = st.bavail.saturating_mul(st.bsize);
    let free_b = st.bfree.saturating_mul(st.bsize);
    let used_b = total_b.saturating_sub(free_b);
    let pct = if total_b == 0 {
        0
    } else {
        (used_b * 100) / total_b
    };
    let (ts, us, as_) = if human {
        (human_size(total_b), human_size(used_b), human_size(avail_b))
    } else {
        (
            format!("{}", total_b / 1024),
            format!("{}", used_b / 1024),
            format!("{}", avail_b / 1024),
        )
    };
    let pct_s = format!("{pct}%");
    let pct_c = if color {
        if pct >= 95 {
            pct_s.bright_red().bold().to_string()
        } else if pct >= 80 {
            pct_s.yellow().to_string()
        } else {
            pct_s.bright_green().to_string()
        }
    } else {
        pct_s
    };
    println!(
        "{:<20} {:>8} {:>8} {:>8} {:>5} {}",
        trunc(source, 20),
        ts,
        us,
        as_,
        pct_c,
        target
    );
}

/// Truncate `s` to at most `n` bytes, appending `…` if it was cut short.
/// Operates on bytes, not chars, so is only safe for ASCII input (mount
/// source paths in practice).
fn trunc(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}

/// Format a byte count as a human-readable size using binary (1024-based)
/// single-letter units, e.g. `1536` -> `"1.5K"`.
fn human_size(n: u64) -> String {
    const U: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
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
    fn unescape_mount_decodes_octal_space() {
        assert_eq!(unescape_mount(r"\040"), " ");
        assert_eq!(unescape_mount(r"/mnt/my\040drive"), "/mnt/my drive");
        assert_eq!(unescape_mount("/plain/path"), "/plain/path");
    }

    #[test]
    fn is_remote_recognizes_network_fstypes() {
        assert!(is_remote("nfs"));
        assert!(is_remote("cifs"));
        assert!(!is_remote("ext4"));
        assert!(!is_remote("xfs"));
    }

    #[test]
    fn trunc_short_and_long_strings() {
        assert_eq!(trunc("short", 20), "short");
        assert_eq!(trunc("this-is-a-very-long-source-name", 10), "this-is-a…");
    }

    #[test]
    fn human_size_formats_units() {
        assert_eq!(human_size(0), "0");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1024 * 1024), "1.0M");
    }

    #[test]
    fn find_mount_for_picks_longest_prefix() {
        let mounts = vec![
            Mount {
                source: "root".into(),
                target: "/".into(),
                fstype: "ext4".into(),
            },
            Mount {
                source: "home".into(),
                target: "/home".into(),
                fstype: "ext4".into(),
            },
        ];
        let m = find_mount_for("/home/alice/file.txt", &mounts).unwrap();
        assert_eq!(m.target, "/home");
    }

    #[test]
    fn statvfs_root_succeeds() {
        // "/" always exists on a Linux test runner.
        let st = statvfs("/").expect("statvfs(\"/\") should succeed");
        assert!(st.blocks > 0);
        assert!(st.bsize > 0);
    }

    #[test]
    fn statvfs_missing_path_errors() {
        let missing = format!("/nonexistent_user_df_test_path_{}", std::process::id());
        assert!(statvfs(&missing).is_err());
    }

    #[test]
    fn resolve_row_succeeds_for_root() {
        let mounts = vec![Mount {
            source: "root".into(),
            target: "/".into(),
            fstype: "ext4".into(),
        }];
        let (source, fstype, target, _st) = resolve_row("/", &mounts).unwrap();
        assert_eq!(source, "root");
        assert_eq!(fstype, "ext4");
        assert_eq!(target, "/");
    }

    #[test]
    fn resolve_row_errors_for_nonexistent_path_even_with_a_matching_mount() {
        // Regression: find_mount_for's unconditional "/" fallback means it
        // returns Some(mount) for a nonexistent path too, so the mount
        // match alone must not be treated as success — resolve_row must
        // still surface the statvfs failure.
        let mounts = vec![Mount {
            source: "root".into(),
            target: "/".into(),
            fstype: "ext4".into(),
        }];
        let missing = format!("/nonexistent_user_df_test_path_{}", std::process::id());
        assert!(resolve_row(&missing, &mounts).is_err());
    }

    #[test]
    fn resolve_row_errors_for_nonexistent_path_with_no_mounts() {
        let mounts: Vec<Mount> = Vec::new();
        let missing = format!("/nonexistent_user_df_test_path_{}", std::process::id());
        assert!(resolve_row(&missing, &mounts).is_err());
    }
}
