//! user sort — sort lines of text files.
use std::cmp::Ordering;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

#[derive(Clone, Copy, PartialEq)]
enum KeyType {
    Default,
    Numeric,
    Human,
    Month,
    Version,
}

/// Entry point for the `sort` utility. Parses `std::env::args()`, reads
/// all input files (stdin if none given) into memory, and either sorts
/// them to stdout or (`-c`) checks the input is already sorted.
///
/// Returns 0 on success; 1 on a usage error, an I/O error, or (in `-c`
/// mode) if the input is out of order.
pub fn run() -> i32 {
    let ui = Ui::new("sort");
    let mut reverse = false;
    let mut unique = false;
    let mut ignore_case = false;
    let mut key_type = KeyType::Default;
    let mut check = false;
    let mut files: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("sort (user_utils) 0.1.0");
                return 0;
            }
            "-r" | "--reverse" => reverse = true,
            "-u" | "--unique" => unique = true,
            "-f" | "--ignore-case" => ignore_case = true,
            "-n" | "--numeric-sort" => key_type = KeyType::Numeric,
            "-h" | "--human-numeric-sort" => key_type = KeyType::Human,
            "-M" | "--month-sort" => key_type = KeyType::Month,
            "-V" | "--version-sort" => key_type = KeyType::Version,
            "-c" | "--check" | "-C" => check = true,
            s if s.starts_with('-') && s != "-" && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'r' => reverse = true,
                        'u' => unique = true,
                        'f' => ignore_case = true,
                        'n' => key_type = KeyType::Numeric,
                        'h' => key_type = KeyType::Human,
                        'M' => key_type = KeyType::Month,
                        'V' => key_type = KeyType::Version,
                        'c' | 'C' => check = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }

    let mut lines: Vec<String> = Vec::new();
    for f in &files {
        if let Err(e) = read_lines(f, &mut lines) {
            ui.err(&format!("{f}: {e}"));
            return 1;
        }
    }

    if check {
        for w in lines.windows(2) {
            let ord = cmp_lines(&w[0], &w[1], key_type, ignore_case);
            let ordered = if reverse {
                ord != Ordering::Less
            } else {
                ord != Ordering::Greater
            };
            if !ordered {
                ui.err("disorder detected");
                return 1;
            }
        }
        return 0;
    }

    lines.sort_by(|a, b| {
        let mut ord = cmp_lines(a, b, key_type, ignore_case);
        if reverse {
            ord = ord.reverse();
        }
        ord
    });

    if unique {
        lines.dedup_by(|a, b| cmp_lines(a, b, key_type, ignore_case) == Ordering::Equal);
    }

    let mut out = io::stdout().lock();
    for line in &lines {
        if let Err(e) = writeln!(out, "{line}") {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            ui.err(&format!("{e}"));
            return 1;
        }
    }
    0
}

