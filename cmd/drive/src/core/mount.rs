use anyhow::{Context, Result};
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::ui::display::{print_error, print_header, print_row, print_success, print_warning};
use crate::utils::units::bytes_to_human;
use crate::utils::validator::normalize_device;

pub fn run_mount(device: &str, custom_mp: Option<&str>, verbose: bool) -> Result<()> {
    let Some(dev_path) = normalize_device(device) else {
        print_error(&format!("Invalid device name: {device}"));
        println!();
        return Ok(());
    };

    print_header("Mount Device");
    println!();

    // Check device exists
    if !Path::new(&dev_path).exists() {
        print_error(&format!("Device not found: {dev_path}"));
        println!("  {} Suggestion: run {} to see available devices",
            "→".bright_cyan(),
            "drive list".bright_blue()
        );
        println!();
        return Ok(());
    }

    // Detect filesystem & label
    let (fs_type, label, size) = probe_device(&dev_path);

    let mountpoint = match custom_mp {
        Some(mp) => PathBuf::from(mp),
        None => suggest_mountpoint(&dev_path, label.as_deref()),
    };

    print_row("Device", &dev_path.bright_blue().to_string());
    print_row(
        "Filesystem",
        &fs_type
            .as_deref()
            .unwrap_or("unknown")
            .bright_magenta()
            .to_string(),
    );
    if let Some(ref lbl) = label {
        print_row("Label", &lbl.bright_magenta().to_string());
    }
    if size > 0 {
        print_row("Size", &bytes_to_human(size).bright_magenta().to_string());
    }
    print_row(
        "Mountpoint",
        &mountpoint
            .display()
            .to_string()
            .bright_magenta()
            .to_string(),
    );
    println!();

    // Create mountpoint if needed
    if !mountpoint.exists() {
        fs::create_dir_all(&mountpoint)
            .with_context(|| format!("Cannot create mountpoint {}", mountpoint.display()))?;
        if verbose {
            println!("  {} Created mountpoint {}",
                "→".bright_cyan(),
                mountpoint.display()
            );
        }
    }

    // Build mount args
    let mut args: Vec<String> = Vec::new();
    if let Some(fs) = &fs_type {
        args.push("-t".to_string());
        args.push(fs.clone());
    }
    args.push(dev_path.clone());
    args.push(mountpoint.to_string_lossy().to_string());

    if verbose {
        println!("  {} Running: mount {}", "→".bright_cyan(), args.join(" "));
    }

    let result = Command::new("mount").args(&args).status();

    match result {
        Ok(status) if status.success() => {
            print_success(&format!(
                "Successfully mounted at {}",
                mountpoint.display().to_string().bright_blue()
            ));
        }
        Ok(status) => {
            print_error(&format!(
                "mount exited with code {}",
                status.code().unwrap_or(-1)
            ));
            maybe_suggest_repair(device);
        }
        Err(e) => {
            print_error(&format!("Failed to execute mount: {e}"));
            println!("  {} Suggestion: Try 'drive repair {device}' first",
                "→".bright_cyan()
            );
        }
    }

    println!();
    Ok(())
}

