//! user find — search for files in a directory hierarchy.
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone)]
enum Pred {
    Name(String),
    Path(String),
    Type(char),
    Empty,
    Executable,
    Size { op: char, bytes: u64 },
    Mtime { op: char, days: i64 },
    Not(Box<Pred>),
}

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!(
            "Usage: find [PATH]... [EXPRESSION]\n\
 Search for files in a directory hierarchy.\n\n\
 Expressions:\n\
 -name PATTERN base of file name matches shell pattern\n\
 -path PATTERN path matches shell pattern\n\
 -type c file is of type c (f,d,l,c,b,p,s)\n\
 -empty file is empty and is either a regular file or directory\n\
 -executable matches files which are executable\n\
 -size N[cwbkMG] file uses n units of space\n\
 -mtime N file data was last modified n*24 hours ago\n\
 -maxdepth N descend at most N levels\n\
 -mindepth N do not apply tests at levels less than N\n\
 -not / ! negate next test\n\
 -print print (default)\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
        );
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("find (user_utils) 0.1.0");
        return 0;
    }

    let mut paths: Vec<PathBuf> = Vec::new();
    let mut preds: Vec<Pred> = Vec::new();
    let mut max_depth = usize::MAX;
    let mut min_depth = 0usize;
    let mut i = 0;
    // collect paths first until expression starts with -
    while i < args.len() {
        if args[i].starts_with('-') || args[i] == "!" {
            break;
        }
        paths.push(PathBuf::from(&args[i]));
        i += 1;
    }
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }

    while i < args.len() {
        match args[i].as_str() {
            "-name" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to `-name'");
                    return 1;
                }
                preds.push(Pred::Name(args[i].clone()));
            }
            "-path" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to `-path'");
                    return 1;
                }
                preds.push(Pred::Path(args[i].clone()));
            }
            "-type" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to `-type'");
                    return 1;
                }
                let c = args[i].chars().next().unwrap_or('?');
                preds.push(Pred::Type(c));
            }
            "-empty" => preds.push(Pred::Empty),
            "-executable" => preds.push(Pred::Executable),
            "-size" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to `-size'");
                    return 1;
                }
                match parse_size(&args[i]) {
                    Ok(v) => preds.push(Pred::Size {
                        op: v.0,
                        bytes: v.1,
                    }),
                    Err(e) => {
                        eprintln!("find: {e}");
                        return 1;
                    }
                }
            }
            "-mtime" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to `-mtime'");
                    return 1;
                }
                match parse_n(&args[i]) {
                    Ok((op, n)) => preds.push(Pred::Mtime { op, days: n }),
                    Err(e) => {
                        eprintln!("find: {e}");
                        return 1;
                    }
                }
            }
            "-maxdepth" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to `-maxdepth'");
                    return 1;
                }
                match parse_depth(&args[i]) {
                    Ok(n) => max_depth = n,
                    Err(_) => {
                        eprintln!("find: invalid argument `{}' to `-maxdepth'", args[i]);
                        return 1;
                    }
                }
            }
            "-mindepth" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: missing argument to `-mindepth'");
                    return 1;
                }
                match parse_depth(&args[i]) {
                    Ok(n) => min_depth = n,
                    Err(_) => {
                        eprintln!("find: invalid argument `{}' to `-mindepth'", args[i]);
                        return 1;
                    }
                }
            }
            "-not" | "!" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("find: expected expression after '!'");
                    return 1;
                }
                // parse single next predicate simply
                // re-process one step by pushing Not of simplified
                // For simplicity, only support -not -name etc via recursive small parse
                let (pred, ni) = parse_one(&args, i);
                match pred {
                    Some(p) => {
                        preds.push(Pred::Not(Box::new(p)));
                        i = ni;
                        continue;
                    }
                    None => {
                        eprintln!("find: invalid expression after '!'");
                        return 1;
                    }
                }
            }
            "-print" => {}
            other => {
                eprintln!("find: unknown predicate `{other}'");
                return 1;
            }
        }
        i += 1;
    }

    let mut status = 0;
    let mut out = io::stdout().lock();
    for p in &paths {
        if let Err(e) = walk(p, p, 0, min_depth, max_depth, &preds, &mut out) {
            eprintln!("find: {}: {e}", p.display());
            status = 1;
        }
    }
    status
}

