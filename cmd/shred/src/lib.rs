//! user shred — securely overwrite and optionally delete files.
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use usercore::Ui;

/// Entry point for the `shred` utility. Parses `std::env::args()` and
/// overwrites each given file `-n` times (default 3) with random data,
/// optionally following up with a final zero pass (`-z`) and/or removing
/// the file afterward (`-u`).
///
/// Returns 0 on success, 1 on a usage or I/O error. Unlike the original
/// implementation, an unparsable `-n`/`-s` argument is now a hard error
/// rather than silently falling back to a default iteration count or the
/// file's own size.
pub fn run() -> i32 {
    let ui = Ui::new("shred");
    let mut iterations = 3usize;
    let mut remove = false;
    let mut zero = false;
    let mut verbose = false;
    let mut size: Option<u64> = None;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: shred [OPTION]... FILE...\nOverwrite the specified FILE(s) repeatedly, to make it harder to recover.\n -n, --iterations=N overwrite N times (default 3)\n -u, --remove truncate and remove file after overwriting\n -z, --zero add a final overwrite with zeros\n -v, --verbose show progress\n -s, --size=N shred this many bytes\n");
                return 0;
            }
            "--version" => {
                println!("shred (user_utils) 0.1.0");
                return 0;
            }
            "-u" | "--remove" => remove = true,
            "-z" | "--zero" => zero = true,
            "-v" | "--verbose" => verbose = true,
            "-n" | "--iterations" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'n'");
                    return 1;
                };
                match arg.parse() {
                    Ok(n) => iterations = n,
                    Err(_) => {
                        ui.err(&format!("invalid number of iterations: '{arg}'"));
                        return 1;
                    }
                }
            }
            "-s" | "--size" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 's'");
                    return 1;
                };
                match parse_size(arg) {
                    Some(n) => size = Some(n),
                    None => {
                        ui.err(&format!("invalid size: '{arg}'"));
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }
    if files.is_empty() {
        ui.err("missing file operand");
        return 1;
    }
    let mut status = 0;
    for f in &files {
        if let Err(e) = shred_file(Path::new(f), iterations, zero, remove, verbose, size) {
            ui.err(&format!("{f}: {e}"));
            status = 1;
        }
    }
    status
}

/// Parse a `-s`/`--size` byte count, accepting an optional `K`/`M`/`G`
/// (powers of 1024) suffix in addition to plain digits. Multiplication
/// overflow is rejected rather than wrapping.
fn parse_size(s: &str) -> Option<u64> {
    let last = *s.as_bytes().last()?;
    if last.is_ascii_digit() {
        return s.parse().ok();
    }
    let mult = match last.to_ascii_uppercase() {
        b'K' => 1024u64,
        b'M' => 1024 * 1024,
        b'G' => 1024 * 1024 * 1024,
        _ => return None,
    };
    let n: u64 = s[..s.len() - 1].parse().ok()?;
    n.checked_mul(mult)
}

fn shred_file(
    path: &Path,
    iterations: usize,
    zero: bool,
    remove: bool,
    verbose: bool,
    size: Option<u64>,
) -> std::io::Result<()> {
    let meta = fs::metadata(path)?;
    let len = size.unwrap_or(meta.len());
    let mut file = OpenOptions::new().write(true).open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    for pass in 0..iterations {
        if verbose {
            eprintln!("shred: {path:?}: pass {}/{iterations}", pass + 1);
        }
        file.seek(SeekFrom::Start(0))?;
        let mut left = len;
        while left > 0 {
            let n = (left as usize).min(buf.len());
            fill_random(&mut buf[..n]);
            file.write_all(&buf[..n])?;
            left -= n as u64;
        }
        file.sync_all()?;
    }
    if zero {
        if verbose {
            eprintln!("shred: {path:?}: zeroing");
        }
        file.seek(SeekFrom::Start(0))?;
        let mut left = len;
        buf.fill(0);
        while left > 0 {
            let n = (left as usize).min(buf.len());
            file.write_all(&buf[..n])?;
            left -= n as u64;
        }
        file.sync_all()?;
    }
    if remove {
        drop(file);
        let _ = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|f| f.set_len(0));
        fs::remove_file(path)?;
    }
    Ok(())
}

fn fill_random(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        // SAFETY: `rand(3)` takes no arguments, dereferences no
        // pointers, and always returns an `int` — it cannot fail or
        // invoke UB regardless of prior seeding state.
        *b = (unsafe { libc::rand() } & 0xff) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_plain_digits() {
        assert_eq!(parse_size("1024"), Some(1024));
    }

    #[test]
    fn parse_size_suffixes() {
        assert_eq!(parse_size("1K"), Some(1024));
        assert_eq!(parse_size("2M"), Some(2 * 1024 * 1024));
        assert_eq!(parse_size("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size("1k"), Some(1024));
    }

    #[test]
    fn parse_size_rejects_unknown_suffix() {
        assert_eq!(parse_size("10X"), None);
    }

    #[test]
    fn parse_size_rejects_empty() {
        assert_eq!(parse_size(""), None);
    }

    #[test]
    fn parse_size_rejects_overflow() {
        assert_eq!(parse_size("99999999999999999999G"), None);
    }

    #[test]
    fn shred_file_overwrites_and_preserves_length_by_default() {
        let dir = std::env::temp_dir().join(format!("user_shred_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.txt");
        fs::write(&path, b"hello world").unwrap();
        let original_len = fs::metadata(&path).unwrap().len();

        shred_file(&path, 1, false, false, false, None).unwrap();

        let new_len = fs::metadata(&path).unwrap().len();
        assert_eq!(new_len, original_len);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shred_file_zero_pass_writes_zero_bytes() {
        let dir = std::env::temp_dir().join(format!("user_shred_test2_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.txt");
        fs::write(&path, b"hello world").unwrap();

        shred_file(&path, 1, true, false, false, None).unwrap();

        let contents = fs::read(&path).unwrap();
        assert!(contents.iter().all(|&b| b == 0));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shred_file_remove_deletes_file() {
        let dir = std::env::temp_dir().join(format!("user_shred_test3_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.txt");
        fs::write(&path, b"hello world").unwrap();

        shred_file(&path, 1, false, true, false, None).unwrap();

        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shred_file_missing_file_errors() {
        let result = shred_file(Path::new("/nonexistent/user-shred-missing"), 1, false, false, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn shred_file_explicit_size_limits_bytes_touched() {
        let dir = std::env::temp_dir().join(format!("user_shred_test4_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.txt");
        fs::write(&path, vec![b'A'; 100]).unwrap();

        // Shred only the first 10 bytes with zeros; the rest should be
        // untouched since size caps how many bytes the pass writes.
        shred_file(&path, 1, true, false, false, Some(10)).unwrap();

        let contents = fs::read(&path).unwrap();
        assert_eq!(contents.len(), 100);
        assert!(contents[..10].iter().all(|&b| b == 0));
        assert!(contents[10..].iter().all(|&b| b == b'A'));
        let _ = fs::remove_dir_all(&dir);
    }
}
