//! user mkdir — create directories (POSIX `mkdir`, as Makefiles and configure scripts call it).
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `mkdir` utility. Parses `std::env::args()` and creates
/// each `DIRECTORY`; with `-p` also its missing parents, and no error when it
/// already exists as a directory. `-m MODE` (octal or symbolic) is set exactly on
/// the named directories, regardless of the umask.
///
/// Returns 0 on success, 1 if any directory could not be created or on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("mkdir");
    let mut parents = false;
    let mut verbose = false;
    let mut mode_arg: Option<String> = None;
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut operands_only = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if operands_only || arg == "-" || !arg.starts_with('-') {
            dirs.push(PathBuf::from(arg));
            continue;
        }
        match arg.as_str() {
            "--" => operands_only = true,
            "-h" | "--help" => {
                print!(
                    "Usage: mkdir [OPTION]... DIRECTORY...\n\
 Create the DIRECTORY(ies), if they do not already exist.\n\n\
 -m, --mode=MODE set file mode (as in chmod), not a=rwx - umask\n\
 -p, --parents no error if existing, make parent directories as needed\n\
 -v, --verbose print a message for each created directory\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("mkdir (user_utils) 0.2.0");
                return 0;
            }
            "-p" | "--parents" => parents = true,
            "-v" | "--verbose" => verbose = true,
            "-m" | "--mode" => match args.next() {
                Some(m) => mode_arg = Some(m),
                None => {
                    ui.err("option requires an argument -- 'm'");
                    return 1;
                }
            },
            s if s.starts_with("--mode=") => mode_arg = Some(s["--mode=".len()..].to_string()),
            s if s.starts_with("--") => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            s => {
                // bundled short options: -pv, -pm755, -m 755
                let rest: Vec<char> = s.chars().skip(1).collect();
                let mut i = 0;
                while i < rest.len() {
                    match rest[i] {
                        'p' => parents = true,
                        'v' => verbose = true,
                        'm' => {
                            let inline: String = rest[i + 1..].iter().collect();
                            mode_arg = if inline.is_empty() { args.next() } else { Some(inline) };
                            if mode_arg.is_none() {
                                ui.err("option requires an argument -- 'm'");
                                return 1;
                            }
                            break;
                        }
                        c => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                    i += 1;
                }
            }
        }
    }
    if dirs.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let mode = match mode_arg.as_deref().map(parse_mode) {
        None => None,
        Some(Ok(m)) => Some(m),
        Some(Err(())) => {
            ui.err(&format!("invalid mode '{}'", mode_arg.unwrap_or_default()));
            return 1;
        }
    };

    let mut status = 0;
    for d in &dirs {
        let made = if parents {
            make_with_parents(d, verbose)
        } else {
            make_one(d, verbose)
        };
        let result = made.and_then(|created| match (created, mode) {
            (true, Some(m)) => fs::set_permissions(d, fs::Permissions::from_mode(m)),
            _ => Ok(()),
        });
        if let Err(e) = result {
            ui.err(&format!(
                "cannot create directory '{}': {}",
                d.display(),
                reason(&e)
            ));
            status = 1;
        }
    }
    status
}

/// Creates `d`; an existing entry is an error. Returns whether it was created.
fn make_one(d: &Path, verbose: bool) -> io::Result<bool> {
    fs::create_dir(d)?;
    if verbose {
        println!("mkdir: created directory '{}'", d.display());
    }
    Ok(true)
}

/// Creates `d` and each missing ancestor; an existing directory is fine.
/// Returns whether `d` itself was created.
fn make_with_parents(d: &Path, verbose: bool) -> io::Result<bool> {
    if d.is_dir() {
        return Ok(false);
    }
    let mut missing: Vec<&Path> = d
        .ancestors()
        .take_while(|p| !p.as_os_str().is_empty() && !p.is_dir())
        .collect();
    missing.reverse();
    for p in missing {
        match fs::create_dir(p) {
            Ok(()) => {
                if verbose {
                    println!("mkdir: created directory '{}'", p.display());
                }
            }
            // another process made it in between
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && p.is_dir() => {}
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

/// The error text without Rust's " (os error N)" suffix, as other mkdirs print it.
fn reason(e: &io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// An octal mode (`755`, `0700`) or a symbolic one (`u=rwx,go=rx`, `a+w`, `+t`) applied
/// to a=rwx, as POSIX specifies for `mkdir -m`.
fn parse_mode(s: &str) -> Result<u32, ()> {
    if !s.is_empty() && s.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        let m = u32::from_str_radix(s, 8).map_err(|_| ())?;
        return if m <= 0o7777 { Ok(m) } else { Err(()) };
    }
    let mut mode: u32 = 0o777;
    for clause in s.split(',') {
        let who_end = clause.find(['+', '-', '=']).ok_or(())?;
        let who = &clause[..who_end];
        let mut mask = 0u32;
        for c in who.chars() {
            mask |= match c {
                'u' => 0o4700,
                'g' => 0o2070,
                'o' => 0o1007,
                'a' => 0o7777,
                _ => return Err(()),
            };
        }
        if who.is_empty() {
            mask = 0o7777;
        }
        let mut rest = &clause[who_end..];
        while let Some(op) = rest.chars().next() {
            rest = &rest[1..];
            let perm_end = rest.find(['+', '-', '=']).unwrap_or(rest.len());
            let mut bits = 0u32;
            for c in rest[..perm_end].chars() {
                bits |= match c {
                    'r' => 0o444,
                    'w' => 0o222,
                    'x' | 'X' => 0o111,
                    's' => 0o6000,
                    't' => 0o1000,
                    _ => return Err(()),
                };
            }
            bits &= mask;
            match op {
                '+' => mode |= bits,
                '-' => mode &= !bits,
                '=' => mode = (mode & !mask) | bits,
                _ => return Err(()),
            }
            rest = &rest[perm_end..];
        }
    }
    Ok(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octal_modes() {
        assert_eq!(parse_mode("755"), Ok(0o755));
        assert_eq!(parse_mode("0700"), Ok(0o700));
        assert_eq!(parse_mode("1777"), Ok(0o1777));
        assert!(parse_mode("99").is_err());
    }

    #[test]
    fn symbolic_modes_start_from_a_rwx() {
        assert_eq!(parse_mode("u=rwx,go=rx"), Ok(0o755));
        assert_eq!(parse_mode("go-w"), Ok(0o755));
        assert_eq!(parse_mode("a=rx"), Ok(0o555));
        assert_eq!(parse_mode("+t"), Ok(0o1777));
        assert!(parse_mode("q=r").is_err());
    }

    #[test]
    fn parents_create_the_whole_tree_and_accept_existing() {
        let base = std::env::temp_dir().join(format!("user-mkdir-test-{}", std::process::id()));
        let deep = base.join("a/b/c");
        assert!(make_with_parents(&deep, false).unwrap());
        assert!(deep.is_dir());
        assert!(!make_with_parents(&deep, false).unwrap());
        assert!(make_one(&deep, false).is_err());
        fs::remove_dir_all(&base).unwrap();
    }
}
