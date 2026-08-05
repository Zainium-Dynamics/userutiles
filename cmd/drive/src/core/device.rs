use anyhow::{Context, Result};
use colored::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::ui::display::{print_error, print_header, print_row, print_success};
use crate::utils::units::bytes_to_human;

/// Represents a block device parsed from /proc/partitions and /sys/block
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BlockDevice {
    pub name: String,
    pub size_bytes: u64,
    pub removable: bool,
    pub rotational: bool,
    pub model: String,
    pub transport: String, // nvme, sata, usb, mmc, virtio
    pub partitions: Vec<BlockDevice>,
    pub mountpoint: Option<String>,
    pub filesystem: Option<String>,
    pub label: Option<String>,
    pub use_percent: Option<u8>,
}

impl BlockDevice {
    #[allow(dead_code)]
    pub fn is_partition(&self) -> bool {
        self.name.contains(|c: char| c.is_ascii_digit())
    }

    pub fn device_type(&self) -> &str {
        if self.name.starts_with("nvme") {
            "NVMe"
        } else if self.rotational {
            "HDD"
        } else {
            "SSD"
        }
    }
}

/// Read all top-level block devices from /sys/block
pub fn enumerate_devices() -> Result<Vec<BlockDevice>> {
    let mut devices = Vec::new();
    let mounts = read_mounts()?;
    let labels = read_labels();

    let block_dir = Path::new("/sys/block");
    if !block_dir.exists() {
        return Err(anyhow::anyhow!(
            "Cannot access /sys/block — are you on Linux?"
        ));
    }

    for entry in fs::read_dir(block_dir).context("Failed to read /sys/block")? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip loop, ram, dm devices unless they have mounts
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
            continue;
        }

        if let Ok(mut dev) = read_device(&name, &mounts, &labels) {
            // Read partitions
            let part_dir = entry.path();
            let mut parts: Vec<BlockDevice> = Vec::new();

            for p in fs::read_dir(&part_dir).into_iter().flatten().flatten() {
                let pname = p.file_name().to_string_lossy().to_string();
                if pname.starts_with(&name) && pname != name {
                    if let Ok(part) = read_device(&pname, &mounts, &labels) {
                        parts.push(part);
                    }
                }
            }

            parts.sort_by(|a, b| a.name.cmp(&b.name));
            dev.partitions = parts;
            devices.push(dev);
        }
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

fn read_device(
    name: &str,
    mounts: &HashMap<String, (String, String)>,
    labels: &HashMap<String, String>,
) -> Result<BlockDevice> {
    let base = format!("/sys/block/{name}");
    let part_base = format!("/sys/class/block/{name}");
    let sys_path = if Path::new(&base).exists() {
        base
    } else {
        part_base
    };

    let size_sectors = read_sys_u64(&format!("{sys_path}/size")).unwrap_or(0);
    let size_bytes = size_sectors * 512;

    let removable = read_sys_u64(&format!("{sys_path}/removable")).unwrap_or(0) == 1;
    let rotational = read_sys_u64(&format!("{sys_path}/queue/rotational")).unwrap_or(0) == 1;

    let model = read_sys_str(&format!("/sys/block/{}/device/model", root_device(name)))
        .unwrap_or_else(|_| "Unknown".to_string());

    let transport = detect_transport(name);

    let dev_path = format!("/dev/{name}");
    let (mountpoint, filesystem) = mounts
        .get(&dev_path)
        .cloned()
        .unwrap_or((String::new(), String::new()));

    let mountpoint = if mountpoint.is_empty() {
        None
    } else {
        Some(mountpoint)
    };
    let filesystem = if filesystem.is_empty() {
        None
    } else {
        Some(filesystem)
    };

    let label = labels.get(&dev_path).cloned();

    let use_percent = if mountpoint.is_some() {
        mountpoint.as_deref().and_then(disk_usage_percent)
    } else {
        None
    };

    Ok(BlockDevice {
        name: name.to_string(),
        size_bytes,
        removable,
        rotational,
        model: model.trim().to_string(),
        transport,
        partitions: Vec::new(),
        mountpoint,
        filesystem,
        label,
        use_percent,
    })
}

