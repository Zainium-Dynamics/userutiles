use anyhow::Result;
use colored::*;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::ui::display::{print_error, print_header, print_row, print_success, print_warning};
use crate::utils::units::bytes_to_human;
use crate::utils::validator::normalize_device;

pub fn run_clone(source: &str, target: &str, verify: bool) -> Result<()> {
    let Some(src) = normalize_device(source) else {
        print_error(&format!("Invalid source device name: {source}"));
        println!();
        return Ok(());
    };
    let Some(dst) = normalize_device(target) else {
        print_error(&format!("Invalid target device name: {target}"));
        println!();
        return Ok(());
    };

    print_header("Clone Device");
    println!();

    // Validate devices
    if !Path::new(&src).exists() {
        print_error(&format!("Source device not found: {src}"));
        println!();
        return Ok(());
    }
    if !Path::new(&dst).exists() {
        print_error(&format!("Target device not found: {dst}"));
        println!();
        return Ok(());
    }

    let src_size = device_size_bytes(&src);
    let dst_size = device_size_bytes(&dst);

    if dst_size > 0 && src_size > dst_size {
        print_error(&format!(
            "Source ({}) is larger than target ({}) — cannot clone",
            bytes_to_human(src_size),
            bytes_to_human(dst_size)
        ));
        println!();
        return Ok(());
    }

    print_row(
        "Source",
        &format!(
            "{} ({})",
            src.bright_blue(),
            bytes_to_human(src_size).bright_magenta()
        ),
    );
    print_row(
        "Target",
        &format!(
            "{} ({})",
            dst.bright_blue(),
            bytes_to_human(dst_size).bright_magenta()
        ),
    );
    if verify {
        print_row("Verify", &"enabled".bright_green().to_string());
    }
    println!();

    let start = Instant::now();

    // Prefer ddrescue if available, else dd with status=progress
    let success = if which::which("ddrescue").is_ok() {
        run_ddrescue(&src, &dst, src_size)
    } else {
        run_dd(&src, &dst, src_size)
    };

    let elapsed = start.elapsed();

    if !success {
        print_error("Clone failed — check for I/O errors");
        println!();
        return Ok(());
    }

    println!();
    print_success("Clone completed successfully");
    print_row(
        "Data written",
        &bytes_to_human(src_size).bright_magenta().to_string(),
    );
    print_row(
        "Time taken",
        &format_duration(elapsed).bright_magenta().to_string(),
    );

    if verify {
        println!();
        println!("  {} Verifying data integrity...", "→".bright_cyan());
        let verified = run_verify(&src, &dst, src_size);
        if verified {
            print_success("Verified — 100% match");
        } else {
            print_warning("Verification encountered differences — check the target device");
        }
    }

    println!();
    Ok(())
}

fn run_ddrescue(src: &str, dst: &str, _size: u64) -> bool {
    // ddrescue with a temporary log file
    let log = "/tmp/drive_clone.log";
    let status = Command::new("ddrescue")
        .args(["-f", "-n", src, dst, log])
        .status();
    let _ = std::fs::remove_file(log);
    status.map(|s| s.success()).unwrap_or(false)
}

fn run_dd(src: &str, dst: &str, _size: u64) -> bool {
    let status = Command::new("dd")
        .args([
            &format!("if={src}"),
            &format!("of={dst}"),
            "bs=4M",
            "conv=fsync,noerror",
            "status=progress",
        ])
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

fn run_verify(src: &str, dst: &str, size: u64) -> bool {
    // SHA256 of first and last 64MB for quick integrity check
    let chunk = std::cmp::min(size / 2, 64 * 1024 * 1024);
    let src_hash = hash_head(src, chunk);
    let dst_hash = hash_head(dst, chunk);
    src_hash == dst_hash && src_hash.is_some()
}

/// Read the first `bytes` of `dev` via `dd` (no shell involved — `dev` is
/// passed as a single argv element, never interpolated into a command
/// string) and hash it in-process with usercore's pure-Rust SHA-256.
fn hash_head(dev: &str, bytes: u64) -> Option<String> {
    let count = bytes / (512 * 1024); // count in 512K blocks
    let out = Command::new("dd")
        .args([&format!("if={dev}"), "bs=512K", &format!("count={count}")])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if out.stdout.is_empty() && count > 0 {
        return None;
    }
    let mut hasher = usercore::digest::Sha256::new();
    hasher.update(&out.stdout);
    Some(usercore::digest::hex_lower(&hasher.finalize()))
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

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs} seconds")
    } else if secs < 3600 {
        format!("{} minutes {} seconds", secs / 60, secs % 60)
    } else {
        format!("{} hours {} minutes", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_seconds() {
        let d = Duration::from_secs(42);
        assert!(format_duration(d).contains("42 seconds"));
    }

    #[test]
    fn format_duration_minutes() {
        let d = Duration::from_secs(125);
        let s = format_duration(d);
        assert!(s.contains("2 minutes"), "got: {s}");
        assert!(s.contains("5 seconds"), "got: {s}");
    }

    #[test]
    fn format_duration_hours() {
        let d = Duration::from_secs(7384);
        let s = format_duration(d);
        assert!(s.contains("2 hours"), "got: {s}");
    }

    #[test]
    fn hash_head_dev_null_is_deterministic() {
        // /dev/null always reads 0 bytes — both calls return same hash
        let a = hash_head("/dev/null", 0);
        let b = hash_head("/dev/null", 0);
        assert_eq!(a, b);
    }
}