fn print_help() {
    print!(
        "Usage: sort [OPTION]... [FILE]...\n\
 Write sorted concatenation of all FILE(s) to standard output.\n\n\
 -b, --ignore-leading-blanks ignore leading blanks\n\
 -c, --check check for sorted input; do not sort\n\
 -f, --ignore-case fold lower case to upper case characters\n\
 -h, --human-numeric-sort compare human readable numbers (e.g., 2K 1G)\n\
 -M, --month-sort compare (unknown) < 'JAN' < ... < 'DEC'\n\
 -n, --numeric-sort compare according to string numerical value\n\
 -r, --reverse reverse the result of comparisons\n\
 -u, --unique output only the first of an equal run\n\
 -V, --version-sort natural sort of (version) numbers within text\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Read every line of `path` (`-` for stdin) into `out`, appending in
/// order. Propagates the first I/O error encountered.
fn read_lines(path: &str, out: &mut Vec<String>) -> io::Result<()> {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    for line in reader.lines() {
        out.push(line?);
    }
    Ok(())
}

/// Compare two lines under the given [`KeyType`], falling back to a
/// plain string comparison to break ties (so equal keys still produce a
/// stable, deterministic order matching the original text).
fn cmp_lines(a: &str, b: &str, kt: KeyType, ignore_case: bool) -> Ordering {
    match kt {
        KeyType::Numeric => {
            let na = parse_num(a);
            let nb = parse_num(b);
            na.partial_cmp(&nb)
                .unwrap_or(Ordering::Equal)
                .then_with(|| str_cmp(a, b, ignore_case))
        }
        KeyType::Human => {
            let na = parse_human(a);
            let nb = parse_human(b);
            na.partial_cmp(&nb)
                .unwrap_or(Ordering::Equal)
                .then_with(|| str_cmp(a, b, ignore_case))
        }
        KeyType::Month => month_key(a)
            .cmp(&month_key(b))
            .then_with(|| str_cmp(a, b, ignore_case)),
        KeyType::Version => ver_cmp(a, b),
        KeyType::Default => str_cmp(a, b, ignore_case),
    }
}

fn str_cmp(a: &str, b: &str, ignore_case: bool) -> Ordering {
    if ignore_case {
        a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())
    } else {
        a.cmp(b)
    }
}

/// Parse the leading numeric run of `s` (optional sign, digits, at most
/// one `.`) as `-n`/`--numeric-sort`'s sort key. Non-numeric or empty
/// input sorts as `0.0`, matching GNU `sort`'s lenient treatment of
/// non-numeric lines in numeric mode.
fn parse_num(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    if !bytes.is_empty() && (bytes[0] == b'-' || bytes[0] == b'+') {
        end = 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    s[..end].parse().unwrap_or(0.0)
}

/// Like [`parse_num`], but also recognizes a trailing `K`/`M`/`G`/`T`/`P`
/// (powers of 1024) suffix for `-h`/`--human-numeric-sort`.
fn parse_human(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    if !bytes.is_empty() && (bytes[0] == b'-' || bytes[0] == b'+') {
        end = 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    let num: f64 = s[..end].parse().unwrap_or(0.0);
    let mult = match bytes.get(end).map(|c| c.to_ascii_uppercase()) {
        Some(b'K') => 1024.0,
        Some(b'M') => 1024.0f64.powi(2),
        Some(b'G') => 1024.0f64.powi(3),
        Some(b'T') => 1024.0f64.powi(4),
        Some(b'P') => 1024.0f64.powi(5),
        _ => 1.0,
    };
    num * mult
}

fn month_key(s: &str) -> u8 {
    let s = s.trim_start();
    let m = s.get(..3).unwrap_or("").to_ascii_uppercase();
    match m.as_str() {
        "JAN" => 1,
        "FEB" => 2,
        "MAR" => 3,
        "APR" => 4,
        "MAY" => 5,
        "JUN" => 6,
        "JUL" => 7,
        "AUG" => 8,
        "SEP" => 9,
        "OCT" => 10,
        "NOV" => 11,
        "DEC" => 12,
        _ => 0,
    }
}

/// `-V`/`--version-sort` comparison: runs of ASCII digits compare
/// numerically (as `u64`, saturating rather than overflowing on
/// pathologically long digit runs), everything else compares
/// char-by-char.
fn ver_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let mut an = 0u64;
                while let Some(c) = ai.peek().copied() {
                    if c.is_ascii_digit() {
                        an = an.saturating_mul(10).saturating_add((c as u8 - b'0') as u64);
                        ai.next();
                    } else {
                        break;
                    }
                }
                let mut bn = 0u64;
                while let Some(c) = bi.peek().copied() {
                    if c.is_ascii_digit() {
                        bn = bn.saturating_mul(10).saturating_add((c as u8 - b'0') as u64);
                        bi.next();
                    } else {
                        break;
                    }
                }
                match an.cmp(&bn) {
                    Ordering::Equal => {}
                    o => return o,
                }
            }
            (Some(ac), Some(bc)) => match ac.cmp(&bc) {
                Ordering::Equal => {
                    ai.next();
                    bi.next();
                }
                o => return o,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn str_cmp_case_sensitive() {
        assert_eq!(str_cmp("Apple", "apple", false), "Apple".cmp("apple"));
    }

    #[test]
    fn str_cmp_case_insensitive() {
        assert_eq!(str_cmp("Apple", "apple", true), Ordering::Equal);
    }

    #[test]
    fn parse_num_basic() {
        assert_eq!(parse_num("42"), 42.0);
        assert_eq!(parse_num("-3.5"), -3.5);
    }

    #[test]
    fn parse_num_non_numeric_defaults_to_zero() {
        assert_eq!(parse_num("abc"), 0.0);
    }

    #[test]
    fn parse_num_leading_whitespace_and_trailing_text() {
        assert_eq!(parse_num("  10abc"), 10.0);
    }

    #[test]
    fn parse_human_suffixes() {
        assert_eq!(parse_human("2K"), 2048.0);
        assert_eq!(parse_human("1M"), 1024.0 * 1024.0);
        assert_eq!(parse_human("3"), 3.0);
    }

    #[test]
    fn month_key_known_and_unknown() {
        assert_eq!(month_key("Jan 1"), 1);
        assert_eq!(month_key("Dec 31"), 12);
        assert_eq!(month_key("nope"), 0);
    }

    #[test]
    fn ver_cmp_numeric_runs() {
        assert_eq!(ver_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(ver_cmp("file10", "file2"), Ordering::Greater);
        assert_eq!(ver_cmp("file1", "file1"), Ordering::Equal);
    }

    #[test]
    fn ver_cmp_does_not_panic_on_huge_digit_run() {
        // 40 digits overflows u64; saturating arithmetic must not panic.
        let a = "9999999999999999999999999999999999999999";
        let b = "1";
        // Should not panic regardless of the resulting ordering.
        let _ = ver_cmp(a, b);
    }

    #[test]
    fn cmp_lines_numeric_ties_break_on_text() {
        // "1" and "01" both parse to 1.0, so the string comparison
        // decides the tie.
        let ord = cmp_lines("01", "1", KeyType::Numeric, false);
        assert_eq!(ord, "01".cmp("1"));
    }

    #[test]
    fn read_lines_missing_file_errors() {
        let mut out = Vec::new();
        assert!(read_lines("/nonexistent/user-sort-missing", &mut out).is_err());
    }

    #[test]
    fn read_lines_reads_all_lines_from_a_real_file() {
        let dir = std::env::temp_dir().join(format!("user_sort_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("in.txt");
        std::fs::write(&path, "b\na\nc\n").unwrap();
        let mut out = Vec::new();
        read_lines(path.to_str().unwrap(), &mut out).unwrap();
        assert_eq!(out, vec!["b", "a", "c"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
