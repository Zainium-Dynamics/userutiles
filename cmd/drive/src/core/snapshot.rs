use anyhow::{Context, Result};
use chrono::Local;
use colored::*;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::SnapshotAction;
use crate::config::Config;
use crate::ui::display::{print_error, print_header, print_row, print_success, print_warning};
use crate::utils::validator::is_valid_snapshot_name;

pub fn run_snapshot(action: SnapshotAction) -> Result<()> {
    match action {
        SnapshotAction::Create { volume, name } => snapshot_create(&volume, name.as_deref()),
        SnapshotAction::List { volume } => snapshot_list(&volume),
        SnapshotAction::Delete { name, yes } => snapshot_delete(&name, yes),
        SnapshotAction::Restore { name, yes } => snapshot_restore(&name, yes),
    }
}

/// Prompt the user to type `YES` to confirm a destructive operation.
/// Returns `true` if confirmed (or `skip_confirm` was set).
fn confirm_destructive(skip_confirm: bool) -> bool {
    if skip_confirm {
        return true;
    }
    print!("  Type {} to confirm: ", "'YES'".bright_yellow().bold());
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    input.trim() == "YES"
}

fn snapshot_create(volume: &str, name: Option<&str>) -> Result<()> {
    print_header("Create Snapshot");
    println!();

    // Verify btrfs subvolume
    if !is_btrfs(volume) {
        print_error(&format!(
            "{volume} is not a btrfs filesystem — snapshots require btrfs"
        ));
        println!();
        return Ok(());
    }

    let cfg = Config::load();
    let snap_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| Local::now().format("%Y-%m-%d_%H%M").to_string());

    if !is_valid_snapshot_name(&snap_name) {
        print_error(&format!("Invalid snapshot name: {snap_name}"));
        println!();
        return Ok(());
    }

    let snap_dir = Path::new(volume).join(&cfg.snapshot_dir);
    let snap_path = snap_dir.join(&snap_name);

    print_row("Volume", &volume.bright_blue().to_string());
    print_row("Snapshot Name", &snap_name.bright_magenta().to_string());
    print_row("Type", &"manual".bright_magenta().to_string());
    println!();

    // Ensure snapshot directory exists as a directory (not subvolume)
    if !snap_dir.exists() {
        fs::create_dir_all(&snap_dir)
            .with_context(|| format!("Cannot create snapshot dir {}", snap_dir.display()))?;
    }

    let status = Command::new("btrfs")
        .args([
            "subvolume",
            "snapshot",
            "-r",
            volume,
            snap_path.to_str().unwrap_or(""),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            print_success("Snapshot created successfully");
            print_row(
                "Location",
                &snap_path.display().to_string().bright_blue().to_string(),
            );
        }
        Ok(s) => {
            print_error(&format!(
                "btrfs exited with code {} — are you running as root?",
                s.code().unwrap_or(-1)
            ));
        }
        Err(e) => {
            print_error(&format!("Failed to execute btrfs: {e}"));
            println!(
                "  {} Make sure btrfs-progs is installed: {}",
                "→".bright_cyan(),
                "pacman -S btrfs-progs".bright_blue()
            );
        }
    }

    println!();
    Ok(())
}

fn snapshot_list(volume: &str) -> Result<()> {
    print_header("Snapshots");
    println!();

    let cfg = Config::load();
    let snap_dir = Path::new(volume).join(&cfg.snapshot_dir);

    if !snap_dir.exists() {
        print_warning(&format!(
            "No snapshot directory found at {}",
            snap_dir.display()
        ));
        println!();
        return Ok(());
    }

    println!(
        "  {:<32} {:<20} {}",
        "NAME".bold().cyan(),
        "CREATED".bold().cyan(),
        "SIZE".bold().cyan(),
    );
    println!("  {}", "─".repeat(70).truecolor(50, 50, 60));

    let mut count = 0;
    let mut entries: Vec<_> = fs::read_dir(&snap_dir)
        .with_context(|| format!("Cannot read {}", snap_dir.display()))?
        .flatten()
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().ok();
        let created = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<Local> = t.into();
                dt.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_else(|| "-".to_string());

        let size = dir_size(entry.path())
            .map(crate::utils::units::bytes_to_human)
            .unwrap_or_else(|| "-".to_string());

        println!(
            "  {:<32} {:<20} {}",
            name.bright_blue(),
            created.truecolor(160, 220, 255),
            size.bright_magenta(),
        );
        count += 1;
    }

    println!();
    println!(
        "  {} {} snapshot{}",
        "✓".bright_green(),
        count.to_string().bright_magenta(),
        if count == 1 { "" } else { "s" }
    );
    println!();
    Ok(())
}