fn root_device(name: &str) -> String {
    // nvme0n1p1 -> nvme0n1 ; nvme0n1 -> nvme0n1 ; sda1 -> sda
    if name.starts_with("nvme") {
        if let Some(pos) = name.rfind('p') {
            let suffix = &name[pos + 1..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                return name[..pos].to_string();
            }
        }
        return name.to_string();
    }

    name.trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}

fn read_sys_u64(path: &str) -> Result<u64> {
    let s = fs::read_to_string(path)?;
    Ok(s.trim().parse()?)
}

fn read_sys_str(path: &str) -> Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_string())
}

fn detect_transport(name: &str) -> String {
    if name.starts_with("nvme") {
        "NVMe".to_string()
    } else if name.starts_with("sd") {
        // Check if USB via /sys
        let link = fs::read_link(format!("/sys/block/{name}"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if link.contains("usb") {
            "USB".to_string()
        } else {
            "SATA".to_string()
        }
    } else if name.starts_with("mmcblk") {
        "MMC".to_string()
    } else if name.starts_with("vd") {
        "VirtIO".to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Parse /proc/mounts to get device -> (mountpoint, fstype)
fn read_mounts() -> Result<HashMap<String, (String, String)>> {
    let mut map = HashMap::new();
    let content = fs::read_to_string("/proc/mounts").context("Cannot read /proc/mounts")?;
    for line in content.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 3 {
            let dev = cols[0].to_string();
            let mp = cols[1].to_string();
            let fs = cols[2].to_string();
            map.insert(dev, (mp, fs));
        }
    }
    Ok(map)
}

/// Read labels from /dev/disk/by-label
fn read_labels() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let label_dir = Path::new("/dev/disk/by-label");
    if let Ok(entries) = fs::read_dir(label_dir) {
        for entry in entries.flatten() {
            let label = entry.file_name().to_string_lossy().to_string();
            if let Ok(target) = fs::read_link(entry.path()) {
                if let Some(dev_name) = target.file_name() {
                    let dev_path = format!("/dev/{}", dev_name.to_string_lossy());
                    map.insert(dev_path, label);
                }
            }
        }
    }
    map
}

/// Get disk usage percentage for a mountpoint via statvfs
fn disk_usage_percent(mp: &str) -> Option<u8> {
    use std::ffi::CString;
    let path = CString::new(mp).ok()?;
    // SAFETY: `libc::statvfs` has no `Default` impl — it is a
    // plain-old-data FFI struct made up entirely of unsigned integer
    // fields (block/inode counts, flags, name-length limits), for which
    // an all-zero bit pattern is always a valid value. It is immediately
    // overwritten by the `libc::statvfs` call below on success, and only
    // read after checking that call's return value is 0.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a valid, NUL-terminated `CString` kept alive for
    // the duration of this call, so `path.as_ptr()` is a sound path
    // argument. `&mut stat` is a valid, properly aligned reference to a
    // local `statvfs` for `statvfs(3)` to write into; the fields read
    // below (`f_blocks`, `f_bfree`) are only used after checking `ret ==
    // 0`, i.e. only once the kernel has actually populated them.
    let ret = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if ret == 0 && stat.f_blocks > 0 {
        let used = stat.f_blocks - stat.f_bfree;
        let pct = (used as f64 / stat.f_blocks as f64 * 100.0) as u8;
        Some(pct)
    } else {
        None
    }
}

pub fn run_list(as_toml: bool) -> Result<()> {
    let devices = enumerate_devices().unwrap_or_else(|e| {
        print_error(&e.to_string());
        Vec::new()
    });

    if as_toml {
        #[derive(serde::Serialize)]
        struct Row<'a> {
            name: &'a str,
            size: u64,
            #[serde(rename = "type")]
            device_type: String,
            filesystem: Option<&'a str>,
            mountpoint: Option<&'a str>,
            model: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Out<'a> {
            devices: Vec<Row<'a>>,
        }
        let out = Out {
            devices: devices
                .iter()
                .map(|d| Row {
                    name: &d.name,
                    size: d.size_bytes,
                    device_type: d.device_type().to_string(),
                    filesystem: d.filesystem.as_deref(),
                    mountpoint: d.mountpoint.as_deref(),
                    model: &d.model,
                })
                .collect(),
        };
        println!("{}", toml::to_string_pretty(&out)?);
        return Ok(());
    }

    print_header("Storage Overview");
    println!();

    // Header row
    println!("  {:<14} {:<8} {:<7} {:<9} {:<24} {:<11} {:<6} {}",
        "DEVICE".bold().cyan(),
        "SIZE".bold().cyan(),
        "TYPE".bold().cyan(),
        "FS".bold().cyan(),
        "MOUNTPOINT".bold().cyan(),
        "HEALTH".bold().cyan(),
        "TEMP".bold().cyan(),
        "USE%".bold().cyan(),
    );
    println!("  {}", "─".repeat(96).truecolor(50, 50, 60));

    let mut warnings = 0u32;

    for dev in &devices {
        let size_str = if dev.size_bytes > 0 {
            bytes_to_human(dev.size_bytes)
        } else {
            "-".to_string()
        };

        let health_str = health_badge(&dev.name);
        let temp_str = read_temp_for(&dev.name);
        let mp = dev.mountpoint.as_deref().unwrap_or("-");
        let fs = dev.filesystem.as_deref().unwrap_or("-");
        let use_str = dev
            .use_percent
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "-".to_string());

        let dev_colored = format!("{}", dev.name.bright_blue().bold());
        let mp_colored = if mp == "-" {
            "-".dimmed().to_string()
        } else {
            mp.truecolor(160, 220, 255).to_string()
        };

        println!("  {:<23} {:<8} {:<7} {:<9} {:<33} {:<20} {:<6} {}",
            dev_colored,
            size_str.bright_magenta(),
            dev.device_type().truecolor(100, 220, 180),
            fs.truecolor(100, 220, 180),
            mp_colored,
            health_str,
            temp_str,
            use_str.bright_magenta(),
        );

        for (i, part) in dev.partitions.iter().enumerate() {
            let is_last = i + 1 == dev.partitions.len();
            let prefix = if is_last { " └-" } else { " ├-" };
            let psize = if part.size_bytes > 0 {
                bytes_to_human(part.size_bytes)
            } else {
                "-".to_string()
            };
            let pfs = part.filesystem.as_deref().unwrap_or("-");
            let pmp = part.mountpoint.as_deref().unwrap_or("-");
            let puse = part
                .use_percent
                .map(|p| format!("{p}%"))
                .unwrap_or_else(|| "-".to_string());

            println!(
                "{}{:<19} {:<8} {:<7} {:<9} {:<33} {:<20} {:<6} {}",
                prefix.truecolor(80, 80, 100),
                part.name.bright_blue(),
                psize.bright_magenta(),
                part.device_type().truecolor(100, 220, 180),
                pfs.truecolor(100, 220, 180),
                pmp.truecolor(160, 220, 255),
                "".normal(),
                "".normal(),
                puse.bright_magenta(),
            );
        }

        if health_badge_is_warn(&dev.name) {
            warnings += 1;
        }
    }

    println!();
    let count = devices.len();
    if warnings > 0 {
        println!("  {} {} physical drive{} detected - {} warning{}",
            "✓".bright_green(),
            count.to_string().bright_magenta(),
            if count == 1 { "" } else { "s" },
            warnings.to_string().bright_yellow(),
            if warnings == 1 { "" } else { "s" },
        );
    } else {
        println!("  {} {} physical drive{} detected — all healthy",
            "✓".bright_green(),
            count.to_string().bright_magenta(),
            if count == 1 { "" } else { "s" },
        );
    }
    println!();

    Ok(())
}

