//! user hostname — show or set the system host name.
use std::ffi::CString;
use std::io::{self, Write};

use usercore::Ui;

/// Entry point for the `hostname` utility. Parses `std::env::args()` and
/// either prints the current host name (optionally truncated to its short
/// form with `-s`) or, if a NAME operand is given, sets it via
/// `sethostname(2)`.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("hostname");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut set_name: Option<String> = None;
    let mut short = false;
    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("Usage: hostname [NAME]\n hostname [-s|--short]\nShow or set the system hostname.\n");
                return 0;
            }
            "--version" => {
                println!("hostname (user_utils) 0.1.0");
                return 0;
            }
            "-s" | "--short" => short = true,
            "-f" | "--fqdn" | "--long" => {}
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            s => {
                if set_name.is_some() {
                    ui.err(&format!("extra operand '{s}'"));
                    return 1;
                }
                set_name = Some(s.to_string());
            }
        }
    }
    if let Some(name) = set_name {
        let Ok(c) = CString::new(name.as_str()) else {
            ui.err("invalid hostname");
            return 1;
        };
        // SAFETY: `c` is a valid, NUL-terminated `CString` kept alive for
        // the duration of this call, so `c.as_ptr()` is a sound
        // `sethostname(2)` argument. `name.len()` is the byte length of
        // `name` *without* the NUL terminator, which is exactly what
        // `sethostname` expects (and matches `c`'s content length, since
        // `CString::new` succeeding above guarantees `name` has no
        // interior NUL bytes).
        let rc = unsafe { libc::sethostname(c.as_ptr(), name.len()) };
        if rc != 0 {
            ui.err(&format!("{}", io::Error::last_os_error()));
            return 1;
        }
        return 0;
    }
    let mut buf = vec![0u8; 256];
    // SAFETY: `buf` is a heap-allocated `Vec<u8>` with exactly 256 bytes
    // of storage, and `buf.len()` is passed as the size bound, so
    // `gethostname` cannot write past the buffer. Per POSIX, if the
    // hostname (plus NUL) doesn't fit, the result may be silently
    // truncated without a trailing NUL; that's handled below by treating
    // "no NUL found" as "use the whole buffer" (`unwrap_or(buf.len())`),
    // which stays within `buf`'s bounds either way.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) };
    if rc != 0 {
        ui.err(&format!("{}", io::Error::last_os_error()));
        return 1;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let host = String::from_utf8_lossy(&buf[..end]).into_owned();
    let host = short_form(&host, short);
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{host}");
    0
}

/// Truncate `host` at its first `.` when `short` is set (implements
/// `-s`/`--short`); otherwise returns `host` unchanged.
fn short_form(host: &str, short: bool) -> String {
    if short {
        host.split('.').next().unwrap_or(host).to_string()
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_form_truncates_at_first_dot() {
        assert_eq!(short_form("host.example.com", true), "host");
    }

    #[test]
    fn short_form_leaves_bare_hostname_unchanged() {
        assert_eq!(short_form("host", true), "host");
    }

    #[test]
    fn short_form_disabled_returns_full_name() {
        assert_eq!(short_form("host.example.com", false), "host.example.com");
    }

    #[test]
    fn short_form_empty_input() {
        assert_eq!(short_form("", true), "");
        assert_eq!(short_form("", false), "");
    }
}
