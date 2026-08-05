//! user cat — concatenate FILE(s) to standard output.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

use usercore::Ui;

/// Entry point for the `cat` utility. Parses `std::env::args()` and
/// writes each `FILE` (or standard input, with no operands or `-`) to
/// standard output, applying any of the numbering/display options.
///
/// Returns 0 on success, 1 if any file could not be read or an unknown
/// option was given.
pub fn run() -> i32 {
    let ui = Ui::new("cat");
    let mut number = false;
    let mut number_nonblank = false;
    let mut show_ends = false;
    let mut show_tabs = false;
    let mut squeeze = false;
    let mut show_nonprint = false;
    let mut files: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("cat (user_utils) 0.1.0");
                return 0;
            }
            "-u" => {} // ignored (POSIX historical)
            "--" => {
                files.extend(args);
                break;
            }
            s if s.starts_with("--") => match s {
                "--number" => number = true,
                "--number-nonblank" => number_nonblank = true,
                "--show-ends" => show_ends = true,
                "--show-tabs" => show_tabs = true,
                "--squeeze-blank" => squeeze = true,
                "--show-nonprinting" => show_nonprint = true,
                "--show-all" => {
                    show_nonprint = true;
                    show_ends = true;
                    show_tabs = true;
                }
                other => {
                    ui.err(&format!("unrecognized option '{other}'"));
                    return 1;
                }
            },
            s if s.starts_with('-') && s != "-" => {
                for c in s.chars().skip(1) {
                    match c {
                        'A' => {
                            show_nonprint = true;
                            show_ends = true;
                            show_tabs = true;
                        }
                        'b' => number_nonblank = true,
                        'e' => {
                            show_nonprint = true;
                            show_ends = true;
                        }
                        'E' => show_ends = true,
                        'n' => number = true,
                        's' => squeeze = true,
                        't' => {
                            show_nonprint = true;
                            show_tabs = true;
                        }
                        'T' => show_tabs = true,
                        'v' => show_nonprint = true,
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

    if number_nonblank {
        number = false; // -b overrides -n
    }
    if files.is_empty() {
        files.push("-".into());
    }

    let fancy = number || number_nonblank || show_ends || show_tabs || squeeze || show_nonprint;
    let mut status = 0;
    let mut out = io::stdout().lock();
    let mut line_no: u64 = 1;
    let mut prev_blank = false;

    for f in &files {
        let res = if fancy {
            cat_fancy(
                f,
                &mut out,
                number,
                number_nonblank,
                show_ends,
                show_tabs,
                squeeze,
                show_nonprint,
                &mut line_no,
                &mut prev_blank,
            )
        } else {
            cat_fast(f, &mut out)
        };
        if let Err(e) = res {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            ui.err(&format!("{f}: {e}"));
            status = 1;
        }
    }
    status
}

/// Copy `path` (or standard input, for `-`) to `out` verbatim, with no
/// line-oriented processing. This is the common case (no display flags)
/// and avoids the line-splitting overhead `cat_fancy` needs.
fn cat_fast(path: &str, out: &mut impl Write) -> io::Result<()> {
    let mut buf = [0u8; 64 * 1024];
    let mut reader: Box<dyn Read> = if path == "-" {
        Box::new(io::stdin().lock())
    } else {
        Box::new(File::open(Path::new(path))?)
    };
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
    }
    out.flush()
}

/// Copy `path` (or standard input, for `-`) to `out` line by line,
/// applying numbering/`$`-at-EOL/tab-visibility/blank-squeezing/
/// non-printing-escape options. `line_no` and `prev_blank` are threaded
/// through across multiple files so numbering and blank-squeezing are
/// continuous across `cat a b`.
#[allow(clippy::too_many_arguments)]
fn cat_fancy(
    path: &str,
    out: &mut impl Write,
    number: bool,
    number_nonblank: bool,
    show_ends: bool,
    show_tabs: bool,
    squeeze: bool,
    show_nonprint: bool,
    line_no: &mut u64,
    prev_blank: &mut bool,
) -> io::Result<()> {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };

    for line_res in reader.split(b'\n') {
        let line = line_res?;
        let blank = line.is_empty();
        if squeeze && blank && *prev_blank {
            continue;
        }
        *prev_blank = blank;

        // `if A {act} else if B {act}` with an identical action is just
        // `if A || B {act}` — clippy (rightly) flags the two-armed form
        // as duplicated blocks.
        if (number_nonblank && !blank) || number {
            write!(out, "{:6}\t", *line_no)?;
            *line_no += 1;
        }

        for &b in &line {
            write_byte(out, b, show_tabs, show_nonprint)?;
        }
        if show_ends {
            out.write_all(b"$")?;
        }
        out.write_all(b"\n")?;
    }
    out.flush()
}

