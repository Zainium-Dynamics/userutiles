//! user updatedb — update a file name database for locate.
//!
//! Database path is **not** hardcoded to `/usr/*`. Resolution order:
//! CLI `--output` → `LOCATE_PATH` / `ZEX_LOCATEDB` → `$ZEX_PREFIX/var/lib/misc/locatedb`.
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `updatedb` utility. Parses `std::env::args()`, walks
/// the requested root directories (default `/`), and writes one absolute
/// path per line to a temp file that is atomically renamed onto the target
/// database on success.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("updatedb");
    let mut output: Option<PathBuf> = None;
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut prune_paths: Vec<PathBuf> = Vec::new();
    let mut prune_names: Vec<String> = vec![
        ".git".into(),
        ".hg".into(),
        ".svn".into(),
        "node_modules".into(),
        "target".into(),
    ];
    let mut verbose = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: updatedb [OPTION]...\n\
 Update a database for locate.\n\n\
 -o, --output=FILE database location\n\
 -U, --database-root=DIR start tree under DIR (repeatable)\n\
 -e, --prunepaths=PATHS colon/space separated prune paths\n\
 -n, --prunenames=NAMES colon/space separated directory names to skip\n\
 -v, --verbose list files as they are found\n\
 --help display this help\n\
 --version output version\n\n\
 Default database: LOCATE_PATH / ZEX_LOCATEDB / $ZEX_PREFIX/var/lib/misc/locatedb\n"
                );
                return 0;
            }
            "--version" => {
                println!("updatedb (user_utils) 0.1.0");
                return 0;
            }
            "-o" | "--output" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option requires an argument -- 'o'");
                    return 1;
                };
                output = Some(PathBuf::from(v));
            }
            s if s.starts_with("--output=") => {
                output = Some(PathBuf::from(&s["--output=".len()..]));
            }
            "-U" | "--database-root" => {
                i += 1;
                let Some(r) = args.get(i) else {
                    ui.err("option requires an argument -- 'U'");
                    return 1;
                };
                roots.push(PathBuf::from(r));
            }
            s if s.starts_with("--database-root=") => {
                roots.push(PathBuf::from(&s["--database-root=".len()..]));
            }
            "-e" | "--prunepaths" => {
                i += 1;
                let Some(s) = args.get(i) else {
                    ui.err("option requires an argument -- 'e'");
                    return 1;
                };
                prune_paths.extend(split_list(s).into_iter().map(PathBuf::from));
            }
            s if s.starts_with("--prunepaths=") => {
                prune_paths.extend(
                    split_list(&s["--prunepaths=".len()..])
                        .into_iter()
                        .map(PathBuf::from),
                );
            }
            "-n" | "--prunenames" => {
                i += 1;
                let Some(s) = args.get(i) else {
                    ui.err("option requires an argument -- 'n'");
                    return 1;
                };
                prune_names = split_list(s);
            }
            "-v" | "--verbose" => verbose = true,
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => roots.push(PathBuf::from(other)),
        }
        i += 1;
    }

    if roots.is_empty() {
        roots.push(PathBuf::from("/"));
    }

    let db = output.unwrap_or_else(usercore::zainium::default_locate_db);
    if let Some(parent) = db.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            ui.err(&format!("cannot create '{}': {e}", parent.display()));
            return 1;
        }
    }

    // Write to a per-process temp file then rename atomically onto the
    // real database, so a reader never observes a partially written db and
    // two concurrent `updatedb` runs don't clobber each other's temp file.
    let tmp = db.with_extension(format!("db.tmp.{}", std::process::id()));
    let file = match File::create(&tmp) {
        Ok(f) => f,
        Err(e) => {
            ui.err(&format!("{}: {e}", tmp.display()));
            return 1;
        }
    };
    let mut out = io::BufWriter::new(file);
    if let Err(e) = writeln!(out, "# user-locatedb 1") {
        ui.err(&format!("write error: {e}"));
        let _ = fs::remove_file(&tmp);
        return 1;
    }

    let mut count = 0u64;
    for root in &roots {
        if let Err(e) = walk(
            root,
            root,
            &prune_paths,
            &prune_names,
            &mut out,
            verbose,
            &mut count,
        ) {
            ui.err(&format!("{}: {e}", root.display()));
        }
    }
    if let Err(e) = out.flush() {
        ui.err(&format!("flush error: {e}"));
        let _ = fs::remove_file(&tmp);
        return 1;
    }
    drop(out);
    if let Err(e) = fs::rename(&tmp, &db) {
        ui.err(&format!("rename to {}: {e}", db.display()));
        let _ = fs::remove_file(&tmp);
        return 1;
    }
    if verbose {
        ui.info(&format!("wrote {count} paths to {}", db.display()));
    }
    0
}

