//! user swapon — enable a swap area.
use std::ffi::CString;
use std::fs;
use std::io;

use usercore::Ui;

// linux/swap.h — stable kernel ABI, not exposed by the `libc` crate.
const SWAP_FLAG_PREFER: libc::c_int = 0x8000;
const SWAP_FLAG_DISCARD: libc::c_int = 0x10000;
const SWAP_FLAG_PRIO_MASK: libc::c_int = 0x7fff;

fn to_cstring(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"))
}

/// Enable `path` as a swap area, with an optional `priority` (0..=32767)
/// and `discard`.
fn do_swapon(path: &str, priority: Option<i32>, discard: bool) -> io::Result<()> {
    let c_path = to_cstring(path)?;
    let mut flags = 0;
    if let Some(p) = priority {
        flags |= SWAP_FLAG_PREFER | (p & SWAP_FLAG_PRIO_MASK);
    }
    if discard {
        flags |= SWAP_FLAG_DISCARD;
    }
    // SAFETY: `c_path` is a valid, NUL-terminated `CString` kept alive
    // for the call; `swapon(2)` takes no other pointer argument.
    let r = unsafe { libc::swapon(c_path.as_ptr(), flags) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Every non-`noauto` `swap`-type entry in `/etc/fstab`.
fn fstab_swap_entries(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let device = f.next()?;
            let _mountpoint = f.next()?;
            let fstype = f.next()?;
            let options = f.next().unwrap_or("defaults");
            if fstype == "swap" && !options.split(',').any(|o| o == "noauto") {
                Some(device.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn print_summary() -> io::Result<()> {
    let text = fs::read_to_string("/proc/swaps")?;
    usercore::ui::write_stdout(text.as_bytes())?;
    usercore::ui::flush_stdout()
}

fn print_help() {
    print!(
        "Usage: swapon [-p PRIORITY] [-d] DEVICE\n\
 swapon -a\n\
 swapon -s\n\
 -a, --all enable every non-noauto swap entry in /etc/fstab\n\
 -p, --priority PRI set the swap priority\n\
 -d, --discard enable discard for freed swap pages\n\
 -s, --summary show swap usage summary (same as /proc/swaps)\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `swapon` utility. Parses `std::env::args()` and
/// either enables one `DEVICE` (optionally with `-p`/`-d`), every
/// `/etc/fstab` swap entry (`-a`), or prints `/proc/swaps` (`-s`).
///
/// Returns 0 on success, 1 if any requested swap area couldn't be enabled.
pub fn run() -> i32 {
    let ui = Ui::new("swapon");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut all = false;
    let mut summary = false;
    let mut priority: Option<i32> = None;
    let mut discard = false;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("swapon (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => all = true,
            "-s" | "--summary" => summary = true,
            "-d" | "--discard" => discard = true,
            "-p" | "--priority" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse().ok()) {
                    Some(v) => priority = Some(v),
                    None => {
                        ui.err("invalid or missing priority");
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if summary {
        return match print_summary() {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&format!("{e}"));
                1
            }
        };
    }

    if all {
        let entries = match fs::read_to_string("/etc/fstab") {
            Ok(t) => fstab_swap_entries(&t),
            Err(e) => {
                ui.err(&format!("/etc/fstab: {e}"));
                return 1;
            }
        };
        let mut status = 0;
        for dev in &entries {
            if let Err(e) = do_swapon(dev, priority, discard) {
                ui.err(&format!("{dev}: {e}"));
                status = 1;
            }
        }
        return status;
    }

    match positional.first() {
        Some(dev) => match do_swapon(dev, priority, discard) {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&format!("{dev}: {e}"));
                1
            }
        },
        None => {
            print_help();
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fstab_swap_entries_filters_by_type_and_noauto() {
        let text = "\
/dev/sda1 / ext4 defaults 0 1
/dev/sda2 none swap sw 0 0
/dev/sda3 none swap noauto 0 0
";
        assert_eq!(fstab_swap_entries(text), vec!["/dev/sda2".to_string()]);
    }

    #[test]
    fn do_swapon_unprivileged_or_bad_target_fails_cleanly() {
        assert!(do_swapon("/nonexistent/user-swapon-missing", None, false).is_err());
    }
}
