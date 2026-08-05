//! user b2sum — print or check 512-bit BLAKE2b checksums.
use std::io;

use usercore::Ui;

/// CLI entry point: parses arguments, hashes (or checks) the given files,
/// and returns the process exit code.
pub fn run() -> i32 {
    let ui = Ui::new("b2sum");
    let mut check = false;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: b2sum [OPTION]... [FILE]...\n\
Print or check 512-bit checksums.\n\
  -c, --check   read checksums from the FILEs and check them\n\
With no FILE, or when FILE is -, read standard input.\n"
                );
                return 0;
            }
            "--version" => {
                println!("b2sum (user_utils) 0.1.0");
                return 0;
            }
            "-c" | "--check" => check = true,
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }
    if check {
        return check_mode(&files, &ui);
    }
    let mut status = 0;
    for f in &files {
        match hash_file(f) {
            Ok(h) => println!("{h} {f}"),
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }
    status
}

/// Compute the lowercase-hex BLAKE2b-512 digest of `path` (`-` means stdin).
pub fn hash_file(path: &str) -> io::Result<String> {
    let mut h = usercore::digest::Blake2b::new();
    usercore::digest::hash_path_update(path, |chunk| h.update(chunk))?;
    Ok(usercore::digest::hex_lower(&h.finalize()))
}

/// Parse a single `b2sum -c` checksum-list line into `(hash, filename)`.
///
/// GNU-style lines are `<hash> <mode><filename>`, where `<mode>` is a space
/// for text mode or `*` for binary mode; either way it must be stripped from
/// the filename rather than left as a stray leading character.
fn parse_check_line(line: &str) -> Option<(&str, &str)> {
    let (hash, rest) = line.split_once(' ')?;
    let file = rest
        .strip_prefix('*')
        .or_else(|| rest.strip_prefix(' '))
        .unwrap_or(rest);
    Some((hash, file))
}

fn check_mode(files: &[String], ui: &Ui) -> i32 {
    let mut status = 0;
    for list in files {
        let data = if list == "-" {
            let mut s = String::new();
            let _ = io::Read::read_to_string(&mut io::stdin(), &mut s);
            s
        } else {
            match std::fs::read_to_string(list) {
                Ok(s) => s,
                Err(e) => {
                    ui.err(&format!("{list}: {e}"));
                    status = 1;
                    continue;
                }
            }
        };
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((hash, file)) = parse_check_line(line) else {
                ui.err("invalid line");
                status = 1;
                continue;
            };
            match hash_file(file) {
                Ok(h) if h.eq_ignore_ascii_case(hash) => println!("{file}: OK"),
                Ok(_) => {
                    println!("{file}: FAILED");
                    status = 1;
                }
                Err(e) => {
                    ui.err(&format!("{file}: {e}"));
                    status = 1;
                }
            }
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("user_b2sum_test_{tag}_{}", std::process::id()))
    }

    #[test]
    fn hash_file_known_vector() {
        // BLAKE2b-512 of the empty string is a well-known constant.
        let p = tmp_path("empty");
        fs::write(&p, b"").unwrap();
        let h = hash_file(p.to_str().unwrap()).unwrap();
        assert_eq!(
            h,
            "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419\
d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
        );
        fs::remove_file(&p).ok();
    }

    #[test]
    fn hash_file_missing_file_errors() {
        let p = tmp_path("missing");
        fs::remove_file(&p).ok();
        assert!(hash_file(p.to_str().unwrap()).is_err());
    }

    #[test]
    fn parse_check_line_text_mode_strips_leading_space() {
        assert_eq!(
            parse_check_line("deadbeef  file.txt"),
            Some(("deadbeef", "file.txt"))
        );
    }

    #[test]
    fn parse_check_line_binary_mode_strips_star() {
        assert_eq!(
            parse_check_line("deadbeef *file.txt"),
            Some(("deadbeef", "file.txt"))
        );
    }

    #[test]
    fn parse_check_line_rejects_garbage() {
        assert_eq!(parse_check_line("no-separator-here"), None);
    }
}
