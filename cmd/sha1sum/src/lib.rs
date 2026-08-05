//! user sha1sum — print or check 160-bit checksums.
use std::io;

use usercore::Ui;

/// Entry point for the `sha1sum` utility. Parses `std::env::args()` and
/// either prints a `HASH  FILE` line per input (default), or (`-c`)
/// reads such lines from the given file(s) and verifies them.
///
/// Returns 0 if all requested hashes/verifications succeeded, 1 if any
/// file failed to hash, was missing, or (in `-c` mode) didn't match.
pub fn run() -> i32 {
    let ui = Ui::new("sha1sum");
    let mut check = false;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
 "Usage: sha1sum [OPTION]... [FILE]...\n Print or check 160-bit checksums.\n -c, --check read checksums from the FILEs and check them\n With no FILE, or when FILE is -, read standard input.\n"
 );
                return 0;
            }
            "--version" => {
                println!("sha1sum (user_utils) 0.1.0");
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
        return check_mode(&ui, &files);
    }
    let mut status = 0;
    for f in &files {
        match hash_file(f) {
            Ok(h) => println!("{h}  {f}"),
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }
    status
}

/// Hash the contents of `path` (`-` for stdin) and return its lower-case
/// hex digest.
fn hash_file(path: &str) -> io::Result<String> {
    let mut h = usercore::digest::Sha1::new();
    usercore::digest::hash_path_update(path, |chunk| h.update(chunk))?;
    Ok(usercore::digest::hex_lower(&h.finalize()))
}

/// Parse one `HASH  FILE` or `HASH *FILE` checksum-list line (GNU
/// coreutils' text/binary-mode marker after the single mandatory space)
/// into `(hash, file)`. Falls back to treating everything after the
/// first space as the filename if there's no mode-marker character,
/// so plain `HASH FILE` lines (a single space, no marker) still parse.
fn parse_checksum_line(line: &str) -> Option<(&str, &str)> {
    let (hash, rest) = line.split_once(' ')?;
    let file = rest
        .strip_prefix('*')
        .or_else(|| rest.strip_prefix(' '))
        .unwrap_or(rest);
    if hash.is_empty() || file.is_empty() {
        return None;
    }
    Some((hash, file))
}

/// Read each file in `files` (`-` for stdin) as a checksum list and
/// verify every non-comment, non-blank line against the actual hash of
/// the named file, printing `FILE: OK` or `FILE: FAILED` per GNU
/// `sha1sum -c` convention.
///
/// Returns 1 if any list file couldn't be read, any line was malformed,
/// any referenced file failed to hash, or any hash mismatched; 0 if
/// every check passed.
fn check_mode(ui: &Ui, files: &[String]) -> i32 {
    let mut status = 0;
    for list in files {
        let data = if list == "-" {
            let mut s = String::new();
            if let Err(e) = io::Read::read_to_string(&mut io::stdin(), &mut s) {
                ui.err(&format!("-: {e}"));
                status = 1;
                continue;
            }
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
            let Some((hash, file)) = parse_checksum_line(line) else {
                ui.err(&format!("invalid line: '{line}'"));
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

    // Vectors cross-checked against the host system's coreutils sha1sum.
    #[test]
    fn empty_input_vector() {
        let mut h = usercore::digest::Sha1::new();
        h.update(b"");
        assert_eq!(
            usercore::digest::hex_lower(&h.finalize()),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn abc_vector() {
        let mut h = usercore::digest::Sha1::new();
        h.update(b"abc");
        assert_eq!(
            usercore::digest::hex_lower(&h.finalize()),
            "a9993e364706816aba3e25717850c26c9cd0d89"
        );
    }

    #[test]
    fn hash_file_reads_real_file() {
        let dir = std::env::temp_dir().join(format!("user_sha1sum_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("in.txt");
        std::fs::write(&path, b"abc").unwrap();
        let h = hash_file(path.to_str().unwrap()).unwrap();
        assert_eq!(h, "a9993e364706816aba3e25717850c26c9cd0d89");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_file_missing_file_errors() {
        assert!(hash_file("/nonexistent/user-sha1sum-missing").is_err());
    }

    #[test]
    fn parse_checksum_line_text_mode_two_spaces() {
        let (h, f) = parse_checksum_line("da39a3ee  myfile").unwrap();
        assert_eq!(h, "da39a3ee");
        assert_eq!(f, "myfile");
    }

    #[test]
    fn parse_checksum_line_binary_mode_asterisk() {
        let (h, f) = parse_checksum_line("da39a3ee *myfile").unwrap();
        assert_eq!(h, "da39a3ee");
        assert_eq!(f, "myfile");
    }

    #[test]
    fn parse_checksum_line_single_space_no_marker() {
        let (h, f) = parse_checksum_line("da39a3ee myfile").unwrap();
        assert_eq!(h, "da39a3ee");
        assert_eq!(f, "myfile");
    }

    #[test]
    fn parse_checksum_line_rejects_missing_space() {
        assert!(parse_checksum_line("da39a3eemyfile").is_none());
    }

    #[test]
    fn parse_checksum_line_rejects_empty() {
        assert!(parse_checksum_line("").is_none());
    }
}