/// Split a colon/whitespace-separated option value (`--prunepaths`,
/// `--prunenames`) into its individual non-empty entries.
fn split_list(s: &str) -> Vec<String> {
    s.split(|c: char| c == ':' || c.is_whitespace())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

/// Recursively walk `path` (part of the tree rooted at `root`), writing
/// one line per non-pruned entry to `out` and bumping `count`. Pruned
/// directories (by exact/prefix path match or by bare directory name) are
/// skipped entirely — their contents are never visited or logged.
///
/// I/O errors reading individual directories are swallowed (matches GNU
/// `updatedb`'s best-effort tree walk: an unreadable subtree just yields
/// fewer entries, it doesn't abort the whole run) but the write side
/// (`out`) still propagates errors, since a failed database write is fatal.
fn walk(
    root: &Path,
    path: &Path,
    prune_paths: &[PathBuf],
    prune_names: &[String],
    out: &mut impl Write,
    verbose: bool,
    count: &mut u64,
) -> io::Result<()> {
    for p in prune_paths {
        if path == p || path.starts_with(p) {
            return Ok(());
        }
    }
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        if prune_names.iter().any(|n| n == name) {
            return Ok(());
        }
    }

    let display = if path.as_os_str().is_empty() {
        root.display().to_string()
    } else {
        path.display().to_string()
    };
    // skip paths with newlines (format constraint)
    if !display.contains('\n') {
        writeln!(out, "{display}")?;
        *count += 1;
        if verbose {
            eprintln!("{display}");
        }
    }

    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Ok(());
    }
    let rd = match fs::read_dir(path) {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };
    for ent in rd {
        let ent = match ent {
            Ok(e) => e,
            Err(_) => continue,
        };
        walk(
            root,
            &ent.path(),
            prune_paths,
            prune_names,
            out,
            verbose,
            count,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_list_handles_colon_and_whitespace() {
        assert_eq!(
            split_list("a:b c  d"),
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]
        );
    }

    #[test]
    fn split_list_empty_input() {
        assert_eq!(split_list(""), Vec::<String>::new());
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("user_updatedb_test_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn walk_lists_files_and_dirs() {
        let root = tmp_dir("basic");
        File::create(root.join("a.txt")).unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        File::create(root.join("sub/b.txt")).unwrap();

        let mut out = Vec::new();
        let mut count = 0;
        walk(&root, &root, &[], &[], &mut out, false, &mut count).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert!(text.contains(&root.display().to_string()));
        assert!(text.contains("a.txt"));
        assert!(text.contains("sub"));
        assert!(text.contains("b.txt"));
        assert_eq!(count, 4); // root, a.txt, sub, sub/b.txt

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn walk_prunes_by_name() {
        let root = tmp_dir("prune_name");
        fs::create_dir(root.join("node_modules")).unwrap();
        File::create(root.join("node_modules/pkg.js")).unwrap();
        File::create(root.join("keep.txt")).unwrap();

        let mut out = Vec::new();
        let mut count = 0;
        let prune_names = vec!["node_modules".to_string()];
        walk(&root, &root, &[], &prune_names, &mut out, false, &mut count).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert!(!text.contains("node_modules"));
        assert!(!text.contains("pkg.js"));
        assert!(text.contains("keep.txt"));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn walk_prunes_by_path() {
        let root = tmp_dir("prune_path");
        let sub = root.join("secret");
        fs::create_dir(&sub).unwrap();
        File::create(sub.join("s.txt")).unwrap();
        File::create(root.join("keep.txt")).unwrap();

        let mut out = Vec::new();
        let mut count = 0;
        let prune_paths = vec![sub.clone()];
        walk(&root, &root, &prune_paths, &[], &mut out, false, &mut count).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert!(!text.contains("secret"));
        assert!(text.contains("keep.txt"));

        fs::remove_dir_all(&root).unwrap();
    }
}
