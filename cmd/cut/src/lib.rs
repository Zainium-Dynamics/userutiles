//! user cut — remove sections from each line of files.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Output settings shared by all modes.
struct Extra {
    complement: bool,
    out_delim: Option<Vec<u8>>,
    term: u8,
}

#[derive(Clone)]
enum Mode {
    Bytes(Vec<(usize, Option<usize>)>),
    Chars(Vec<(usize, Option<usize>)>),
    Fields {
        ranges: Vec<(usize, Option<usize>)>,
        delim: u8,
        only_delimited: bool,
    },
}

/// Entry point for the `cut` utility. Parses `std::env::args()` and prints
/// the selected bytes, characters, or delimited fields of each line from
/// each FILE (or stdin if none given) to stdout.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("cut");
    let mut mode: Option<Mode> = None;
    let mut delim = b'\t';
    let mut only_delimited = false;
    let mut extra = Extra { complement: false, out_delim: None, term: b'\n' };
    let mut files: Vec<String> = Vec::new();

    // --opt=VALUE as two arguments
    let args: Vec<String> = std::env::args()
        .skip(1)
        .flat_map(|a| match a.split_once('=') {
            Some((k @ ("--bytes" | "--characters" | "--fields" | "--delimiter" | "--output-delimiter"), v)) => {
                vec![k.to_string(), v.to_string()]
            }
            _ => vec![a],
        })
        .collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: cut OPTION... [FILE]...\n\
 Print selected parts of lines from each FILE to standard output.\n\n\
 -b, --bytes=LIST select only these bytes\n\
 -c, --characters=LIST select only these characters\n\
 -d, --delimiter=DELIM use DELIM instead of TAB for field delimiter\n\
 -f, --fields=LIST select only these fields\n\
 -s, --only-delimited do not print lines not containing delimiters\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("cut (user_utils) 0.1.0");
                return 0;
            }
            "-s" | "--only-delimited" => only_delimited = true,
            "--complement" => extra.complement = true,
            "-z" | "--zero-terminated" => extra.term = 0,
            "-n" => {}
            "--output-delimiter" => {
                i += 1;
                extra.out_delim = args.get(i).map(|d| d.as_bytes().to_vec());
            }
            "-b" | "--bytes" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'b'");
                    return 1;
                }
                match parse_list(&args[i]) {
                    Ok(r) => mode = Some(Mode::Bytes(r)),
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            "-c" | "--characters" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'c'");
                    return 1;
                }
                match parse_list(&args[i]) {
                    Ok(r) => mode = Some(Mode::Chars(r)),
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            "-f" | "--fields" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'f'");
                    return 1;
                }
                match parse_list(&args[i]) {
                    Ok(r) => {
                        mode = Some(Mode::Fields {
                            ranges: r,
                            delim,
                            only_delimited,
                        })
                    }
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            "-d" | "--delimiter" => {
                i += 1;
                if i >= args.len() {
                    ui.err("option requires an argument -- 'd'");
                    return 1;
                }
                let d = args[i].as_bytes();
                if d.is_empty() {
                    ui.err("the delimiter must be a single character");
                    return 1;
                }
                delim = d[0];
                if let Some(Mode::Fields {
                    ranges,
                    only_delimited: od,
                    ..
                }) = mode.clone()
                {
                    mode = Some(Mode::Fields {
                        ranges,
                        delim,
                        only_delimited: od,
                    });
                }
            }
            s if s.starts_with("-b") && s.len() > 2 => match parse_list(&s[2..]) {
                Ok(r) => mode = Some(Mode::Bytes(r)),
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with("-c") && s.len() > 2 => match parse_list(&s[2..]) {
                Ok(r) => mode = Some(Mode::Chars(r)),
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with("-f") && s.len() > 2 => match parse_list(&s[2..]) {
                Ok(r) => {
                    mode = Some(Mode::Fields {
                        ranges: r,
                        delim,
                        only_delimited,
                    })
                }
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with("-d") && s.len() > 2 => {
                delim = s.as_bytes()[2];
            }
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }

    // refresh only_delimited into Fields mode
    if let Some(Mode::Fields {
        ranges, delim: d, ..
    }) = mode
    {
        mode = Some(Mode::Fields {
            ranges,
            delim: d,
            only_delimited,
        });
    }

    let mode = match mode {
        Some(m) => m,
        None => {
            ui.err("you must specify a list of bytes, characters, or fields");
            return 1;
        }
    };
    if files.is_empty() {
        files.push("-".into());
    }

    let mut status = 0;
    let mut out = io::stdout().lock();
    for f in &files {
        if let Err(e) = process_file(f, &mode, &extra, &mut out) {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            ui.err(&format!("{f}: {e}"));
            status = 1;
        }
    }
    status
}

/// Parse a comma-separated list of 1-based ranges such as `1,3-5,7-`
/// (GNU `cut` LIST syntax) into `(start, end)` pairs where `end == None`
/// means "to end of line". Returns an error message if the list is empty,
/// malformed, or contains a `0` (numbering starts at 1).
fn parse_list(s: &str) -> Result<Vec<(usize, Option<usize>)>, String> {
    let mut ranges = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let start = if a.is_empty() {
                1
            } else {
                a.parse::<usize>()
                    .map_err(|_| "invalid byte/field list".to_string())?
            };
            let end = if b.is_empty() {
                None
            } else {
                Some(
                    b.parse::<usize>()
                        .map_err(|_| "invalid byte/field list".to_string())?,
                )
            };
            if start == 0 || end == Some(0) {
                return Err("byte/field numbering starts at 1".into());
            }
            ranges.push((start, end));
        } else {
            let n: usize = part
                .parse()
                .map_err(|_| "invalid byte/field list".to_string())?;
            if n == 0 {
                return Err("byte/field numbering starts at 1".into());
            }
            ranges.push((n, Some(n)));
        }
    }
    if ranges.is_empty() {
        return Err("missing list of fields/bytes".into());
    }
    Ok(ranges)
}

