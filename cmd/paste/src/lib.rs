//! user paste — merge lines of files.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `paste` utility. Parses `std::env::args()` as
/// `[OPTION]... [FILE]...` (reading stdin if none are given) and writes
/// lines consisting of the sequentially corresponding lines from each FILE,
/// separated by TAB (or `-d`/`--delimiters`), joined either in parallel
/// (default, one output line per input line-number across all files) or
/// serially (`-s`, one output line per input file).
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("paste");
    let mut serial = false;
    let mut delim = vec![b'\t'];
    let mut files: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: paste [OPTION]... [FILE]...\n\
 Write lines consisting of the sequentially corresponding lines from each FILE.\n\n\
 -d, --delimiters=LIST reuse characters from LIST instead of TABs\n\
 -s, --serial paste one file at a time instead of in parallel\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("paste (user_utils) 0.1.0");
                return 0;
            }
            "-s" | "--serial" => serial = true,
            "-d" | "--delimiters" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'd'");
                    return 1;
                }
                delim = expand_delim(&args[i]);
            }
            s if s.starts_with("-d") && s.len() > 2 => {
                delim = expand_delim(&s[2..]);
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
        files.push("-".into());
    }
    if delim.is_empty() {
        delim.push(b'\t');
    }

    let mut out = io::stdout().lock();
    let result = if serial {
        files
            .iter()
            .try_for_each(|f| paste_serial(f, &delim, &mut out).map_err(|e| (f.clone(), e)))
    } else {
        paste_parallel(&files, &delim, &mut out).map_err(|e| (String::new(), e))
    };
    if let Err((f, e)) = result {
        if e.kind() == io::ErrorKind::BrokenPipe {
            return 0;
        }
        if f.is_empty() {
            ui.err(&format!("{e}"));
        } else {
            ui.err(&format!("{f}: {e}"));
        }
        return 1;
    }
    0
}