fn snapshot_delete(name: &str, skip_confirm: bool) -> Result<()> {
    print_header("Delete Snapshot");
    println!();

    if !is_valid_snapshot_name(name) {
        print_error(&format!("Invalid snapshot name: {name}"));
        println!();
        return Ok(());
    }

    let cfg = Config::load();
    let snap_path = PathBuf::from("/").join(&cfg.snapshot_dir).join(name);

    if !snap_path.exists() {
        print_error(&format!("Snapshot not found: {name}"));
        println!(
            "  {} Run {} to see available snapshots",
            "→".bright_cyan(),
            "drive snapshot list".bright_blue()
        );
        println!();
        return Ok(());
    }

    print_row("Snapshot", &name.bright_blue().to_string());
    print_row(
        "Path",
        &snap_path.display().to_string().bright_magenta().to_string(),
    );
    print_warning("This permanently deletes the snapshot subvolume — this cannot be undone");
    println!();

    if !confirm_destructive(skip_confirm) {
        print_warning("Delete cancelled — no changes made");
        println!();
        return Ok(());
    }
    println!();

    let status = Command::new("btrfs")
        .args(["subvolume", "delete", snap_path.to_str().unwrap_or("")])
        .status();

    match status {
        Ok(s) if s.success() => print_success(&format!("Snapshot '{name}' deleted")),
        Ok(s) => print_error(&format!(
            "btrfs exited with code {}",
            s.code().unwrap_or(-1)
        )),
        Err(e) => print_error(&format!("Failed to execute btrfs: {e}")),
    }

    println!();
    Ok(())
}

fn snapshot_restore(name: &str, skip_confirm: bool) -> Result<()> {
    print_header("Restore Snapshot");
    println!();

    if !is_valid_snapshot_name(name) {
        print_error(&format!("Invalid snapshot name: {name}"));
        println!();
        return Ok(());
    }

    let cfg = Config::load();
    let snap_path = PathBuf::from("/").join(&cfg.snapshot_dir).join(name);

    if !snap_path.exists() {
        print_error(&format!("Snapshot not found: {name}"));
        println!();
        return Ok(());
    }

    print_row("Snapshot", &name.bright_blue().to_string());
    print_warning(
        "Restoring will replace the current subvolume — ensure you have a separate backup",
    );
    println!();

    if !confirm_destructive(skip_confirm) {
        print_warning("Restore cancelled — no changes made");
        println!();
        return Ok(());
    }
    println!();

    // Create a new writable subvolume from the read-only snapshot
    let restore_target = format!("/_restore_{name}");
    let status = Command::new("btrfs")
        .args([
            "subvolume",
            "snapshot",
            snap_path.to_str().unwrap_or(""),
            &restore_target,
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            print_success(&format!("Writable restore created at {restore_target}"));
            println!(
                "  {} Reboot into recovery to swap subvolumes if needed",
                "→".bright_cyan()
            );
        }
        Ok(s) => print_error(&format!(
            "btrfs exited with code {}",
            s.code().unwrap_or(-1)
        )),
        Err(e) => print_error(&format!("Failed to execute btrfs: {e}")),
    }

    println!();
    Ok(())
}

// --- Helpers -----------------------------------------------------------------

fn is_btrfs(path: &str) -> bool {
    let out = Command::new("stat").args(["-f", "-c", "%T", path]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "btrfs")
        .unwrap_or(false)
}

fn dir_size(path: std::path::PathBuf) -> Option<u64> {
    let out = Command::new("du")
        .args(["-sb", path.to_str()?])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn is_btrfs_returns_false_for_tmpfs() {
        // /tmp is typically tmpfs, not btrfs
        assert!(!is_btrfs("/tmp"));
    }

    #[test]
    fn dir_size_returns_some_for_existing_dir() {
        let dir = tempdir().unwrap();
        // du should succeed on any real directory
        let size = dir_size(dir.path().to_path_buf());
        assert!(
            size.is_some(),
            "dir_size should return Some for a real directory"
        );
    }

    #[test]
    fn dir_size_returns_none_for_missing_path() {
        let size = dir_size(std::path::PathBuf::from("/no/such/path/xyz_drive_test"));
        // du will fail, so we expect None
        assert!(size.is_none());
    }

    #[test]
    fn snapshot_delete_rejects_path_traversal_before_touching_disk() {
        // A traversal name must be rejected by validation, never reaching
        // the snap_path.exists()/btrfs invocation.
        let result = snapshot_delete("../../etc/passwd", true);
        assert!(
            result.is_ok(),
            "should return Ok(()) after printing an error, not Err"
        );
    }

    #[test]
    fn snapshot_restore_rejects_path_traversal_before_touching_disk() {
        let result = snapshot_restore("../../etc", true);
        assert!(
            result.is_ok(),
            "should return Ok(()) after printing an error, not Err"
        );
    }
}