/// Return true if the 1-based position `pos` falls within any of `ranges`.
fn in_ranges(pos: usize, ranges: &[(usize, Option<usize>)]) -> bool {
    ranges.iter().any(|&(s, e)| match e {
        Some(e) => pos >= s && pos <= e,
        None => pos >= s,
    })
}

/// Read `path` (or stdin if `path == "-"`) line by line and write the
/// selected bytes/characters/fields of each line to `out`, per `mode`.
fn process_file(path: &str, mode: &Mode, x: &Extra, out: &mut impl Write) -> io::Result<()> {
    let mut reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    let selected = |pos: usize, ranges: &[(usize, Option<usize>)]| in_ranges(pos, ranges) != x.complement;
    let mut raw = Vec::new();
    loop {
        raw.clear();
        if reader.read_until(x.term, &mut raw)? == 0 {
            break;
        }
        if raw.last() == Some(&x.term) {
            raw.pop();
        }
        let line = String::from_utf8_lossy(&raw).into_owned();
        match mode {
            Mode::Bytes(ranges) | Mode::Chars(ranges) => {
                // characters as UTF-8 for -c, raw bytes for -b; --output-delimiter goes
                // between selected runs that aren't adjacent
                let units: Vec<Vec<u8>> = if matches!(mode, Mode::Bytes(_)) {
                    raw.iter().map(|b| vec![*b]).collect()
                } else {
                    line.chars().map(|c| c.to_string().into_bytes()).collect()
                };
                let mut prev: Option<usize> = None;
                for (i, u) in units.iter().enumerate() {
                    if selected(i + 1, ranges) {
                        if let (Some(p), Some(d)) = (prev, &x.out_delim) {
                            if p + 1 != i {
                                out.write_all(d)?;
                            }
                        }
                        out.write_all(u)?;
                        prev = Some(i);
                    }
                }
                out.write_all(&[x.term])?;
            }
            Mode::Fields {
                ranges,
                delim,
                only_delimited,
            } => {
                if !raw.contains(delim) {
                    if !only_delimited {
                        out.write_all(&raw)?;
                        out.write_all(&[x.term])?;
                    }
                    continue;
                }
                let sep = x.out_delim.clone().unwrap_or_else(|| vec![*delim]);
                let mut first = true;
                for (i, field) in raw.split(|b| b == delim).enumerate() {
                    if selected(i + 1, ranges) {
                        if !first {
                            out.write_all(&sep)?;
                        }
                        out.write_all(field)?;
                        first = false;
                    }
                }
                out.write_all(&[x.term])?;
            }
        }
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_list_single_and_range() {
        assert_eq!(parse_list("3").unwrap(), vec![(3, Some(3))]);
        assert_eq!(parse_list("2-4").unwrap(), vec![(2, Some(4))]);
        assert_eq!(parse_list("-3").unwrap(), vec![(1, Some(3))]);
        assert_eq!(parse_list("3-").unwrap(), vec![(3, None)]);
    }

    #[test]
    fn parse_list_rejects_zero_and_empty() {
        assert!(parse_list("0").is_err());
        assert!(parse_list("0-3").is_err());
        assert!(parse_list("").is_err());
    }

    #[test]
    fn in_ranges_bounds() {
        let r = vec![(2usize, Some(4usize)), (7, None)];
        assert!(!in_ranges(1, &r));
        assert!(in_ranges(2, &r));
        assert!(in_ranges(4, &r));
        assert!(!in_ranges(5, &r));
        assert!(in_ranges(10, &r));
    }

    #[test]
    fn process_file_selects_fields() {
        let dir = std::env::temp_dir().join(format!("user_cut_test_{}", std::process::id()));
        std::fs::write(&dir, "a:b:c\nd:e:f\n").unwrap();
        let mode = Mode::Fields {
            ranges: vec![(2, Some(2))],
            delim: b':',
            only_delimited: false,
        };
        let mut buf: Vec<u8> = Vec::new();
        process_file(dir.to_str().unwrap(), &mode, &Extra { complement: false, out_delim: None, term: b'\n' }, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "b\ne\n");
        std::fs::remove_file(&dir).ok();
    }

    #[test]
    fn process_file_missing_file_errors() {
        let missing = std::env::temp_dir().join(format!(
            "user_cut_test_missing_{}_does_not_exist",
            std::process::id()
        ));
        let mode = Mode::Bytes(vec![(1, Some(1))]);
        let mut buf: Vec<u8> = Vec::new();
        let result = process_file(missing.to_str().unwrap(), &mode, &Extra { complement: false, out_delim: None, term: b'\n' }, &mut buf);
        assert!(result.is_err());
    }
}