fn parse_one(args: &[String], i: usize) -> (Option<Pred>, usize) {
    if i >= args.len() {
        return (None, i);
    }
    match args[i].as_str() {
        "-name" if i + 1 < args.len() => (Some(Pred::Name(args[i + 1].clone())), i + 1),
        "-path" if i + 1 < args.len() => (Some(Pred::Path(args[i + 1].clone())), i + 1),
        "-type" if i + 1 < args.len() => (
            Some(Pred::Type(args[i + 1].chars().next().unwrap_or('?'))),
            i + 1,
        ),
        "-empty" => (Some(Pred::Empty), i),
        "-executable" => (Some(Pred::Executable), i),
        _ => (None, i),
    }
}

fn parse_size(s: &str) -> Result<(char, u64), String> {
    let (op, rest) = match s.chars().next() {
        Some('+') => ('+', &s[1..]),
        Some('-') => ('-', &s[1..]),
        _ => ('=', s),
    };
    let (num, mult) = match rest.chars().last() {
        Some('c') => (&rest[..rest.len() - 1], 1u64),
        Some('w') => (&rest[..rest.len() - 1], 2),
        Some('b') | None
            if rest
                .chars()
                .last()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false) =>
        {
            (rest, 512)
        }
        Some('k') | Some('K') => (&rest[..rest.len() - 1], 1024),
        Some('M') => (&rest[..rest.len() - 1], 1024 * 1024),
        Some('G') => (&rest[..rest.len() - 1], 1024 * 1024 * 1024),
        Some('b') => (&rest[..rest.len() - 1], 512),
        _ => {
            // pure number means 512-byte blocks
            (rest, 512)
        }
    };
    let n: u64 = num.parse().map_err(|_| format!("invalid size `{s}'"))?;
    Ok((op, n * mult))
}

fn parse_n(s: &str) -> Result<(char, i64), String> {
    let (op, rest) = match s.chars().next() {
        Some('+') => ('+', &s[1..]),
        Some('-') => ('-', &s[1..]),
        _ => ('=', s),
    };
    let n: i64 = rest.parse().map_err(|_| format!("invalid number `{s}'"))?;
    Ok((op, n))
}

/// Parse a `-maxdepth`/`-mindepth` argument. Previously malformed input
/// silently defaulted to `0` via `unwrap_or(0)`, which is indistinguishable
/// from the user explicitly asking for depth 0 — now returns an error so the
/// caller can reject it instead of quietly changing what gets searched.
fn parse_depth(s: &str) -> Result<usize, String> {
    s.parse().map_err(|_| format!("invalid depth `{s}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_depth_accepts_valid_numbers() {
        assert_eq!(parse_depth("0"), Ok(0));
        assert_eq!(parse_depth("3"), Ok(3));
    }

    #[test]
    fn parse_depth_rejects_garbage_instead_of_defaulting_to_zero() {
        // Regression: this used to silently become 0 via unwrap_or(0).
        assert!(parse_depth("abc").is_err());
        assert!(parse_depth("-1").is_err());
        assert!(parse_depth("").is_err());
        assert!(parse_depth("3.5").is_err());
    }

    #[test]
    fn parse_size_units() {
        assert_eq!(parse_size("10c"), Ok(('=', 10)));
        assert_eq!(parse_size("+2k"), Ok(('+', 2048)));
        assert_eq!(parse_size("-1M"), Ok(('-', 1024 * 1024)));
    }

    #[test]
    fn parse_n_signs() {
        assert_eq!(parse_n("+7"), Ok(('+', 7)));
        assert_eq!(parse_n("-3"), Ok(('-', 3)));
        assert_eq!(parse_n("5"), Ok(('=', 5)));
        assert!(parse_n("nope").is_err());
    }
}

fn walk(
    root: &Path,
    path: &Path,
    depth: usize,
    min_depth: usize,
    max_depth: usize,
    preds: &[Pred],
    out: &mut impl Write,
) -> io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    if depth >= min_depth && match_all(path, preds)? {
        writeln!(out, "{}", path.display())?;
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    if meta.is_dir() && !meta.file_type().is_symlink() && depth < max_depth {
        let rd = match fs::read_dir(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("find: {}: {e}", path.display());
                return Ok(());
            }
        };
        for ent in rd {
            let ent = ent?;
            walk(
                root,
                &ent.path(),
                depth + 1,
                min_depth,
                max_depth,
                preds,
                out,
            )?;
        }
    }
    Ok(())
}