pub fn run_info(device: &str, as_toml: bool) -> Result<()> {
    let clean = device.trim_start_matches("/dev/");
    let devices = enumerate_devices()?;
    let dev = match devices.iter().find(|d| d.name == clean) {
        Some(dev) => dev,
        None => {
            if as_toml {
                #[derive(serde::Serialize)]
                struct ErrOut<'a> {
                    error: &'a str,
                    device: &'a str,
                }
                println!(
                    "{}",
                    toml::to_string_pretty(&ErrOut {
                        error: "Device not found",
                        device,
                    })?
                );
            } else {
                crate::ui::display::print_error(&format!("Device not found: {device}"));
            }
            return Ok(());
        }
    };

    let serial = read_sys_str(&format!("/sys/block/{}/device/serial", clean))
        .unwrap_or_else(|_| "N/A".to_string());
    let firmware = read_sys_str(&format!("/sys/block/{}/device/firmware_rev", clean))
        .unwrap_or_else(|_| "N/A".to_string());

    if as_toml {
        #[derive(serde::Serialize)]
        struct InfoOut<'a> {
            name: &'a str,
            model: &'a str,
            serial: &'a str,
            firmware: &'a str,
            size_bytes: u64,
            transport: &'a str,
            rotational: bool,
            removable: bool,
        }
        let out = InfoOut {
            name: &dev.name,
            model: &dev.model,
            serial: serial.trim(),
            firmware: firmware.trim(),
            size_bytes: dev.size_bytes,
            transport: &dev.transport,
            rotational: dev.rotational,
            removable: dev.removable,
        };
        println!("{}", toml::to_string_pretty(&out)?);
        return Ok(());
    }

    print_header("Drive Information");
    println!();

    let rows = vec![
        ("Model", dev.model.clone()),
        ("Serial Number", serial.trim().to_string()),
        ("Firmware", firmware.trim().to_string()),
        (
            "Capacity",
            format!(
                "{} ({:.1} GiB usable)",
                bytes_to_human(dev.size_bytes),
                dev.size_bytes as f64 / 1024.0 / 1024.0 / 1024.0
            ),
        ),
        ("Transport", dev.transport.clone()),
        (
            "Rotational",
            if dev.rotational {
                "Yes (HDD)"
            } else {
                "No (SSD/NVMe)"
            }
            .to_string(),
        ),
        (
            "Removable",
            if dev.removable { "Yes" } else { "No" }.to_string(),
        ),
        ("Health", health_label_for(clean)),
        ("Temperature", read_temp_raw(clean)),
        (
            "Mountpoint",
            dev.mountpoint
                .as_deref()
                .unwrap_or("Not mounted")
                .to_string(),
        ),
        (
            "Filesystem",
            dev.filesystem.as_deref().unwrap_or("N/A").to_string(),
        ),
    ];

    for (label, value) in rows {
        print_row(label, &value);
    }

    println!();

    // SMART check
    let smart_status = smart_passed(clean);
    if smart_status {
        print_success("SMART status: PASSED — drive is healthy and performing optimally");
    } else {
        crate::ui::display::print_warning(
            "SMART status: could not be determined (run as root or install smartmontools)",
        );
    }

    println!();
    Ok(())
}

