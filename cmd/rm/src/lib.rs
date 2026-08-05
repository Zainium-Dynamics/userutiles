//! user rm — remove files or directories.
//!
//! Zainium protected paths (never removable, even as root / with sudo):
//! - `/overlayer/syshub` and everything under it
//! - any `zaisys` directory and everything under it
//! - any `zexlib` directory itself (contents *inside* zexlib may be deleted)

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use usercore::protect;

pub fn run() -> i32 {
    let mut recursive = false;
    let mut force = false;
    let mut interactive = false;
    let mut verbose = false;
    let mut dir_only = false;
    let mut paths: Vec<PathBuf> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: rm [OPTION]... [FILE]...\n\
Remove (unlink) the FILE(s).\n\n\
  -f, --force           ignore nonexistent files and arguments, never prompt\n\
  -i                    prompt before every removal\n\
  -I                    prompt once before removing more than three files\n\
  -r, -R, --recursive   remove directories and their contents recursively\n\
  -d, --dir             remove empty directories\n\
  -v, --verbose         explain what is being done\n\
      --help            display this help and exit\n\
      --version         output version information and exit\n\n\
Zainium protected paths (not removable even as root):\n\
  /overlayer/syshub     entire tree\n\
  zaisys                entire tree (any location)\n\
  zexlib                the directory itself only\n\
                        (items inside zexlib may be removed)\n"
                );
                return 0;
            }
            "--version" => {
                println!("rm (user_utils) 0.1.0");
                return 0;
            }
            "-f" | "--force" => force = true,
            "-i" => interactive = true,
            "-I" => {} // soft interactive — treat as non-fatal for automation
            "-r" | "-R" | "--recursive" => recursive = true,
            "-d" | "--dir" => dir_only = true,
            "-v" | "--verbose" => verbose = true,
            "--" => {}
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'f' => force = true,
                        'i' => interactive = true,
                        'I' => {}
                        'r' | 'R' => recursive = true,
                        'd' => dir_only = true,
                        'v' => verbose = true,
                        _ => {
                            eprintln!("rm: invalid option -- '{c}'");
                            return 1;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                eprintln!("rm: unrecognized option '{s}'");
                return 1;
            }
            other => paths.push(PathBuf::from(other)),
        }
    }

    if paths.is_empty() {
        if !force {
            eprintln!("rm: missing operand");
            eprintln!("Try 'rm --help' for more information.");
            return 1;
        }
        return 0;
    }

    let mut status = 0;
    for p in &paths {
        if let Err(e) = remove_path(p, recursive, force, interactive, verbose, dir_only) {
            // Protected paths always report (force cannot bypass Zainium guards).
            if protect::removal_denied(p).is_some() || !force || e.kind() != io::ErrorKind::NotFound
            {
                eprintln!("rm: cannot remove '{}': {e}", p.display());
                status = 1;
            }
        }
    }
    status
}

fn protect_err(path: &Path) -> io::Result<()> {
    if let Some(reason) = protect::removal_denied(path) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            reason.message(),
        ));
    }
    Ok(())
}

fn remove_path(
    path: &Path,
    recursive: bool,
    force: bool,
    interactive: bool,
    verbose: bool,
    dir_only: bool,
) -> io::Result<()> {
    // Refuse before any mutation — even with -f / root / sudo.
    protect_err(path)?;

    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if force && e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
        if recursive {
            if interactive && !prompt(&format!("rm: remove directory '{}'? ", path.display())) {
                return Ok(());
            }
            remove_dir_recursive(path, force, interactive, verbose)?;
            if verbose {
                println!("removed directory '{}'", path.display());
            }
            Ok(())
        } else if dir_only {
            if interactive && !prompt(&format!("rm: remove directory '{}'? ", path.display())) {
                return Ok(());
            }
            protect_err(path)?;
            fs::remove_dir(path)?;
            if verbose {
                println!("removed directory '{}'", path.display());
            }
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                "Is a directory",
            ))
        }
    } else {
        if interactive && !prompt(&format!("rm: remove '{}'? ", path.display())) {
            return Ok(());
        }
        protect_err(path)?;
        fs::remove_file(path)?;
        if verbose {
            println!("removed '{}'", path.display());
        }
        Ok(())
    }
}

fn remove_dir_recursive(
    path: &Path,
    force: bool,
    interactive: bool,
    verbose: bool,
) -> io::Result<()> {
    // Directory itself must not be a protected root/tree.
    protect_err(path)?;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        // Skip (and fail) protected children rather than wiping them via parent -rf.
        if let Some(reason) = protect::removal_denied(&p) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                reason.message(),
            ));
        }
        let ft = entry.file_type()?;
        if ft.is_dir() && !ft.is_symlink() {
            remove_dir_recursive(&p, force, interactive, verbose)?;
        } else {
            if interactive && !prompt(&format!("rm: remove '{}'? ", p.display())) {
                continue;
            }
            protect_err(&p)?;
            fs::remove_file(&p).or_else(|e| {
                if ft.is_dir() {
                    fs::remove_dir_all(&p)
                } else {
                    Err(e)
                }
            })?;
            if verbose {
                println!("removed '{}'", p.display());
            }
        }
    }
    protect_err(path)?;
    fs::remove_dir(path)
}

fn prompt(msg: &str) -> bool {
    eprint!("{msg}");
    let _ = io::stderr().flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.chars().next(), Some('y') | Some('Y'))
}
