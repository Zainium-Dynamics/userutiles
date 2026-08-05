//! user more — page through text one screenful at a time.
use std::fs::File;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::os::fd::AsRawFd;

use usercore::Ui;

/// Entry point for the `more` utility. Parses `std::env::args()` as a list
/// of `FILE`s (reading stdin if none are given) and pages each through
/// stdout a screenful at a time when stdout is a terminal.
///
/// Returns 0 on success, 1 if any file could not be opened or a non-broken-
/// pipe I/O error occurred while paging.
pub fn run() -> i32 {
    let ui = Ui::new("more");
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: more [FILE]...\nPage through text one screenful at a time.\nSPACE next page, Enter next line, q quit.\n");
                return 0;
            }
            "--version" => {
                println!("more (user_utils) 0.1.0");
                return 0;
            }
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
    let rows = term_rows().saturating_sub(1).max(1);
    let interactive = io::stdout().is_terminal();
    let mut status = 0;
    for f in &files {
        if files.len() > 1 {
            println!("::::::::::::::\n{f}\n::::::::::::::");
        }
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(io::BufReader::new(io::stdin()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(io::BufReader::new(fh)),
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    status = 1;
                    continue;
                }
            }
        };
        if let Err(e) = page(reader, rows, interactive, &mut io::stdout()) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                ui.err(&format!("{e}"));
                status = 1;
            }
            break;
        }
    }
    status
}

/// Terminal row count from `ioctl(TIOCGWINSZ)` on stdout, falling back to
/// `$LINES`, then 24.
fn term_rows() -> usize {
    // SAFETY: `libc::winsize` consists solely of four `u16` fields with no invalid
    // bit patterns, so the all-zero value from `mem::zeroed` is a valid `winsize`.
    // `libc::ioctl` is called on `STDOUT_FILENO` with the `TIOCGWINSZ` request,
    // which expects a `*mut winsize`; `&mut ws` is a valid pointer to that local,
    // live `winsize` for the duration of the call. If stdout is closed/not a tty
    // the call simply fails (returns -1 / sets errno) rather than causing UB, and
    // we only read `ws.ws_row` after checking the return value is 0.
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 {
            return ws.ws_row as usize;
        }
    }
    std::env::var("LINES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24)
}

/// Copy lines from `reader` to `out`, pausing every `rows` lines to prompt
/// for a `--More--` action when `interactive` is true (reading the action
/// from `/dev/tty` or stdin via [`read_cmd`]). When `interactive` is false
/// (e.g. stdout is a pipe), the entire input is copied through without
/// pausing.
fn page(
    reader: Box<dyn BufRead>,
    rows: usize,
    interactive: bool,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut count = 0usize;
    for line in reader.lines() {
        let line = line?;
        writeln!(out, "{line}")?;
        count += 1;
        if interactive && count >= rows {
            eprint!("--More--");
            let _ = io::stderr().flush();
            let action = read_cmd()?;
            eprint!("\r \r");
            let _ = io::stderr().flush();
            match action {
                Cmd::Quit => return Ok(()),
                Cmd::Line => count = rows - 1,
                Cmd::Page => count = 0,
            }
        }
    }
    Ok(())
}

enum Cmd {
    Quit,
    Line,
    Page,
}

/// Read one pager command byte from `/dev/tty` (falling back to stdin if
/// `/dev/tty` can't be opened), temporarily switching the terminal to raw
/// (non-canonical, non-echo) mode so a single keypress is enough.
fn read_cmd() -> io::Result<Cmd> {
    let tty = File::open("/dev/tty");
    let (mut file, fd) = match tty {
        Ok(f) => {
            let fd = f.as_raw_fd();
            (Some(f), fd)
        }
        Err(_) => (None, libc::STDIN_FILENO),
    };
    // SAFETY: `libc::termios` is a plain-old-data struct of integer fields and a
    // fixed-size `c_cc` array, all of which admit an all-zero bit pattern, so
    // `mem::zeroed` produces a valid (if not yet meaningful) `termios`. This value
    // is only used as scratch space to be filled by `tcgetattr` below; if that call
    // fails, `raw_ok` is false and `old` is never read again.
    let mut old: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is either the raw fd of the `/dev/tty` `File` held live in
    // `file` for the remainder of this function, or `STDIN_FILENO`, both of which
    // are valid open file descriptors for `tcgetattr` to operate on. `&mut old` is
    // a valid pointer to the live local `termios` above for `tcgetattr` to fill
    // in; the return value is checked before `old` is trusted.
    let raw_ok = unsafe { libc::tcgetattr(fd, &mut old) == 0 };
    if raw_ok {
        let mut raw = old;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        // SAFETY: this block only runs when `raw_ok` is true, i.e. `tcgetattr`
        // already succeeded on `fd` above, so `fd` is confirmed valid for terminal
        // ioctls. `raw` is a fully-initialized `termios` (copied from `old`, which
        // `tcgetattr` populated) with only flag/`c_cc` fields modified, and `&raw`
        // is a valid pointer to it for the duration of the call.
        unsafe {
            libc::tcsetattr(fd, libc::TCSANOW, &raw);
        }
    }
    let mut buf = [0u8; 1];
    let n = if let Some(ref mut f) = file {
        f.read(&mut buf)?
    } else {
        io::stdin().read(&mut buf)?
    };
    if raw_ok {
        // SAFETY: guarded by `raw_ok`, so `fd` was already validated as a working
        // terminal fd by the earlier successful `tcgetattr` call. `old` holds the
        // original attributes `tcgetattr` populated before we mutated `raw`, and
        // `&old` is a valid pointer to it for the duration of this restore call.
        unsafe {
            libc::tcsetattr(fd, libc::TCSANOW, &old);
        }
    }
    if n == 0 {
        return Ok(Cmd::Quit);
    }
    Ok(match buf[0] {
        b'q' | b'Q' => Cmd::Quit,
        b'\n' | b'\r' => Cmd::Line,
        _ => Cmd::Page,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_non_interactive_copies_all_lines() {
        let input = b"one\ntwo\nthree\n".as_slice();
        let mut out = Vec::new();
        page(Box::new(input), 1, false, &mut out).unwrap();
        assert_eq!(out, b"one\ntwo\nthree\n");
    }

    #[test]
    fn page_empty_input_writes_nothing() {
        let input: &[u8] = b"";
        let mut out = Vec::new();
        page(Box::new(input), 10, false, &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn page_non_interactive_ignores_row_limit() {
        // With interactive=false, more than `rows` lines must still all be
        // written without pausing for a --More-- prompt (which would block
        // on stdin in a test).
        let input = b"1\n2\n3\n4\n5\n".as_slice();
        let mut out = Vec::new();
        page(Box::new(input), 2, false, &mut out).unwrap();
        assert_eq!(out, b"1\n2\n3\n4\n5\n");
    }

    #[test]
    fn term_rows_has_sane_fallback() {
        // We can't control the test harness's tty, but the result must
        // always be a usable positive row count.
        assert!(term_rows() > 0);
    }
}