// --- Health helpers ----------------------------------------------------------

fn health_badge(name: &str) -> String {
    let temp = read_temp_celsius(name);
    match temp {
        Some(t) if t >= 55 => format!("● {}", "Critical".bright_red()),
        Some(t) if t >= 45 => format!("● {}", "Warning".bright_yellow()),
        Some(t) if t >= 35 => format!("● {}", "Fair".truecolor(255, 165, 0)),
        _ => format!("● {}", "Excellent".bright_green()),
    }
}

fn health_badge_is_warn(name: &str) -> bool {
    matches!(read_temp_celsius(name), Some(t) if t >= 45)
}

fn health_label_for(name: &str) -> String {
    match read_temp_celsius(name) {
        Some(t) if t >= 55 => "Critical".bright_red().to_string(),
        Some(t) if t >= 45 => "Warning — high temperature".bright_yellow().to_string(),
        Some(t) if t >= 35 => "Fair".truecolor(255, 165, 0).to_string(),
        _ => "Excellent".bright_green().to_string(),
    }
}

fn read_temp_for(name: &str) -> String {
    match read_temp_celsius(name) {
        Some(t) if t >= 55 => format!("{t}°C").bright_red().to_string(),
        Some(t) if t >= 45 => format!("{t}°C").bright_yellow().to_string(),
        Some(t) => format!("{t}°C").bright_magenta().to_string(),
        None => "-".dimmed().to_string(),
    }
}