/// Expand backslash escapes (`\n`, `\t`, `\0`, `\\`) in a `-d`/`--delimiters`
/// argument into the raw delimiter bytes that get cycled through between
/// output columns.
fn expand_delim(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            out.push(match b[i + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'0' => 0,
                b'\\' => b'\\',
                other => other,
            });
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// Open `path` for buffered line reading, or stdin if `path == "-"`.
fn open_reader(path: &str) -> io::Result<Box<dyn BufRead>> {
    if path == "-" {
        Ok(Box::new(BufReader::new(io::stdin())))
    } else {
        Ok(Box::new(BufReader::new(File::open(path)?)))
    }
}

/// Write all of `path`'s lines joined onto a single output line, separated
/// by (cycled) `delim` bytes, matching `paste -s` for one file.
fn paste_serial(path: &str, delim: &[u8], out: &mut impl Write) -> io::Result<()> {
    let reader = open_reader(path)?;
    let mut first = true;
    let mut di = 0;
    for line in reader.lines() {
        let line = line?;
        if !first {
            out.write_all(&[delim[di % delim.len()]])?;
            di += 1;
        }
        out.write_all(line.as_bytes())?;
        first = false;
    }
    out.write_all(b"\n")?;
    out.flush()
}

/// Read one line at a time from each of `files` in lockstep, joining the
/// Nth lines from every file (in order) onto output line N, separated by
/// (cycled) `delim` bytes. Files that run out of lines contribute an empty
/// field for the remaining rows; iteration stops once every file is
/// exhausted.
fn paste_parallel(files: &[String], delim: &[u8], out: &mut impl Write) -> io::Result<()> {
    let mut readers: Vec<Option<Box<dyn BufRead>>> = Vec::new();
    for f in files {
        readers.push(Some(open_reader(f)?));
    }
    loop {
        let mut any = false;
        let mut row = Vec::new();
        for (idx, slot) in readers.iter_mut().enumerate() {
            if idx > 0 {
                row.push(delim[(idx - 1) % delim.len()]);
            }
            if let Some(r) = slot.as_mut() {
                let mut line = String::new();
                match r.read_line(&mut line) {
                    Ok(0) => {
                        *slot = None;
                    }
                    Ok(_) => {
                        any = true;
                        if line.ends_with('\n') {
                            line.pop();
                            if line.ends_with('\r') {
                                line.pop();
                            }
                        }
                        row.extend_from_slice(line.as_bytes());
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        if !any {
            break;
        }
        row.push(b'\n');
        out.write_all(&row)?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_file(tag: &str, contents: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("user_paste_test_{tag}_{}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn expand_delim_handles_escapes() {
        assert_eq!(expand_delim("\\n"), vec![b'\n']);
        assert_eq!(expand_delim("\\t"), vec![b'\t']);
        assert_eq!(expand_delim("\\0"), vec![0]);
        assert_eq!(expand_delim("\\\\"), vec![b'\\']);
        assert_eq!(expand_delim(",;"), vec![b',', b';']);
    }

    #[test]
    fn expand_delim_trailing_backslash_is_literal() {
        assert_eq!(expand_delim("a\\"), vec![b'a', b'\\']);
    }

    #[test]
    fn paste_serial_joins_lines_with_delimiter() {
        let f = scratch_file("serial", "a\nb\nc\n");
        let mut out = Vec::new();
        paste_serial(f.to_str().unwrap(), b",", &mut out).unwrap();
        assert_eq!(out, b"a,b,c\n");
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn paste_serial_empty_file_prints_blank_line() {
        let f = scratch_file("serial_empty", "");
        let mut out = Vec::new();
        paste_serial(f.to_str().unwrap(), b",", &mut out).unwrap();
        assert_eq!(out, b"\n");
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn paste_serial_missing_file_errors() {
        let missing = std::env::temp_dir().join(format!(
            "user_paste_test_missing_{}_nope",
            std::process::id()
        ));
        let mut out = Vec::new();
        let err = paste_serial(missing.to_str().unwrap(), b",", &mut out).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn paste_parallel_merges_equal_length_files() {
        let f1 = scratch_file("par1", "1\n2\n3\n");
        let f2 = scratch_file("par2", "a\nb\nc\n");
        let mut out = Vec::new();
        paste_parallel(
            &[
                f1.to_str().unwrap().to_string(),
                f2.to_str().unwrap().to_string(),
            ],
            b"\t",
            &mut out,
        )
        .unwrap();
        assert_eq!(out, b"1\ta\n2\tb\n3\tc\n");
        let _ = std::fs::remove_file(&f1);
        let _ = std::fs::remove_file(&f2);
    }

    #[test]
    fn paste_parallel_ragged_files_pad_with_empty_field() {
        let f1 = scratch_file("ragged1", "1\n2\n3\n");
        let f2 = scratch_file("ragged2", "a\n");
        let mut out = Vec::new();
        paste_parallel(
            &[
                f1.to_str().unwrap().to_string(),
                f2.to_str().unwrap().to_string(),
            ],
            b"\t",
            &mut out,
        )
        .unwrap();
        assert_eq!(out, b"1\ta\n2\t\n3\t\n");
        let _ = std::fs::remove_file(&f1);
        let _ = std::fs::remove_file(&f2);
    }

    #[test]
    fn paste_parallel_cycles_multi_char_delimiter() {
        let f1 = scratch_file("cyc1", "1\n1\n");
        let f2 = scratch_file("cyc2", "2\n2\n");
        let f3 = scratch_file("cyc3", "3\n3\n");
        let mut out = Vec::new();
        paste_parallel(
            &[
                f1.to_str().unwrap().to_string(),
                f2.to_str().unwrap().to_string(),
                f3.to_str().unwrap().to_string(),
            ],
            b",;",
            &mut out,
        )
        .unwrap();
        assert_eq!(out, b"1,2;3\n1,2;3\n");
        let _ = std::fs::remove_file(&f1);
        let _ = std::fs::remove_file(&f2);
        let _ = std::fs::remove_file(&f3);
    }
}
