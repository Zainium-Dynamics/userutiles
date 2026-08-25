//! user ln — make hard and symbolic links between files.
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use usercore::protect;

pub fn run() -> i32 {
    let mut symbolic = false;
    let mut force = false;
    let mut interactive = false;
    let mut verbose = false;
    let mut no_deref = false;
    let mut paths: Vec<PathBuf> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: ln [OPTION]... [-T] TARGET LINK_NAME\n\
 ln [OPTION]... TARGET\n\
 ln [OPTION]... TARGET... DIRECTORY\n\
 Create links between files.\n\n\
 -f, --force remove existing destination files\n\
 -i, --interactive prompt whether to remove destinations\n\
 -n, --no-dereference treat LINK_NAME as a normal file if it is a\n\
 symbolic link to a directory\n\
 -s, --symbolic make symbolic links instead of hard links\n\
 -v, --verbose print name of each linked file\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("ln (user_utils) 0.1.0");
                return 0;
            }
            "-s" | "--symbolic" => symbolic = true,
            "-f" | "--force" => force = true,
            "-i" | "--interactive" => interactive = true,
            "-v" | "--verbose" => verbose = true,
            "-n" | "--no-dereference" => no_deref = true,
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        's' => symbolic = true,
                        'f' => force = true,
                        'i' => interactive = true,
                        'v' => verbose = true,
                        'n' => no_deref = true,
                        _ => {
                            eprintln!("ln: invalid option -- '{c}'");
                            return 1;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                eprintln!("ln: unrecognized option '{s}'");
                return 1;
            }
            other => paths.push(PathBuf::from(other)),
        }
    }

    if paths.is_empty() {
        eprintln!("ln: missing file operand");
        return 1;
    }

    let (targets, link_dir_or_name) = if paths.len() == 1 {
        // ln TARGET -> create link in cwd named after target
        let t = paths[0].clone();
        let name = t
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| t.clone());
        (vec![t], name)
    } else {
        // paths.len() >= 2 here (the len()==1 arm above and the is_empty()
        // check further up cover the other cases), so this is provably
        // non-empty — but pop() stays an explicit error path rather than an
        // unwrap() so a future refactor of the checks above fails loudly
        // with a message instead of panicking.
        let Some(dest) = paths.pop() else {
            eprintln!("ln: missing file operand");
            return 1;
        };
        (paths, dest)
    };

    let dest_is_dir = link_dir_or_name.is_dir()
        && !(no_deref
            && link_dir_or_name
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false));

    if targets.len() > 1 && !dest_is_dir {
        eprintln!(
            "ln: target '{}' is not a directory",
            link_dir_or_name.display()
        );
        return 1;
    }

    let mut status = 0;
    for target in &targets {
        let link_path = if dest_is_dir {
            link_dir_or_name.join(target.file_name().unwrap_or_default())
        } else {
            link_dir_or_name.clone()
        };

        if let Some(reason) = protect::modification_denied(&link_path) {
            eprintln!(
                "ln: failed to create link '{}': {}",
                link_path.display(),
                reason.message()
            );
            status = 1;
            continue;
        }

        // Try the create directly first — symlink(2)/link(2) already fail
        // atomically with EEXIST if link_path exists, so the common
        // (non-clobbering) case has no check-then-act race at all.
        match create_link(target, &link_path, symbolic) {
            Ok(()) => {
                if verbose {
                    println!("'{}' -> '{}'", link_path.display(), target.display());
                }
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if interactive && !prompt(&link_path) {
                    continue;
                }
                if !force && !interactive {
                    eprintln!(
                        "ln: failed to create link '{}': File exists",
                        link_path.display()
                    );
                    status = 1;
                    continue;
                }
                match replace_link(target, &link_path, symbolic) {
                    Ok(()) => {
                        if verbose {
                            println!("'{}' -> '{}'", link_path.display(), target.display());
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "ln: failed to create {} link '{}' => '{}': {e}",
                            if symbolic { "symbolic" } else { "hard" },
                            link_path.display(),
                            target.display()
                        );
                        status = 1;
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "ln: failed to create {} link '{}' => '{}': {e}",
                    if symbolic { "symbolic" } else { "hard" },
                    link_path.display(),
                    target.display()
                );
                status = 1;
            }
        }
    }
    status
}

fn create_link(target: &Path, link_path: &Path, symbolic: bool) -> io::Result<()> {
    if symbolic {
        symlink(target, link_path)
    } else {
        fs::hard_link(target, link_path)
    }
}

/// Replace an existing `link_path` with a fresh link to `target`.
///
/// Creates the new link at a temporary sibling path first, then atomically
/// `rename()`s it into place — never a window where `link_path` is missing,
/// and never a way to lose the original entry if link creation fails partway
/// through (the previous implementation removed the existing entry *before*
/// attempting the create, so a failed create silently destroyed the original
/// with nothing left in its place).
fn replace_link(target: &Path, link_path: &Path, symbolic: bool) -> io::Result<()> {
    let parent = link_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file_name = link_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp_path = parent.join(format!(".{file_name}.user-ln-tmp-{}", std::process::id()));

    // Best-effort cleanup of a stale temp file from a previous crashed run.
    let _ = fs::remove_file(&tmp_path);

    create_link(target, &tmp_path, symbolic)?;

    match fs::rename(&tmp_path, link_path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}

fn prompt(path: &Path) -> bool {
    eprint!("ln: replace '{}'? ", path.display());
    let _ = io::stderr().flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.chars().next(), Some('y') | Some('Y'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("user_ln_test_{tag}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_link_makes_a_working_symlink() {
        let dir = tmp_dir("create_symlink");
        let target = dir.join("target.txt");
        let link = dir.join("link.txt");
        fs::write(&target, b"hello").unwrap();

        create_link(&target, &link, true).unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&link).unwrap(), b"hello");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn create_link_fails_with_already_exists_when_dest_present() {
        let dir = tmp_dir("create_conflict");
        let target = dir.join("target.txt");
        let link = dir.join("link.txt");
        fs::write(&target, b"hello").unwrap();
        fs::write(&link, b"pre-existing").unwrap();

        let err = create_link(&target, &link, true).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        // Original must be untouched — no check-then-act window opened.
        assert_eq!(fs::read(&link).unwrap(), b"pre-existing");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replace_link_atomically_swaps_in_the_new_link() {
        let dir = tmp_dir("replace_link");
        let target = dir.join("target.txt");
        let link = dir.join("link.txt");
        fs::write(&target, b"new content").unwrap();
        fs::write(&link, b"old content").unwrap();

        replace_link(&target, &link, true).unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&link).unwrap(), b"new content");

        // No stray temp file left behind in the directory.
        let leftover: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("user-ln-tmp"))
            .collect();
        assert!(leftover.is_empty(), "leftover temp files: {leftover:?}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replace_link_preserves_original_when_target_missing() {
        // If the new link can't be created (bad target scenario simulated by
        // pointing at a path whose parent doesn't exist), the original entry
        // at link_path must survive untouched — unlike the old
        // remove-then-create approach, which deleted the original first.
        let dir = tmp_dir("replace_preserves_original");
        let link = dir.join("link.txt");
        fs::write(&link, b"must survive").unwrap();
        let bogus_target = dir.join("no/such/parent/target.txt");

        // Hard-linking to a target through a missing parent directory fails.
        let result = replace_link(&bogus_target, &link, false);
        assert!(result.is_err());
        assert_eq!(fs::read(&link).unwrap(), b"must survive");
        fs::remove_dir_all(&dir).ok();
    }
}
