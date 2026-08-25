//! user swapoff — disable a swap area.
use std::ffi::CString;
use std::fs;
use std::io;

use usercore::Ui;

fn to_cstring(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"))
}

fn do_swapoff(path: &str) -> io::Result<()> {
    let c_path = to_cstring(path)?;
    // SAFETY: `c_path` is a valid, NUL-terminated `CString` kept alive
    // for the call; `swapoff(2)` takes no other argument.
    let r = unsafe { libc::swapoff(c_path.as_ptr()) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Every currently-active swap device/file, from `/proc/swaps` (first
/// column, first data line skipped since it's the header).
fn active_swaps(text: &str) -> Vec<String> {
    text.lines()
        .skip(1)
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

fn print_help() {
    print!(
        "Usage: swapoff DEVICE...\n\
 swapoff -a\n\
 -a, --all disable every active swap area (from /proc/swaps)\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `swapoff` utility. Parses `std::env::args()` and
/// either disables each named `DEVICE`, or (`-a`) every active swap area
/// listed in `/proc/swaps`.
///
/// Returns 0 on success, 1 if any requested swap area couldn't be disabled.
pub fn run() -> i32 {
    let ui = Ui::new("swapoff");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut all = false;
    let mut positional: Vec<String> = Vec::new();
    for arg in &args {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("swapoff (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => all = true,
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => positional.push(other.to_string()),
        }
    }

    let targets = if all {
        match fs::read_to_string("/proc/swaps") {
            Ok(t) => active_swaps(&t),
            Err(e) => {
                ui.err(&format!("/proc/swaps: {e}"));
                return 1;
            }
        }
    } else if positional.is_empty() {
        print_help();
        return 1;
    } else {
        positional
    };

    let mut status = 0;
    for dev in &targets {
        if let Err(e) = do_swapoff(dev) {
            ui.err(&format!("{dev}: {e}"));
            status = 1;
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_swaps_skips_header() {
        let text = "\
Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority
/dev/sda2                              partition\t2097148\t0\t-2
/swapfile                               file\t1048572\t0\t-3
";
        assert_eq!(
            active_swaps(text),
            vec!["/dev/sda2".to_string(), "/swapfile".to_string()]
        );
    }

    #[test]
    fn active_swaps_empty_when_none_active() {
        assert!(active_swaps("Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n").is_empty());
    }

    #[test]
    fn do_swapoff_unprivileged_or_bad_target_fails_cleanly() {
        assert!(do_swapoff("/nonexistent/user-swapoff-missing").is_err());
    }
}