pub fn run_umount(device: &str, verbose: bool) -> Result<()> {
    let Some(dev_path) = normalize_device(device) else {
        print_error(&format!("Invalid device name: {device}"));
        println!();
        return Ok(());
    };

    print_header("Unmount Device");
    println!();

    if !Path::new(&dev_path).exists() {
        print_error(&format!("Device not found: {dev_path}"));
        println!();
        return Ok(());
    }

    let (fs_type, _, _) = probe_device(&dev_path);
    let mountpoint = current_mountpoint(&dev_path);

    print_row("Device", &dev_path.bright_blue().to_string());
    print_row(
        "Mountpoint",
        &mountpoint
            .as_deref()
            .unwrap_or("not mounted")
            .bright_magenta()
            .to_string(),
    );
    print_row(
        "Filesystem",
        &fs_type
            .as_deref()
            .unwrap_or("unknown")
            .bright_magenta()
            .to_string(),
    );
    println!();

    if mountpoint.is_none() {
        print_warning(&format!("{dev_path} is not currently mounted"));
        println!();
        return Ok(());
    }

    // Sync buffers first
    if verbose {
        println!("  {} Flushing buffers (sync)...", "→".bright_cyan());
    }
    let _ = Command::new("sync").status();

    let result = Command::new("umount").arg(&dev_path).status();

    match result {
        Ok(status) if status.success() => {
            print_success(&format!(
                "Successfully unmounted {}",
                dev_path.bright_blue()
            ));
        }
        Ok(_) => {
            // Try lazy unmount
            let lazy = Command::new("umount").args(["-l", &dev_path]).status();
            if lazy.map(|s| s.success()).unwrap_or(false) {
                print_success("Lazy unmount succeeded (device was busy)");
            } else {
                print_error(&format!(
                    "Failed to unmount {dev_path} — device may be busy"
                ));
                println!("  {} Tip: check open files with {} or {}",
                    "→".bright_cyan(),
                    "lsof +f -- /mountpoint".bright_blue(),
                    "fuser -m /mountpoint".bright_blue()
                );
            }
        }
        Err(e) => {
            print_error(&format!("Failed to execute umount: {e}"));
        }
    }

    println!();
    Ok(())
}

// --- Helpers -----------------------------------------------------------------

/// Returns (fstype, label, size_bytes)
fn probe_device(dev: &str) -> (Option<String>, Option<String>, u64) {
    let blkid = Command::new("blkid").args(["-o", "export", dev]).output();

    let mut fstype = None;
    let mut label = None;

    if let Ok(out) = blkid {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("TYPE=") {
                fstype = Some(v.trim().to_string());
            }
            if let Some(v) = line.strip_prefix("LABEL=") {
                label = Some(v.trim().to_string());
            }
        }
    }

    // Size from blockdev
    let size = Command::new("blockdev")
        .args(["--getsize64", dev])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0);

    (fstype, label, size)
}

fn suggest_mountpoint(dev: &str, label: Option<&str>) -> PathBuf {
    let cfg = Config::load();
    let base = cfg.mount_base;

    if let Some(lbl) = label {
        return base.join(lbl.to_lowercase().replace(' ', "_"));
    }

    // Use last component of device name
    let short = Path::new(dev)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "drive".to_string());

    base.join(short)
}

fn current_mountpoint(dev: &str) -> Option<String> {
    let content = fs::read_to_string("/proc/mounts").ok()?;
    for line in content.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 2 && cols[0] == dev {
            return Some(cols[1].to_string());
        }
    }
    None
}

fn maybe_suggest_repair(device: &str) {
    println!("  {} Suggestion: Try {}",
        "→".bright_cyan(),
        format!("drive repair {device}").bright_blue()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggest_mountpoint_uses_label() {
        let mp = suggest_mountpoint("/dev/sdb1", Some("MY_DRIVE"));
        assert!(
            mp.to_string_lossy().contains("my_drive"),
            "expected label-based mountpoint, got {}",
            mp.display()
        );
    }

    #[test]
    fn suggest_mountpoint_falls_back_to_device_name() {
        let mp = suggest_mountpoint("/dev/sdb1", None);
        assert!(
            mp.to_string_lossy().ends_with("sdb1"),
            "expected device-name-based mountpoint, got {}",
            mp.display()
        );
    }

    #[test]
    fn current_mountpoint_nonexistent_returns_none() {
        let mp = current_mountpoint("/dev/drive_no_such_xyz");
        assert!(mp.is_none());
    }

    #[test]
    fn probe_device_dev_null_does_not_panic() {
        // blkid on /dev/null won't crash — may return empty
        let (_fs, _label, _size) = probe_device("/dev/null");
        // Just ensure it runs without panic
    }
}
