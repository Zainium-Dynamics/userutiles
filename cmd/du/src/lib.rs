//! user du — estimate file space usage.
use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `du` utility. Parses `std::env::args()` and prints
/// disk usage (in 1K blocks by default) for each FILE operand,
/// recursively for directories, defaulting to the current directory if
/// none are given.
///
/// Returns 0 on success, 1 if any path could not be read.
pub fn run() -> i32 {
    let ui = Ui::new("du");
    let mut human = false;
    let mut summarize = false;
    let mut all = false;
    let mut bytes = false;
    let mut max_depth: Option<usize> = None;
    let mut apparent = false;
    let mut paths: Vec<PathBuf> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("du (user_utils) 0.1.0");
                return 0;
            }
            "-h" | "--human-readable" => human = true,
            "-s" | "--summarize" => summarize = true,
            "-a" | "--all" => all = true,
            "-b" | "--bytes" => bytes = true,
            "--apparent-size" => apparent = true,
            "-d" | "--max-depth" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'd'");
                    return 1;
                }
                max_depth = args[i].parse().ok();
            }
            s if s.starts_with("--max-depth=") => {
                max_depth = s["--max-depth=".len()..].parse().ok();
            }
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'h' => human = true,
                        's' => summarize = true,
                        'a' => all = true,
                        'b' => bytes = true,
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
            other => paths.push(PathBuf::from(other)),
        }
        i += 1;
    }
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    if summarize {
        max_depth = Some(0);
    }

    let mut status = 0;
    let mut seen = HashSet::new();
    for p in &paths {
        match du_path(p, human, bytes, apparent, all, max_depth, 0, &mut seen) {
            Ok(_total) => {}
            Err(e) => {
                ui.err(&format!("{}: {e}", p.display()));
                status = 1;
            }
        }
    }
    status
}