fn read_temp_raw(name: &str) -> String {
    match read_temp_celsius(name) {
        Some(t) => format!("{t}°C"),
        None => "N/A".to_string(),
    }
}

fn read_temp_celsius(name: &str) -> Option<u32> {
    // hwmon nodes under /sys/class/block/<name>/device/hwmon/
    let hwmon_base = format!("/sys/class/block/{name}/device/hwmon");
    if let Ok(entries) = fs::read_dir(&hwmon_base) {
        for entry in entries.flatten() {
            for i in 1..=8u32 {
                let temp_path = entry.path().join(format!("temp{i}_input"));
                if let Ok(val) = read_sys_u64(temp_path.to_str().unwrap_or("")) {
                    return Some((val / 1000) as u32);
                }
            }
        }
    }
    // NVMe thermal via /sys/class/nvme/<dev>/hwmon
    let nvme_hwmon = format!("/sys/class/nvme/{name}/hwmon");
    if let Ok(entries) = fs::read_dir(&nvme_hwmon) {
        for entry in entries.flatten() {
            let p = entry.path().join("temp1_input");
            if let Ok(val) = read_sys_u64(p.to_str().unwrap_or("")) {
                return Some((val / 1000) as u32);
            }
        }
    }
    None
}

fn smart_passed(name: &str) -> bool {
    // Try running smartctl if available
    if let Ok(which) = which::which("smartctl") {
        let out = std::process::Command::new(which)
            .args(["-H", &format!("/dev/{name}")])
            .output();
        if let Ok(o) = out {
            let text = String::from_utf8_lossy(&o.stdout);
            return text.contains("PASSED") || text.contains("OK");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_device_for_nvme_partition() {
        assert_eq!(root_device("nvme0n1p1"), "nvme0n1");
        assert_eq!(root_device("nvme1n1p3"), "nvme1n1");
    }

    #[test]
    fn root_device_for_sata() {
        assert_eq!(root_device("sda1"), "sda");
        assert_eq!(root_device("sdb12"), "sdb");
    }

    #[test]
    fn root_device_for_disk_itself() {
        assert_eq!(root_device("sda"), "sda");
        assert_eq!(root_device("nvme0n1"), "nvme0n1");
    }

    #[test]
    fn detect_transport_nvme() {
        assert_eq!(detect_transport("nvme0n1"), "NVMe");
    }

    #[test]
    fn detect_transport_mmc() {
        assert_eq!(detect_transport("mmcblk0"), "MMC");
    }

    #[test]
    fn detect_transport_virtio() {
        assert_eq!(detect_transport("vda"), "VirtIO");
    }

    #[test]
    fn detect_transport_unknown() {
        assert_eq!(detect_transport("xvda"), "Unknown");
    }

    #[test]
    fn disk_usage_percent_range() {
        // /tmp always exists and has some usage
        if let Some(pct) = disk_usage_percent("/tmp") {
            assert!(pct <= 100, "usage percent should be 0-100, got {pct}");
        }
    }

    #[test]
    fn read_mounts_parses_proc_mounts() {
        let mounts = read_mounts().expect("Should be able to read /proc/mounts on Linux");
        // There must be at least one mount (rootfs)
        assert!(
            !mounts.is_empty(),
            "/proc/mounts should have at least one entry"
        );
    }
}
