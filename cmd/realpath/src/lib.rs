//! user realpath — print the resolved absolute path.
use std::path::{Path, PathBuf};

use usercore::pathx::{canonicalize_mode, Need};
use usercore::Ui;

/// Entry point for `realpath`: each FILE's absolute, resolved path (`-E` by default:
/// all but the last component must exist), relative to `--relative-to` / under
/// `--relative-base` when given.
pub fn run() -> i32 {
    let ui = Ui::new("realpath");
    let mut need = Need::AllButLast;
    let mut follow = true;
    let mut zero = false;
    let mut quiet = false;
    let mut relative_to: Option<PathBuf> = None;
    let mut relative_base: Option<PathBuf> = None;
    let mut paths: Vec<PathBuf> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut operands = false;
    while i < args.len() {
        let a = args[i].as_str();
        if operands || !a.starts_with('-') || a == "-" {
            paths.push(PathBuf::from(a));
            i += 1;
            continue;
        }
        match a {
            "--" => operands = true,
            "-h" | "--help" => {
                print!(
                    "Usage: realpath [OPTION]... FILE...\n\
 Print the resolved absolute file name.\n\n\
 -e, --canonicalize-existing  all components of the path must exist\n\
 -E, --canonicalize           all but the last component must exist (default)\n\
 -m, --canonicalize-missing   no path components need exist or be a directory\n\
 -L, --logical                resolve '..' components before symlinks\n\
 -P, --physical               resolve symlinks as encountered (default)\n\
 -q, --quiet                  suppress most error messages\n\
     --relative-to=DIR        print the resolved path relative to DIR\n\
     --relative-base=DIR      print absolute paths unless paths below DIR\n\
 -s, --strip, --no-symlinks   don't expand symlinks\n\
 -z, --zero                   end each output line with NUL, not newline\n"
                );
                return 0;
            }
            "--version" => {
                println!("realpath (user_utils) 0.2.0");
                return 0;
            }
            "--relative-to" | "--relative-base" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{a}' requires an argument"));
                    return 1;
                };
                if a == "--relative-to" {
                    relative_to = Some(PathBuf::from(v));
                } else {
                    relative_base = Some(PathBuf::from(v));
                }
            }
            s if s.starts_with("--relative-to=") => relative_to = Some(PathBuf::from(&s[14..])),
            s if s.starts_with("--relative-base=") => relative_base = Some(PathBuf::from(&s[16..])),
            "--canonicalize-existing" => need = Need::All,
            "--canonicalize" => need = Need::AllButLast,
            "--canonicalize-missing" => need = Need::Nothing,
            "--logical" | "--strip" | "--no-symlinks" => follow = false,
            "--physical" => follow = true,
            "--quiet" => quiet = true,
            "--zero" => zero = true,
            s if !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'e' => need = Need::All,
                        'E' => need = Need::AllButLast,
                        'm' => need = Need::Nothing,
                        'L' | 's' => follow = false,
                        'P' => follow = true,
                        'q' => quiet = true,
                        'z' => zero = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            s => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
        }
        i += 1;
    }
    if paths.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let resolve = |p: &Path| canonicalize_mode(p, need, follow);
    let to = match relative_to.as_deref().map(resolve).transpose() {
        Ok(t) => t,
        Err(e) => {
            ui.err(&format!("{}: {e}", relative_to.unwrap_or_default().display()));
            return 1;
        }
    };
    let base = match relative_base.as_deref().map(resolve).transpose() {
        Ok(b) => b,
        Err(e) => {
            ui.err(&format!("{}: {e}", relative_base.unwrap_or_default().display()));
            return 1;
        }
    };
    // with both, --relative-to only applies when it is itself under --relative-base
    let to = match (&to, &base) {
        (Some(t), Some(b)) if !t.starts_with(b) => None,
        _ => to,
    };
    let end = if zero { "\0" } else { "\n" };
    let mut status = 0;
    for p in &paths {
        match resolve(p) {
            Ok(abs) => {
                let shown = match (&to, &base) {
                    (Some(t), Some(b)) if abs.starts_with(b) => pathdiff(t, &abs),
                    (Some(_), Some(_)) => abs.display().to_string(),
                    (Some(t), None) => pathdiff(t, &abs),
                    (None, Some(b)) if abs.starts_with(b) => pathdiff(b, &abs),
                    _ => abs.display().to_string(),
                };
                print!("{shown}{end}");
            }
            Err(e) => {
                if !quiet {
                    let msg = e.to_string();
                    let msg = msg.find(" (os error").map_or(msg.as_str(), |i| &msg[..i]).to_string();
                    ui.err(&format!("{}: {msg}", p.display()));
                }
                status = 1;
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
