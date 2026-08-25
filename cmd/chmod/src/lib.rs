//! user chmod — change file mode bits.
use std::fs;

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use usercore::protect;

pub fn run() -> i32 {
    let mut recursive = false;
    let mut verbose = false;
    let mut changes = false;
    let args: Vec<String> = std::env::args().skip(1).collect();

    // strip flags
    let mut mode_str = None;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: chmod [OPTION]... MODE[,MODE]... FILE...\n\
 chmod [OPTION]... OCTAL-MODE FILE...\n\
 Change the mode of each FILE to MODE.\n\n\
 -R, --recursive change files and directories recursively\n\
 -v, --verbose output a diagnostic for every file processed\n\
 -c, --changes like verbose but report only when a change is made\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("chmod (user_utils) 0.1.0");
                return 0;
            }
            "-R" | "--recursive" => recursive = true,
            "-v" | "--verbose" => verbose = true,
            "-c" | "--changes" => changes = true,
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'R' => recursive = true,
                        'v' => verbose = true,
                        'c' => changes = true,
                        _ => {
                            eprintln!("chmod: invalid option -- '{c}'");
                            return 1;
                        }
                    }
                }
            }
            other => {
                if mode_str.is_none() {
                    mode_str = Some(other.to_string());
                } else {
                    paths.push(PathBuf::from(other));
                }
            }
        }
        i += 1;
    }

    let mode_str = match mode_str {
        Some(m) => m,
        None => {
            eprintln!("chmod: missing operand");
            return 1;
        }
    };
    if paths.is_empty() {
        eprintln!("chmod: missing operand after '{mode_str}'");
        return 1;
    }

    let mut status = 0;
    for p in &paths {
        if let Some(reason) = protect::modification_denied(p) {
            eprintln!(
                "chmod: changing permissions of '{}': {}",
                p.display(),
                reason.message()
            );
            status = 1;
            continue;
        }
        if let Err(e) = chmod_path(p, &mode_str, recursive, verbose, changes) {
            eprintln!("chmod: changing permissions of '{}': {e}", p.display());
            status = 1;
        }
    }
    status
}

fn chmod_path(
    path: &Path,
    mode_str: &str,
    recursive: bool,
    verbose: bool,
    changes: bool,
) -> std::io::Result<()> {
    apply_mode(path, mode_str, verbose, changes)?;
    // Use symlink_metadata (lstat), not path.is_dir() (stat): the latter
    // follows symlinks, so a symlink pointing at a directory — or worse, a
    // self-referential symlink — would be treated as a directory and
    // recursed into forever. Matching GNU chmod, -R never follows symlinks
    // during recursion.
    if recursive {
        let is_real_dir = fs::symlink_metadata(path)
            .map(|m| m.file_type().is_dir())
            .unwrap_or(false);
        if is_real_dir {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let p = entry.path();
                chmod_path(&p, mode_str, recursive, verbose, changes)?;
            }
        }
    }
    Ok(())
}

fn apply_mode(path: &Path, mode_str: &str, verbose: bool, changes: bool) -> std::io::Result<()> {
    let meta = fs::metadata(path)?;
    let old = meta.mode() & 0o7777;
    let new = parse_mode(mode_str, old)?;
    if new != old {
        fs::set_permissions(path, fs::Permissions::from_mode(new))?;
        if verbose || changes {
            println!(
                "mode of '{}' changed from {:04o} to {:04o}",
                path.display(),
                old,
                new
            );
        }
    } else if verbose {
        println!("mode of '{}' retained as {:04o}", path.display(), old);
    }
    Ok(())
}

fn parse_mode(s: &str, current: u32) -> std::io::Result<u32> {
    // octal?
    if s.chars().all(|c| c.is_ascii_digit()) {
        let v = u32::from_str_radix(s, 8)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        return Ok(v & 0o7777);
    }
    // symbolic: [ugoa]*[+-=][rwxXst]*
    let mut mode = current;
    for part in s.split(',') {
        mode = apply_symbolic(part, mode)?;
    }
    Ok(mode)
}

fn apply_symbolic(spec: &str, mut mode: u32) -> std::io::Result<u32> {
    if spec.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid mode",
        ));
    }
    let bytes = spec.as_bytes();
    let mut i = 0;
    let mut who = 0u32; // bitflags: u=4 g=2 o=1 a=7
    while i < bytes.len() && matches!(bytes[i], b'u' | b'g' | b'o' | b'a') {
        match bytes[i] {
            b'u' => who |= 4,
            b'g' => who |= 2,
            b'o' => who |= 1,
            b'a' => who |= 7,
            _ => {}
        }
        i += 1;
    }
    if who == 0 {
        who = 7; // default all
    }
    if i >= bytes.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid mode",
        ));
    }
    let op = bytes[i] as char;
    if !matches!(op, '+' | '-' | '=') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid mode: '{spec}'"),
        ));
    }
    i += 1;
    let mut bits = 0u32;
    while i < bytes.len() {
        match bytes[i] {
            b'r' => bits |= 0o444,
            b'w' => bits |= 0o222,
            b'x' => bits |= 0o111,
            b'X' => {
                if mode & 0o111 != 0 || (mode & 0o170000) == 0o040000 {
                    bits |= 0o111;
                }
            }
            b's' => {
                if who & 4 != 0 {
                    bits |= 0o4000;
                }
                if who & 2 != 0 {
                    bits |= 0o2000;
                }
            }
            b't' => bits |= 0o1000,
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("invalid mode: '{spec}'"),
                ));
            }
        }
        i += 1;
    }
    // mask bits by who
    let mut mask = 0u32;
    if who & 4 != 0 {
        mask |= 0o700 | 0o4000;
    }
    if who & 2 != 0 {
        mask |= 0o070 | 0o2000;
    }
    if who & 1 != 0 {
        mask |= 0o007 | 0o1000;
    }
    let apply_bits = bits & mask;
    match op {
        '+' => mode |= apply_bits,
        '-' => mode &= !apply_bits,
        '=' => {
            mode &= !mask;
            mode |= apply_bits;
        }
        _ => {}
    }
    Ok(mode & 0o7777)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("user_chmod_test_{tag}_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn recursive_chmod_does_not_hang_on_self_referential_symlink() {
        // Regression: chmod_path used path.is_dir() (follows symlinks), so a
        // symlink pointing back into its own parent directory caused
        // unbounded recursion during -R. symlink_metadata-based detection
        // must skip the symlink instead of descending into it.
        let dir = tmp_dir("symlink_loop");
        let loop_link = dir.join("loop");
        std::os::unix::fs::symlink(&dir, &loop_link).unwrap();

        let result = chmod_path(&dir, "755", true, false, false);
        assert!(result.is_ok(), "recursive chmod must terminate: {result:?}");

        fs::remove_file(&loop_link).ok();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recursive_chmod_applies_to_real_subdirectories() {
        let dir = tmp_dir("real_subdir");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("f.txt");
        fs::write(&file, b"x").unwrap();

        chmod_path(&dir, "700", true, false, false).unwrap();

        let mode = fs::metadata(&file).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o700);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_mode_octal() {
        assert_eq!(parse_mode("755", 0o644).unwrap(), 0o755);
    }

    #[test]
    fn parse_mode_symbolic_add() {
        assert_eq!(parse_mode("u+x", 0o644).unwrap(), 0o744);
    }

    #[test]
    fn parse_mode_symbolic_rejects_garbage() {
        assert!(parse_mode("u+z", 0o644).is_err());
        assert!(parse_mode("", 0o644).is_err());
    }
}