/// Write a single input byte to `out`, applying `-t`/`-T` (`^I` for tabs)
/// and `-v` (`^X`/`M-X` caret-and-meta notation) display substitutions.
fn write_byte(out: &mut impl Write, b: u8, show_tabs: bool, show_nonprint: bool) -> io::Result<()> {
    if b == b'\t' && show_tabs {
        return out.write_all(b"^I");
    }
    if !show_nonprint {
        return out.write_all(&[b]);
    }
    if b == b'\t' || b == b'\n' {
        return out.write_all(&[b]);
    }
    if b < 0x20 {
        out.write_all(&[b'^', b + 0x40])
    } else if b == 0x7f {
        out.write_all(b"^?")
    } else if b >= 0x80 {
        // meta
        out.write_all(b"M-")?;
        write_byte(out, b & 0x7f, show_tabs, true)
    } else {
        out.write_all(&[b])
    }
}

fn print_help() {
    print!(
        "Usage: cat [OPTION]... [FILE]...\n\
 Concatenate FILE(s) to standard output.\n\n\
 With no FILE, or when FILE is -, read standard input.\n\n\
 -A, --show-all equivalent to -vET\n\
 -b, --number-nonblank number nonempty output lines\n\
 -e equivalent to -vE\n\
 -E, --show-ends display $ at end of each line\n\
 -n, --number number all output lines\n\
 -s, --squeeze-blank suppress repeated empty output lines\n\
 -t equivalent to -vT\n\
 -T, --show-tabs display TAB characters as ^I\n\
 -v, --show-nonprinting use ^ and M- notation, except for LFD and TAB\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_file(tag: &str, contents: &[u8]) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "user_cat_test_{}_{}_{}",
            std::process::id(),
            tag,
            n
        ));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn cat_fast_copies_file_verbatim() {
        let path = scratch_file("fast", b"hello\nworld\n");
        let mut out = Vec::new();
        cat_fast(path.to_str().unwrap(), &mut out).unwrap();
        assert_eq!(out, b"hello\nworld\n");
    }

    #[test]
    fn cat_fast_missing_file_errors() {
        let missing = format!("/nonexistent_user_cat_test_{}", std::process::id());
        let mut out = Vec::new();
        assert!(cat_fast(&missing, &mut out).is_err());
    }

    #[test]
    fn cat_fast_empty_file_writes_nothing() {
        let path = scratch_file("empty", b"");
        let mut out = Vec::new();
        cat_fast(path.to_str().unwrap(), &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn cat_fancy_number_lines() {
        let path = scratch_file("num", b"a\nb\n");
        let mut out = Vec::new();
        let mut line_no = 1u64;
        let mut prev_blank = false;
        cat_fancy(
            path.to_str().unwrap(),
            &mut out,
            true,
            false,
            false,
            false,
            false,
            false,
            &mut line_no,
            &mut prev_blank,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "     1\ta\n     2\tb\n");
    }

    #[test]
    fn cat_fancy_number_nonblank_skips_blank_lines() {
        let path = scratch_file("num_nb", b"a\n\nb\n");
        let mut out = Vec::new();
        let mut line_no = 1u64;
        let mut prev_blank = false;
        cat_fancy(
            path.to_str().unwrap(),
            &mut out,
            false,
            true,
            false,
            false,
            false,
            false,
            &mut line_no,
            &mut prev_blank,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "     1\ta\n\n     2\tb\n");
    }

    #[test]
    fn cat_fancy_show_ends() {
        let path = scratch_file("ends", b"a\nb\n");
        let mut out = Vec::new();
        let mut line_no = 1u64;
        let mut prev_blank = false;
        cat_fancy(
            path.to_str().unwrap(),
            &mut out,
            false,
            false,
            true,
            false,
            false,
            false,
            &mut line_no,
            &mut prev_blank,
        )
        .unwrap();
        assert_eq!(out, b"a$\nb$\n");
    }

    #[test]
    fn cat_fancy_squeeze_blank_collapses_runs() {
        let path = scratch_file("squeeze", b"a\n\n\n\nb\n");
        let mut out = Vec::new();
        let mut line_no = 1u64;
        let mut prev_blank = false;
        cat_fancy(
            path.to_str().unwrap(),
            &mut out,
            false,
            false,
            false,
            false,
            true,
            false,
            &mut line_no,
            &mut prev_blank,
        )
        .unwrap();
        assert_eq!(out, b"a\n\nb\n");
    }

    #[test]
    fn write_byte_show_tabs() {
        let mut out = Vec::new();
        write_byte(&mut out, b'\t', true, false).unwrap();
        assert_eq!(out, b"^I");
    }

    #[test]
    fn write_byte_show_nonprinting_control_char() {
        let mut out = Vec::new();
        write_byte(&mut out, 0x01, false, true).unwrap();
        assert_eq!(out, b"^A");
    }

    #[test]
    fn write_byte_show_nonprinting_del() {
        let mut out = Vec::new();
        write_byte(&mut out, 0x7f, false, true).unwrap();
        assert_eq!(out, b"^?");
    }

    #[test]
    fn write_byte_show_nonprinting_meta() {
        let mut out = Vec::new();
        write_byte(&mut out, 0x81, false, true).unwrap();
        assert_eq!(out, b"M-^A");
    }

    #[test]
    fn write_byte_plain_passthrough() {
        let mut out = Vec::new();
        write_byte(&mut out, b'x', false, false).unwrap();
        assert_eq!(out, b"x");
    }
}
