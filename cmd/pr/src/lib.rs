//! user pr — paginate or columnate files for printing (subset).
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `pr` utility. Parses `std::env::args()` and paginates
/// each `FILE` (or stdin, if none are given) for printing, inserting a
/// header/page-break every `PAGE_LENGTH` lines (`-l`, default 66) and
/// optionally numbering lines (`-n`).
///
/// Returns 0 on success, 1 if any file could not be opened or read.
pub fn run() -> i32 {
    let ui = Ui::new("pr");
    let mut pagesize = 66usize;
    let mut show_header = true;
    let mut number = false;
    let mut width = 72usize;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: pr [OPTION]... [FILE]...\nPaginate FILE(s) for printing.\n -l PAGE_LENGTH page length (default 66)\n -n number lines\n -t omit header/trailer\n -w PAGE_WIDTH page width (default 72)\n");
                return 0;
            }
            "--version" => {
                println!("pr (user_utils) 0.1.0");
                return 0;
            }
            "-t" => show_header = false,
            "-n" => number = true,
            "-l" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'l'");
                    return 1;
                };
                let Ok(n) = arg.parse::<usize>() else {
                    ui.err(&format!("invalid page length '{arg}'"));
                    return 1;
                };
                pagesize = n.max(1);
            }
            "-w" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'w'");
                    return 1;
                };
                let Ok(n) = arg.parse::<usize>() else {
                    ui.err(&format!("invalid page width '{arg}'"));
                    return 1;
                };
                width = n.max(1);
            }
            s if s.starts_with("-l") && s.len() > 2 => {
                let Ok(n) = s[2..].parse::<usize>() else {
                    ui.err(&format!("invalid page length '{}'", &s[2..]));
                    return 1;
                };
                pagesize = n.max(1);
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
    let mut out = io::stdout().lock();
    let mut status = 0;
    let body = pagesize
        .saturating_sub(if show_header { 5 } else { 0 })
        .max(1);
    for f in files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            match File::open(&f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    status = 1;
                    continue;
                }
            }
        };
        let mut page = 1usize;
        let mut in_page = 0usize;
        let date = now_str();
        for (line_no, line) in (1usize..).zip(reader.lines()) {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    ui.err(&format!("{e}"));
                    status = 1;
                    break;
                }
            };
            if in_page == 0 && show_header {
                let title = if f == "-" { "" } else { f.as_str() };
                let _ = writeln!(out, "\n{date} {title} Page {page}\n");
                in_page = 2;
            }
            if number {
                let _ = writeln!(out, "{line_no:>6}\t{line}");
            } else {
                let _ = writeln!(out, "{line}");
            }
            in_page += 1;
            if in_page >= body {
                if show_header {
                    let _ = writeln!(out, "\n");
                }
                page += 1;
                in_page = 0;
            }
        }
    }
    // This simplified `pr` does not implement multi-column layout, so the
    // parsed page width is currently unused beyond validating `-w`'s argument.
    let _ = width;
    status
}

/// Format the current local time as `YYYY-MM-DD HH:MM`, for use in page
/// headers.
fn now_str() -> String {
    // SAFETY: `libc::tm` is a plain C struct of integers (no padding
    // that must hold an invariant, no pointers/references inside on
    // Linux) — the all-zero bit pattern is a valid value for it, so
    // `mem::zeroed` cannot produce UB here.
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    // SAFETY: `time(2)` with a null `tloc` argument just returns the
    // current time and performs no pointer dereferencing; it cannot fail.
    let t = unsafe { libc::time(std::ptr::null_mut()) };
    // SAFETY: `t` is a valid, initialized `time_t` local and `tm` is a
    // valid, initialized (zeroed) `libc::tm` local declared above, so
    // `&t` and `&mut tm` are both valid non-null pointers of the
    // expected types for `localtime_r(3)` to read from / write into;
    // no aliasing since `t` and `tm` are distinct locals.
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_str_has_expected_shape() {
        let s = now_str();
        // "YYYY-MM-DD HH:MM" is 16 chars.
        assert_eq!(s.len(), 16);
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
        assert_eq!(s.as_bytes()[10], b' ');
        assert_eq!(s.as_bytes()[13], b':');
    }
}
