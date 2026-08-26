//! user fsck — filesystem check front-end.
//!
//! Detects each device's filesystem type (via `-t`, or else
//! `usercore::blkprobe`) and dispatches to the matching `fsck.<type>`
//! found on `PATH` — the same front-end/backend split real `fsck(8)`
//! uses, just without vendoring any of the actual per-filesystem
//! checkers (`e2fsck`, `xfs_repair`, `fsck.vfat`, …).
use std::path::PathBuf;
use std::process::Command;

use usercore::Ui;

/// Real `fsck(8)`'s documented exit-code bits — a caller can combine
/// several by bitwise OR, and this front-end does the same across
/// multiple devices. `EX_UNCORRECTED` is never constructed here (a
/// child `fsck.<type>` produces it on its own, and we just relay its
/// exit code) — kept as documentation of the contract, exercised by the
/// OR-combination test below.
#[allow(dead_code)]
const EX_UNCORRECTED: i32 = 4;
const EX_OPERATIONAL_ERROR: i32 = 8;
const EX_USAGE: i32 = 16;

/// Find `fsck.<fstype>` on Zainium's standard `PATH` directories (falls
/// back to the real `$PATH` the same way `usercore::zainium::path_dirs`
/// always does).
fn find_checker(fstype: &str) -> Option<PathBuf> {
    let name = format!("fsck.{fstype}");
    usercore::zainium::path_dirs()
        .into_iter()
        .map(|d| d.join(&name))
        .find(|p| p.is_file())
}

fn detect_fstype(device: &str) -> Option<String> {
    usercore::blkprobe::probe_path(std::path::Path::new(device))
        .ok()
        .flatten()
        .map(|p| p.fstype)
}

/// Run one device's check: resolve its filesystem type, find the
/// matching `fsck.<type>`, and run it with `passthrough` args followed
/// by `device`. Returns the checker's exit code, or an operational-error
/// code (8) if no type could be determined or no checker was found.
fn check_one(ui: &Ui, device: &str, forced_type: Option<&str>, passthrough: &[String]) -> i32 {
    let fstype = match forced_type {
        Some(t) => t.to_string(),
        None => match detect_fstype(device) {
            Some(t) => t,
            None => {
                ui.err(&format!("{device}: unable to determine filesystem type"));
                return EX_OPERATIONAL_ERROR;
            }
        },
    };

    let Some(checker) = find_checker(&fstype) else {
        ui.err(&format!("fsck.{fstype}: not found"));
        return EX_OPERATIONAL_ERROR;
    };

    match Command::new(checker).args(passthrough).arg(device).status() {
        Ok(status) => status.code().unwrap_or(EX_OPERATIONAL_ERROR),
        Err(e) => {
            ui.err(&format!("{device}: failed to run fsck.{fstype}: {e}"));
            EX_OPERATIONAL_ERROR
        }
    }
}

fn print_help() {
    print!(
        "Usage: fsck [-t TYPE] [OPTION...] DEVICE...\n\
 Check filesystems by dispatching to fsck.<type> on PATH.\n\
 -t, --type TYPE force the filesystem type (default: auto-detect)\n\
 Other OPTIONs are passed through unchanged to the fsck.<type> checker.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `fsck` utility. Parses `std::env::args()` for
/// `-t TYPE` and any number of `DEVICE` operands (other flags pass
/// through to the resolved `fsck.<type>` unchanged), checking each in
/// turn.
///
/// Returns the bitwise OR of every device's exit code (or a usage-error
/// code if no device was given), matching real `fsck(8)`'s convention.
pub fn run() -> i32 {
    let ui = Ui::new("fsck");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut forced_type: Option<String> = None;
    let mut devices: Vec<String> = Vec::new();
    let mut passthrough: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("fsck (user_utils) 0.1.0");
                return 0;
            }
            "-t" | "--type" => {
                i += 1;
                match args.get(i) {
                    Some(v) => forced_type = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 't'");
                        return EX_USAGE;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => passthrough.push(s.to_string()),
            other => devices.push(other.to_string()),
        }
        i += 1;
    }

    if devices.is_empty() {
        ui.err("no device specified");
        return EX_USAGE;
    }

    let mut status = 0;
    for device in &devices {
        status |= check_one(&ui, device, forced_type.as_deref(), &passthrough);
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_checker_none_for_a_type_with_no_installed_checker() {
        assert!(find_checker("user-fsck-test-nonexistent-type").is_none());
    }

    #[test]
    fn check_one_reports_operational_error_when_checker_missing() {
        let ui = Ui::new("fsck");
        let code = check_one(
            &ui,
            "/dev/null",
            Some("user-fsck-test-nonexistent-type"),
            &[],
        );
        assert_eq!(code, EX_OPERATIONAL_ERROR);
    }

    #[test]
    fn check_one_reports_operational_error_when_type_undetectable() {
        let dir = std::env::temp_dir().join(format!("user_fsck_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blank.img");
        std::fs::write(&path, vec![0u8; 4096]).unwrap();

        let ui = Ui::new("fsck");
        let code = check_one(&ui, path.to_str().unwrap(), None, &[]);
        assert_eq!(code, EX_OPERATIONAL_ERROR);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exit_code_bits_combine_as_expected() {
        assert_eq!(EX_UNCORRECTED | EX_OPERATIONAL_ERROR, 12);
    }
}
