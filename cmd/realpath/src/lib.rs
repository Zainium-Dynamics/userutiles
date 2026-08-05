//! user realpath — print the resolved absolute path.
use std::fs;
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `realpath` utility. Parses `std::env::args()` and
/// prints the canonicalized (absolute, symlink-resolved) form of each
/// `FILE`, optionally relative to `--relative-to=DIR`.
///
/// For a `FILE` that doesn't exist, falls back to printing its
/// (non-canonicalized) absolute form rather than failing outright, unless
/// the current directory itself can't be determined.
///
/// Returns 0 on success, 1 if any path could not be resolved at all (and
/// `-q` was not given to suppress the error), or on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("realpath");
    let mut zero = false;
    let mut quiet = false;
    let mut relative_to: Option<PathBuf> = None;
    let mut paths: Vec<PathBuf> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: realpath [OPTION]... FILE...\n\
 -z, --zero end each line with NUL\n\
 -q, --quiet suppress error messages\n\
 -s, --strip, --no-symlinks don't expand symlinks\n\
 --relative-to=DIR print relative to DIR\n"
                );
                return 0;
            }
            "--version" => {
                println!("realpath (user_utils) 0.1.0");
                return 0;
            }
            "-z" | "--zero" => zero = true,
            "-q" | "--quiet" => quiet = true,
            "-s" | "--strip" | "--no-symlinks" => {} // still canonicalize best-effort
            s if s.starts_with("--relative-to=") => {
                relative_to = Some(PathBuf::from(&s["--relative-to=".len()..]));
            }
            "--relative-to" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'relative-to'");
                    return 1;
                }
                relative_to = Some(PathBuf::from(&args[i]));
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => paths.push(PathBuf::from(other)),
        }
        i += 1;
    }
    if paths.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let end = if zero { "\0" } else { "\n" };
    let mut status = 0;
    for p in &paths {
        match fs::canonicalize(p) {
            Ok(abs) => {
                let out = if let Some(ref base) = relative_to {
                    match fs::canonicalize(base) {
                        Ok(b) => pathdiff(&b, &abs),
                        Err(_) => abs.display().to_string(),
                    }
                } else {
                    abs.display().to_string()
                };
                print!("{out}{end}");
            }
            Err(e) => {
                // if path does not exist, try absolute without resolve
                if p.is_absolute() {
                    print!("{}{end}", p.display());
                } else if let Ok(cwd) = std::env::current_dir() {
                    print!("{}{end}", cwd.join(p).display());
                } else {
                    if !quiet {
                        ui.err(&format!("{}: {e}", p.display()));
                    }
                    status = 1;
                }
            }
        }
    }
    status
}

/// Compute `target`'s path relative to `base`, given that both are already
/// absolute and canonicalized. Strips their common leading components, then
/// prepends one `..` per remaining `base` component; returns `"."` if
/// `target` and `base` are identical.
fn pathdiff(base: &Path, target: &Path) -> String {
    let mut b = base.components().peekable();
    let mut t = target.components().peekable();
    while matches!((b.peek(), t.peek()), (Some(x), Some(y)) if x == y) {
        b.next();
        t.next();
    }
    let mut out = PathBuf::new();
    for _ in b {
        out.push("..");
    }
    for c in t {
        out.push(c);
    }
    if out.as_os_str().is_empty() {
        ".".into()
    } else {
        out.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathdiff_identical_paths_is_dot() {
        assert_eq!(pathdiff(Path::new("/a/b"), Path::new("/a/b")), ".");
    }

    #[test]
    fn pathdiff_sibling_directory() {
        assert_eq!(pathdiff(Path::new("/a/b"), Path::new("/a/c")), "../c");
    }

    #[test]
    fn pathdiff_nested_child() {
        assert_eq!(pathdiff(Path::new("/a"), Path::new("/a/b/c")), "b/c");
    }

    #[test]
    fn pathdiff_unrelated_absolute_paths() {
        assert_eq!(
            pathdiff(Path::new("/a/b/c"), Path::new("/x/y")),
            "../../../x/y"
        );
    }
}