fn print_help() {
    print!(
        "Usage: du [OPTION]... [FILE]...\n\
 Summarize disk usage of the set of FILEs, recursively for directories.\n\n\
 -a, --all write counts for all files, not just directories\n\
 -b, --bytes equivalent to '--apparent-size --block-size=1'\n\
 -d, --max-depth=N print the total for a directory (or file, with --all)\n\
 only if it is N or fewer levels below the command line\n\
 -h, --human-readable print sizes in human readable format (e.g., 1K 234M 2G)\n\
 -s, --summarize display only a total for each argument\n\
 --apparent-size print apparent sizes, rather than disk usage\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Recursively compute (and print, subject to `all`/`max_depth`) the disk
/// usage of `path`.
///
/// Uses `symlink_metadata` (lstat) rather than following symlinks, so a
/// symlink — including a self-referential one — is counted as its own
/// entry and never descended into; this is what keeps recursion bounded
/// on a directory tree containing a symlink loop.
///
/// `seen` de-duplicates by `(dev, ino)` so a file with multiple hard
/// links inside the scanned tree is only counted once, matching GNU `du`.
/// Directories are excluded from that de-dup set since counting a
/// directory's own block only once per traversal path is correct (dirs
/// aren't typically hard-linked, and excluding them keeps the total's
/// self-size contribution simple).
fn du_path(
    path: &Path,
    human: bool,
    bytes: bool,
    apparent: bool,
    all: bool,
    max_depth: Option<usize>,
    depth: usize,
    seen: &mut HashSet<(u64, u64)>,
) -> io::Result<u64> {
    let meta = fs::symlink_metadata(path)?;
    // hardlink de-dup
    let key = (meta.dev(), meta.ino());
    if !meta.is_dir() && !seen.insert(key) {
        return Ok(0);
    }

    let self_size = if bytes || apparent {
        meta.len()
    } else {
        // st_blocks is 512-byte units
        (meta.blocks() as u64) * 512
    };

    if meta.is_symlink() {
        // don't follow
        if all || depth == 0 {
            print_size(self_size, path, human, bytes);
        }
        return Ok(self_size);
    }

    if meta.is_file() || !meta.is_dir() {
        if all || depth == 0 {
            print_size(self_size, path, human, bytes);
        }
        return Ok(self_size);
    }

    // directory
    let mut total = self_size;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        total += du_path(
            &entry.path(),
            human,
            bytes,
            apparent,
            all,
            max_depth,
            depth + 1,
            seen,
        )?;
    }

    let show = match max_depth {
        Some(d) => depth <= d,
        None => true,
    };
    if show {
        print_size(total, path, human, bytes);
    }
    Ok(total)
}

/// Print one `SIZE\tPATH` row to stdout, formatting `size` in human units,
/// raw bytes, or 1K blocks per the `human`/`bytes` flags.
fn print_size(size: u64, path: &Path, human: bool, bytes: bool) {
    let mut out = io::stdout().lock();
    if human {
        let _ = writeln!(out, "{}\t{}", human_size(size), path.display());
    } else if bytes {
        let _ = writeln!(out, "{size}\t{}", path.display());
    } else {
        // 1K blocks
        let blocks = (size + 1023) / 1024;
        let _ = writeln!(out, "{blocks}\t{}", path.display());
    }
}

/// Format a byte count as a human-readable size using binary (1024-based)
/// single-letter units, e.g. `1536` -> `"1.5K"`.
fn human_size(n: u64) -> String {
    const U: [&str; 6] = ["", "K", "M", "G", "T", "P"];
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

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("user_du_test_{tag}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn du_path_does_not_hang_on_self_referential_symlink() {
        // Regression: a symlink loop must not cause unbounded recursion.
        // du_path already uses symlink_metadata (never follows symlinks
        // during traversal), so this should terminate immediately.
        let dir = tmp_dir("symlink_loop");
        let loop_link = dir.join("loop");
        std::os::unix::fs::symlink(&dir, &loop_link).unwrap();

        let mut seen = HashSet::new();
        let result = du_path(&dir, false, false, false, false, None, 0, &mut seen);
        assert!(result.is_ok(), "du_path must terminate: {result:?}");

        fs::remove_file(&loop_link).ok();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn du_path_sums_file_sizes_in_directory() {
        let dir = tmp_dir("sum");
        fs::write(dir.join("a.txt"), b"hello").unwrap();
        fs::write(dir.join("b.txt"), b"world!!").unwrap();

        let mut seen = HashSet::new();
        let total = du_path(&dir, false, true, true, false, None, 0, &mut seen).unwrap();
        // apparent size, bytes mode: exact byte sum plus the dir entry's
        // own apparent size (typically small but nonzero on most fs).
        assert!(total >= 12, "expected at least 12 bytes of file content, got {total}");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn du_path_missing_file_errors() {
        let missing = std::env::temp_dir().join(format!(
            "user_du_test_missing_{}_does_not_exist",
            std::process::id()
        ));
        let mut seen = HashSet::new();
        let result = du_path(&missing, false, false, false, false, None, 0, &mut seen);
        assert!(result.is_err());
    }

    #[test]
    fn du_path_hardlinks_counted_once() {
        // Two directories with the same entry count and same-length entry
        // names (so their own directory-inode apparent size matches),
        // differing only in whether the second file is a hardlink to the
        // first (same inode) or an independent file with equal-length
        // content. The hardlinked total should be exactly one file's worth
        // of bytes smaller.
        let dir = tmp_dir("hardlink");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        fs::write(&a, b"hello").unwrap();
        fs::hard_link(&a, &b).unwrap();

        let mut seen = HashSet::new();
        let total_with_link =
            du_path(&dir, false, true, true, false, None, 0, &mut seen).unwrap();

        let dir2 = tmp_dir("no_hardlink");
        fs::write(dir2.join("a.txt"), b"hello").unwrap();
        fs::write(dir2.join("c.txt"), b"world").unwrap();

        let mut seen2 = HashSet::new();
        let total_without_link =
            du_path(&dir2, false, true, true, false, None, 0, &mut seen2).unwrap();

        assert_eq!(total_without_link, total_with_link + 5);

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&dir2).ok();
    }

    #[test]
    fn human_size_formats_units() {
        assert_eq!(human_size(0), "0");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1024 * 1024), "1.0M");
    }
}
