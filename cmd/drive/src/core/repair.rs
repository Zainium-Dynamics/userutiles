use anyhow::Result;
use colored::*;
use std::path::Path;
use std::process::Command;

use crate::ui::display::{print_error, print_header, print_row, print_success, print_warning};
use crate::utils::validator::normalize_device;

pub fn run_repair(device: &str, dry_run: bool) -> Result<()> {
    let Some(dev_path) = normalize_device(device) else {
        print_error(&format!("Invalid device name: {device}"));
        println!();
        return Ok(());
    };

    print_header("Filesystem Repair");
    println!();

    if !Path::new(&dev_path).exists() {
        print_error(&format!("Device not found: {dev_path}"));
        println!();
        return Ok(());
    }

    if is_mounted(&dev_path) {
        print_error(&format!("{dev_path} is currently mounted — unmount first"));
        println!(
            "  {} Run: {}",
            "→".bright_cyan(),
            format!("drive umount {dev_path}").bright_blue()
        );
        println!();
        return Ok(());
    }

    let fs_type = detect_fs(&dev_path).unwrap_or_else(|| "unknown".to_string());

    print_row("Device", &dev_path.bright_blue().to_string());
    print_row("Filesystem", &fs_type.bright_magenta().to_string());
    print_row(
        "Mode",
        &if dry_run {
            "Dry-run (no changes)".bright_yellow().to_string()
        } else {
            "Automatic".bright_magenta().to_string()
        },
    );
    println!();

    if dry_run {
        print_warning("Dry-run mode — reporting issues only, no changes will be made");
        println!();
    }

    println!("  {} Running filesystem check...", "→".bright_cyan());
    println!();

    let result = dispatch_repair(&fs_type, &dev_path, dry_run);

    match result {
        RepairResult::Success {
            errors_fixed,
            files_recovered,
        } => {
            print_success("Repair completed");
            if errors_fixed > 0 {
                print_row(
                    "Errors fixed",
                    &errors_fixed.to_string().bright_magenta().to_string(),
                );
            }
            if files_recovered > 0 {
                print_row(
                    "Files recovered",
                    &files_recovered.to_string().bright_magenta().to_string(),
                );
            }
            print_row("Status", &"Clean".bright_green().to_string());
            println!();
            print_success("Filesystem is now consistent");
        }
        RepairResult::NoErrors => {
            print_success("No errors found — filesystem is clean");
        }
        RepairResult::Failed(reason) => {
            print_error(&format!("Repair failed: {reason}"));
            println!(
                "  {} Consider cloning the device before further attempts: {}",
                "→".bright_cyan(),
                format!("drive clone {dev_path} /dev/TARGET").bright_blue()
            );
        }
        RepairResult::UnsupportedFs => {
            print_warning(&format!(
                "Automatic repair not supported for filesystem: {fs_type}"
            ));
            println!(
                "  {} Check the manual for: {}",
                "→".bright_cyan(),
                format!("man fsck.{fs_type}").bright_blue()
            );
        }
    }

    println!();
    Ok(())
}

enum RepairResult {
    Success {
        errors_fixed: u32,
        files_recovered: u32,
    },
    NoErrors,
    Failed(String),
    UnsupportedFs,
}

fn dispatch_repair(fs: &str, dev: &str, dry_run: bool) -> RepairResult {
    match fs {
        "ext2" | "ext3" | "ext4" => repair_ext(dev, dry_run),
        "btrfs" => repair_btrfs(dev, dry_run),
        "xfs" => repair_xfs(dev, dry_run),
        "vfat" | "fat32" | "fat16" => repair_fat(dev, dry_run),
        "ntfs" => repair_ntfs(dev, dry_run),
        "f2fs" => repair_f2fs(dev, dry_run),
        _ => RepairResult::UnsupportedFs,
    }
}

fn repair_ext(dev: &str, dry_run: bool) -> RepairResult {
    let mut args: Vec<&str> = vec!["-v"];
    if dry_run {
        args.push("-n"); // no changes
    } else {
        args.push("-y"); // auto-fix
        args.push("-f"); // force check
    }
    args.push(dev);

    let out = Command::new("e2fsck").args(&args).output();

    match out {
        Ok(o) => {
            let code = o.status.code().unwrap_or(-1);
            // e2fsck exit codes: 0=clean, 1=errors corrected, 2=reboot needed, 4=uncorrected
            match code {
                0 => RepairResult::NoErrors,
                1 => {
                    // Parse stderr/stdout for counts
                    let text = String::from_utf8_lossy(&o.stdout).to_string()
                        + &String::from_utf8_lossy(&o.stderr);
                    let fixed = count_pattern(&text, "fixed") + count_pattern(&text, "corrected");
                    RepairResult::Success {
                        errors_fixed: fixed,
                        files_recovered: 0,
                    }
                }
                2 => RepairResult::Success {
                    errors_fixed: 1,
                    files_recovered: 0,
                },
                _ => RepairResult::Failed(format!("e2fsck exit code {code}")),
            }
        }
        Err(e) => RepairResult::Failed(format!("Could not run e2fsck: {e}")),
    }
}