fn match_all(path: &Path, preds: &[Pred]) -> io::Result<bool> {
    if preds.is_empty() {
        return Ok(true);
    }
    for p in preds {
        if !match_pred(path, p)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn match_pred(path: &Path, pred: &Pred) -> io::Result<bool> {
    match pred {
        Pred::Not(inner) => Ok(!match_pred(path, inner)?),
        Pred::Name(pat) => {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(glob_match(pat, &name))
        }
        Pred::Path(pat) => Ok(glob_match(pat, &path.to_string_lossy())),
        Pred::Type(c) => {
            let meta = fs::symlink_metadata(path)?;
            let ft = meta.file_type();
            Ok(match c {
                'f' => ft.is_file(),
                'd' => ft.is_dir(),
                'l' => ft.is_symlink(),
                'c' => ft.is_char_device(),
                'b' => ft.is_block_device(),
                'p' => ft.is_fifo(),
                's' => ft.is_socket(),
                _ => false,
            })
        }
        Pred::Empty => {
            let meta = fs::symlink_metadata(path)?;
            if meta.is_file() {
                Ok(meta.len() == 0)
            } else if meta.is_dir() {
                Ok(fs::read_dir(path)?.next().is_none())
            } else {
                Ok(false)
            }
        }
        Pred::Executable => {
            let meta = fs::metadata(path)?;
            Ok(meta.permissions().mode() & 0o111 != 0)
        }
        Pred::Size { op, bytes } => {
            let meta = fs::symlink_metadata(path)?;
            let len = meta.len();
            Ok(cmp_u64(*op, len, *bytes))
        }
        Pred::Mtime { op, days } => {
            let meta = fs::symlink_metadata(path)?;
            let mtime = meta.mtime();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let age_days = (now - mtime) / 86400;
            Ok(cmp_i64(*op, age_days, *days))
        }
    }
}

fn cmp_u64(op: char, val: u64, n: u64) -> bool {
    match op {
        '+' => val > n,
        '-' => val < n,
        _ => val == n,
    }
}

fn cmp_i64(op: char, val: i64, n: i64) -> bool {
    match op {
        '+' => val > n,
        '-' => val < n,
        _ => val == n,
    }
}

/// Simple glob: * and ? only.
fn glob_match(pat: &str, text: &str) -> bool {
    fn rec(p: &[u8], t: &[u8]) -> bool {
        let mut pi = 0;
        let mut ti = 0;
        let mut star_p = None;
        let mut star_t = 0;
        while ti < t.len() {
            if pi < p.len() && (p[pi] == t[ti] || p[pi] == b'?') {
                pi += 1;
                ti += 1;
            } else if pi < p.len() && p[pi] == b'*' {
                star_p = Some(pi);
                star_t = ti;
                pi += 1;
            } else if let Some(sp) = star_p {
                pi = sp + 1;
                star_t += 1;
                ti = star_t;
            } else {
                return false;
            }
        }
        while pi < p.len() && p[pi] == b'*' {
            pi += 1;
        }
        pi == p.len()
    }
    rec(pat.as_bytes(), text.as_bytes())
}
