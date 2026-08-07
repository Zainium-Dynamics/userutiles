//! user tee — read from standard input and write to standard output and files.
use std::fs::OpenOptions;
use std::io::{self, Read, Write};

use std::path::Path;
use usercore::{protect, Ui};

/// Entry point for the `tee` utility. Parses `std::env::args()`, opens each
/// named file (creating/truncating or appending per `-a`), then copies
/// stdin to stdout and every opened file.
///
/// Returns 0 on success, 1 if any file could not be opened or a write
/// failed (other than a broken pipe on stdout, which is treated as
/// pipeline-normal).
pub fn run() -> i32 {
    let ui = Ui::new("tee");
    let mut append = false;
    let mut files: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("tee (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--append" => append = true,
            // SAFETY: `libc::signal` takes two plain integer arguments (a signal
            // number and either a function pointer or one of the sentinel
            // constants `SIG_IGN`/`SIG_DFL`); it performs no pointer
            // dereference on the Rust side and cannot fail in a way that
            // corrupts memory. Installing SIG_IGN for SIGINT only changes how
            // the process handles that signal going forward.
            "-i" | "--ignore-interrupts" => unsafe {
                libc::signal(libc::SIGINT, libc::SIG_IGN);
            },
            s if s.starts_with('-') && s != "-" => {
                for c in s.chars().skip(1) {
                    match c {
                        'a' => append = true,
                        // SAFETY: same as above — `libc::signal(SIGINT, SIG_IGN)`
                        // takes no pointers and cannot be unsound regardless of
                        // process state.
                        'i' => unsafe {
                            libc::signal(libc::SIGINT, libc::SIG_IGN);
                        },
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            other => files.push(other.to_string()),
        }
    }

    let mut outputs: Vec<Box<dyn Write>> = Vec::new();
    outputs.push(Box::new(io::stdout()));
    let mut status = 0;
    for f in &files {
        match open_output(f, append) {
            Ok(file) => outputs.push(Box::new(file)),
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }

    let mut stdin = io::stdin().lock();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = match stdin.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                ui.err(&e.to_string());
                return 1;
            }
        };
        for out in outputs.iter_mut() {
            if let Err(e) = out.write_all(&buf[..n]) {
                if e.kind() != io::ErrorKind::BrokenPipe {
                    ui.err(&e.to_string());
                    status = 1;
                }
            }
        }
    }
    for out in outputs.iter_mut() {
        let _ = out.flush();
    }
    status
}

fn print_help() {
    print!(
        "Usage: tee [OPTION]... [FILE]...\n\
 Copy standard input to each FILE, and also to standard output.\n\n\
 -a, --append append to the given FILEs, do not overwrite\n\
 -i, --ignore-interrupts ignore interrupt signals\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Open `path` for `tee` output: created if missing, truncated unless
/// `append` is set (in which case writes are appended instead).
fn open_output(path: &str, append: bool) -> io::Result<std::fs::File> {
    if let Some(reason) = protect::modification_denied(Path::new(path)) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason.message()));
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("user_tee_test_{}_{name}", std::process::id()))
    }

    #[test]
    fn open_output_creates_and_truncates_by_default() {
        let p = tmp("truncate");
        fs::write(&p, b"old contents").unwrap();
        {
            let mut f = open_output(p.to_str().unwrap(), false).unwrap();
            f.write_all(b"new").unwrap();
        }
        assert_eq!(fs::read(&p).unwrap(), b"new");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn open_output_appends_when_requested() {
        let p = tmp("append");
        fs::write(&p, b"old-").unwrap();
        {
            let mut f = open_output(p.to_str().unwrap(), true).unwrap();
            f.write_all(b"new").unwrap();
        }
        assert_eq!(fs::read(&p).unwrap(), b"old-new");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn open_output_in_missing_directory_errors() {
        let missing = tmp("missing_dir").join("file.txt");
        assert!(open_output(missing.to_str().unwrap(), false).is_err());
    }
}
