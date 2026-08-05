//! user md5sum — print or check 128-bit checksums.
use std::io;

use usercore::Ui;

/// Entry point for the `md5sum` utility. Parses `std::env::args()` and
/// either prints an MD5 digest for each FILE (default; `-` or no operand
/// means stdin), or, with `-c`/`--check`, reads each FILE as a list of
/// previously-generated `hash  filename` lines and re-verifies them.
///
/// Returns 0 if every file was hashed/verified successfully, 1 otherwise.
pub fn run() -> i32 {
    let ui = Ui::new("md5sum");
    let mut check = false;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
 "Usage: md5sum [OPTION]... [FILE]...\n Print or check 128-bit checksums.\n -c, --check read checksums from the FILEs and check them\n With no FILE, or when FILE is -, read standard input.\n"
 );
                return 0;
            }
            "--version" => {
                println!("md5sum (user_utils) 0.1.0");
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

/// Compute the MD5 digest of `path` (`-` reads stdin) as a lowercase hex
/// string.
fn hash_file(path: &str) -> io::Result<String> {
    let mut h = usercore::digest::Md5::new();
    usercore::digest::hash_path_update(path, |chunk| h.update(chunk))?;
    Ok(usercore::digest::hex_lower(&h.finalize()))
}

/// Split a `md5sum`-format checksum line into `(hash, filename)`.
///
/// GNU `md5sum` lines are `HASH SPFILENAME`, where `SP` is a single space
/// and the following byte is a mode indicator: a plain space for text
/// mode or `*` for binary mode. Naively splitting on the first space
/// (as an earlier version of this function did) leaves that indicator
/// byte stuck onto the front of the filename — a leading space or `*` —
/// which then fails to open as a real path. This strips exactly one
/// indicator byte after the separating space.
fn parse_check_line(line: &str) -> Option<(&str, &str)> {
    let (hash, rest) = line.split_once(' ')?;
    if hash.is_empty() || rest.is_empty() {
        return None;
    }
    let file = rest.strip_prefix('*').or_else(|| rest.strip_prefix(' ')).unwrap_or(rest);
    if file.is_empty() {
        return None;
    }
    Some((hash, file))
}

/// Implements `-c`/`--check`: read each entry in `files` as a checksum
/// list and re-hash the named file(s), printing `OK`/`FAILED` per entry.
///
/// Returns 0 if every entry verified and every list file was readable,
/// 1 otherwise.
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

    #[test]
    fn hash_file_known_vector() {
        // MD5("") == d41d8cd98f00b204e9800998ecf8427e
        let dir = std::env::temp_dir().join(format!("user_md5sum_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.txt");
        std::fs::write(&path, b"").unwrap();
        let h = hash_file(path.to_str().unwrap()).unwrap();
        assert_eq!(h, "d41d8cd98f00b204e9800998ecf8427e");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_file_missing_path_errors() {
        let missing = format!("/nonexistent_user_md5sum_test_{}", std::process::id());
        assert!(hash_file(&missing).is_err());
    }

    #[test]
    fn parse_check_line_text_mode_two_spaces() {
        // Regression: a naive split_once(' ') used to leave a leading
        // space on the filename here.
        let line = "d41d8cd98f00b204e9800998ecf8427e  file.txt";
        let (hash, file) = parse_check_line(line).unwrap();
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(file, "file.txt");
    }

    #[test]
    fn parse_check_line_binary_mode_asterisk() {
        // Regression: a naive split_once(' ') used to leave a leading
        // '*' on the filename here.
        let line = "d41d8cd98f00b204e9800998ecf8427e *file.bin";
        let (hash, file) = parse_check_line(line).unwrap();
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(file, "file.bin");
    }

    #[test]
    fn parse_check_line_single_space_single_word_filename() {
        let line = "abc123 file.txt";
        let (hash, file) = parse_check_line(line).unwrap();
        assert_eq!(hash, "abc123");
        assert_eq!(file, "file.txt");
    }

    #[test]
    fn parse_check_line_rejects_malformed_input() {
        assert_eq!(parse_check_line("no-space-here"), None);
        assert_eq!(parse_check_line("hash "), None);
        assert_eq!(parse_check_line(" file.txt"), None);
    }
}