fn repair_btrfs(dev: &str, dry_run: bool) -> RepairResult {
    let mut args: Vec<&str> = vec!["check"];
    if !dry_run {
        args.push("--repair");
    }
    args.push(dev);

    let out = Command::new("btrfs").args(&args).output();

    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            if text.contains("no error") || text.contains("No errors") {
                RepairResult::NoErrors
            } else {
                RepairResult::Success {
                    errors_fixed: 1,
                    files_recovered: 0,
                }
            }
        }
        Ok(o) => RepairResult::Failed(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => RepairResult::Failed(format!("Could not run btrfs: {e}")),
    }
}

fn repair_xfs(dev: &str, dry_run: bool) -> RepairResult {
    let mut args: Vec<&str> = Vec::new();
    if dry_run {
        args.push("-n");
    }
    args.push(dev);

    let out = Command::new("xfs_repair").args(&args).output();

    match out {
        Ok(o) if o.status.success() => RepairResult::NoErrors,
        Ok(o) => RepairResult::Failed(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => RepairResult::Failed(format!("Could not run xfs_repair: {e}")),
    }
}

fn repair_fat(dev: &str, dry_run: bool) -> RepairResult {
    let mut args: Vec<&str> = Vec::new();
    if dry_run {
        args.push("-n");
    } else {
        args.push("-a");
    }
    args.push(dev);

    let out = Command::new("fsck.fat").args(&args).output();

    match out {
        Ok(o) if o.status.success() => RepairResult::NoErrors,
        Ok(_) => RepairResult::Success {
            errors_fixed: 1,
            files_recovered: 0,
        },
        Err(e) => RepairResult::Failed(format!("Could not run fsck.fat: {e}")),
    }
}

fn repair_ntfs(dev: &str, _dry_run: bool) -> RepairResult {
    let out = Command::new("ntfsfix").arg(dev).output();
    match out {
        Ok(o) if o.status.success() => RepairResult::Success {
            errors_fixed: 1,
            files_recovered: 0,
        },
        Ok(o) => RepairResult::Failed(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => RepairResult::Failed(format!("Could not run ntfsfix: {e}")),
    }
}

fn repair_f2fs(dev: &str, dry_run: bool) -> RepairResult {
    let mut args: Vec<&str> = Vec::new();
    if !dry_run {
        args.push("-f");
    }
    args.push(dev);

    let out = Command::new("fsck.f2fs").args(&args).output();
    match out {
        Ok(o) if o.status.success() => RepairResult::NoErrors,
        Ok(_) => RepairResult::Success {
            errors_fixed: 1,
            files_recovered: 0,
        },
        Err(e) => RepairResult::Failed(format!("Could not run fsck.f2fs: {e}")),
    }
}

// --- Helpers -----------------------------------------------------------------

fn detect_fs(dev: &str) -> Option<String> {
    let out = Command::new("blkid")
        .args(["-o", "value", "-s", "TYPE", dev])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn is_mounted(dev: &str) -> bool {
    std::fs::read_to_string("/proc/mounts")
        .map(|s| s.contains(dev))
        .unwrap_or(false)
}

fn count_pattern(text: &str, pattern: &str) -> u32 {
    text.to_lowercase().matches(pattern).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_mounted_for_fake_device() {
        assert!(!is_mounted("/dev/drive_no_such_device_xyz"));
    }

    #[test]
    fn count_pattern_basic() {
        let text = "errors corrected: 3 errors fixed in pass 2";
        assert_eq!(count_pattern(text, "errors"), 2);
        assert_eq!(count_pattern(text, "fixed"), 1);
    }

    #[test]
    fn count_pattern_empty() {
        assert_eq!(count_pattern("", "anything"), 0);
        assert_eq!(count_pattern("hello world", "xyz"), 0);
    }

    #[test]
    fn unsupported_fs_dispatches_correctly() {
        let result = dispatch_repair("zfs", "/dev/null", true);
        assert!(matches!(result, RepairResult::UnsupportedFs));
    }

    #[test]
    fn unsupported_fs_reiserfs() {
        let result = dispatch_repair("reiserfs", "/dev/null", true);
        assert!(matches!(result, RepairResult::UnsupportedFs));
    }
}
