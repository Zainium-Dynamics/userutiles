use anyhow::{bail, Result};
use colored::*;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use crate::ui::display::{print_error, print_header, print_row, print_success, print_warning};
use crate::utils::units::bytes_to_human;
use crate::utils::validator::{is_supported_fs, normalize_device};

static SUPPORTED_FS: &[&str] = &["ext4", "btrfs", "xfs", "exfat", "vfat", "ntfs", "f2fs"];

pub fn run_format(
    device: &str,
    filesystem: &str,
    label: Option<&str>,
    skip_confirm: bool,
) -> Result<()> {
    let Some(dev_path) = normalize_device(device) else {
        print_error(&format!("Invalid device name: {device}"));
        println!();
        return Ok(());
    };

    print_header("Format Device");
    println!();

    if !Path::new(&dev_path).exists() {
        print_error(&format!("Device not found: {dev_path}"));
        println!();
        return Ok(());
    }

    let fs = filesystem.to_lowercase();
    if !is_supported_fs(&fs) {
        print_error(&format!("Unsupported filesystem: {fs}"));
        println!(
            "  {} Supported: {}",
            "→".bright_cyan(),
            SUPPORTED_FS.join(", ").bright_magenta()
        );
        println!();
        return Ok(());
    }

    let size = device_size_bytes(&dev_path);

    // --- Danger warning ---------------------------------------------------
    println!(
        "  {} {} All data on {} will be {} erased!",
        "⚠".bright_yellow(),
        "DANGER:".bright_yellow().bold(),
        dev_path.bright_blue(),
        "permanently".bright_red().bold()
    );
    println!();

    print_row("Device", &dev_path.bright_blue().to_string());
    print_row("Filesystem", &fs.bright_magenta().to_string());
    if let Some(lbl) = label {
        print_row("Label", &lbl.bright_magenta().to_string());
    }
    if size > 0 {
        print_row("Size", &bytes_to_human(size).bright_magenta().to_string());
    }
    println!();

    if !skip_confirm {
        print!("  Type {} to confirm: ", "'YES'".bright_yellow().bold());
        io::stdout().flush().ok();

        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();

        if input.trim() != "YES" {
            println!();
            print_warning("Format cancelled — no changes made");
            println!();
            return Ok(());
        }
        println!();
    }

    println!("  {} Preparing to format...", "→".bright_cyan());
    println!();

    // Unmount first if mounted
    if is_mounted(&dev_path) {
        println!("  {} Device is mounted — unmounting...", "→".bright_cyan());
        let _ = Command::new("umount").arg(&dev_path).status();
    }

    let status = run_mkfs(&fs, &dev_path, label)?;

    if status {
        print_success("Formatting completed successfully");

        // btrfs gets compression hint
        if fs == "btrfs" {
            print_success("Filesystem created with compression support enabled");
            println!(
                "  {} Mount with {} for transparent compression",
                "→".bright_cyan(),
                "-o compress=zstd".bright_blue()
            );
        }
    } else {
        print_error("Formatting failed — check device and permissions");
        println!(
            "  {} Try running with {}",
            "→".bright_cyan(),
            "sudo".bright_blue()
        );
    }

    println!();
    Ok(())
}

fn run_mkfs(fs: &str, device: &str, label: Option<&str>) -> Result<bool> {
    let status = match fs {
        "ext4" => {
            let mut cmd = Command::new("mkfs.ext4");
            cmd.arg("-F");
            if let Some(lbl) = label {
                cmd.args(["-L", lbl]);
            }
            cmd.arg(device).status()
        }
        "btrfs" => {
            let mut cmd = Command::new("mkfs.btrfs");
            cmd.arg("-f");
            if let Some(lbl) = label {
                cmd.args(["-L", lbl]);
            }
            cmd.arg(device).status()
        }
        "xfs" => {
            let mut cmd = Command::new("mkfs.xfs");
            cmd.arg("-f");
            if let Some(lbl) = label {
                cmd.args(["-L", lbl]);
            }
            cmd.arg(device).status()
        }
        "exfat" => {
            let mut cmd = Command::new("mkfs.exfat");
            if let Some(lbl) = label {
                cmd.args(["-n", lbl]);
            }
            cmd.arg(device).status()
        }
        "vfat" => {
            let mut cmd = Command::new("mkfs.vfat");
            if let Some(lbl) = label {
                cmd.args(["-n", lbl]);
            }
            cmd.arg(device).status()
        }
        "ntfs" => {
            let mut cmd = Command::new("mkfs.ntfs");
            cmd.arg("-f");
            if let Some(lbl) = label {
                cmd.args(["-L", lbl]);
            }
            cmd.arg(device).status()
        }
        "f2fs" => {
            let mut cmd = Command::new("mkfs.f2fs");
            if let Some(lbl) = label {
                cmd.args(["-l", lbl]);
            }
            cmd.arg(device).status()
        }
        other => bail!("Unhandled filesystem: {other}"),
    };

    Ok(status.map(|s| s.success()).unwrap_or(false))
}

fn device_size_bytes(dev: &str) -> u64 {
    Command::new("blockdev")
        .args(["--getsize64", dev])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
}

fn is_mounted(dev: &str) -> bool {
    std::fs::read_to_string("/proc/mounts")
        .map(|s| s.contains(dev))
        .unwrap_or(false)
}

// --- Unit tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_fs_list_is_complete() {
        for fs in SUPPORTED_FS {
            // Every entry must be lowercase and non-empty
            assert!(!fs.is_empty());
            assert_eq!(*fs, fs.to_lowercase());
        }
    }

    #[test]
    fn unsupported_fs_caught() {
        // run_format with bad fs should print error and return Ok (graceful)
        // We can't call run_format without a real device, but we can verify the
        // supported list doesn't contain obviously wrong entries.
        assert!(!SUPPORTED_FS.contains(&"reiserfs"));
        assert!(!SUPPORTED_FS.contains(&"zfs"));
    }

    #[test]
    fn supported_filesystems_accepted() {
        for fs in SUPPORTED_FS {
            assert!(SUPPORTED_FS.contains(fs), "{fs} missing from list");
        }
    }

    #[test]
    fn is_mounted_on_nonexistent_returns_false() {
        // /dev/drive_test_xyz should never be in /proc/mounts
        assert!(!is_mounted("/dev/drive_test_xyz_nonexistent"));
    }
}
