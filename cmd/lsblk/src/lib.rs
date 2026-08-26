//! user lsblk — list block devices as a tree.
//!
//! Reads `/sys/block` directly (major:minor, size, removable, read-only)
//! rather than `libblkid`/`libmount`; filesystem type/label/UUID come
//! from `usercore::blkprobe` and mountpoints from `/proc/self/mounts`.
use std::fs;
use std::path::Path;

use usercore::Ui;

struct Device {
    name: String,
    devnode: String,
    kind: &'static str,
    maj_min: String,
    removable: bool,
    readonly: bool,
    size: u64,
    children: Vec<Device>,
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_flag(path: &Path) -> bool {
    read_trimmed(path).as_deref() == Some("1")
}

fn read_sectors(path: &Path) -> u64 {
    read_trimmed(path)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn device_kind(name: &str, is_partition: bool) -> &'static str {
    if is_partition {
        "part"
    } else if name.starts_with("loop") {
        "loop"
    } else if name.starts_with("zram") {
        "disk"
    } else if name.starts_with("dm-") {
        "lvm"
    } else if name.starts_with("sr") {
        "rom"
    } else {
        "disk"
    }
}

fn build_device(sys_path: &Path, name: &str, is_partition: bool) -> Option<Device> {
    let maj_min = read_trimmed(&sys_path.join("dev"))?;
    let size_sectors = read_sectors(&sys_path.join("size"));
    let removable = read_flag(&sys_path.join("removable"));
    let readonly = read_flag(&sys_path.join("ro"));

    let mut children = Vec::new();
    if !is_partition {
        if let Ok(entries) = fs::read_dir(sys_path) {
            let mut names: Vec<String> = entries
                .flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    if n.starts_with(name) && e.path().join("partition").is_file() {
                        Some(n)
                    } else {
                        None
                    }
                })
                .collect();
            names.sort();
            for child_name in names {
                if let Some(child) = build_device(&sys_path.join(&child_name), &child_name, true) {
                    children.push(child);
                }
            }
        }
    }

    Some(Device {
        name: name.to_string(),
        devnode: format!("/dev/{name}"),
        kind: device_kind(name, is_partition),
        maj_min,
        removable,
        readonly,
        size: size_sectors.saturating_mul(512),
        children,
    })
}

fn list_devices() -> Vec<Device> {
    let mut devices = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return devices;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let sys_path = Path::new("/sys/block").join(&name);
        if let Some(d) = build_device(&sys_path, &name, false) {
            devices.push(d);
        }
    }
    devices
}

/// Binary (1024-based) human size, matching `lsblk`'s default `SIZE`
/// column formatting.
fn human_size(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n}B")
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

fn mountpoints_for(devnode: &str, mounts: &[(String, String)]) -> String {
    mounts
        .iter()
        .filter(|(src, _)| src == devnode)
        .map(|(_, mp)| mp.clone())
        .collect::<Vec<_>>()
        .join(",")
}

fn read_mounts() -> Vec<(String, String)> {
    fs::read_to_string("/proc/self/mounts")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some((f.next()?.to_string(), f.next()?.to_string()))
        })
        .collect()
}

fn print_default(devices: &[Device], mounts: &[(String, String)]) {
    println!("NAME        MAJ:MIN RM   SIZE RO TYPE MOUNTPOINTS");
    for d in devices {
        print_row_default(&d.name, d, mounts);
        for (i, child) in d.children.iter().enumerate() {
            let branch = if i == d.children.len() - 1 {
                "└─"
            } else {
                "├─"
            };
            print_row_default(&format!("{branch}{}", child.name), child, mounts);
        }
    }
}

fn print_row_default(name_col: &str, d: &Device, mounts: &[(String, String)]) {
    println!(
        "{:<11} {:<7} {:<2}  {:>5} {:<2} {:<4} {}",
        name_col,
        d.maj_min,
        d.removable as u8,
        human_size(d.size),
        d.readonly as u8,
        d.kind,
        mountpoints_for(&d.devnode, mounts),
    );
}

fn print_fs(devices: &[Device], mounts: &[(String, String)]) {
    println!(
        "NAME        FSTYPE      LABEL        UUID                                 MOUNTPOINTS"
    );
    for d in devices {
        print_device_fs(d, mounts);
        for c in &d.children {
            print_device_fs(c, mounts);
        }
    }
}

fn print_device_fs(d: &Device, mounts: &[(String, String)]) {
    let probe = usercore::blkprobe::probe_path(Path::new(&d.devnode))
        .ok()
        .flatten();
    let fstype = probe.as_ref().map(|p| p.fstype.as_str()).unwrap_or("");
    let label = probe
        .as_ref()
        .and_then(|p| p.label.as_deref())
        .unwrap_or("");
    let uuid = probe.as_ref().and_then(|p| p.uuid.as_deref()).unwrap_or("");
    println!(
        "{:<11} {:<11} {:<12} {:<36} {}",
        d.name,
        fstype,
        label,
        uuid,
        mountpoints_for(&d.devnode, mounts),
    );
}

fn print_help() {
    print!(
        "Usage: lsblk [-f]\n\
 List block devices as a tree, from /sys/block.\n\
 -f, --fs show filesystem TYPE/LABEL/UUID instead of size/type\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `lsblk` utility. Lists every block device under
/// `/sys/block` (disks, then their partitions as a tree) with
/// major:minor, size, removable/read-only flags, type, and mountpoints;
/// `-f` instead shows filesystem type/label/UUID.
///
/// Returns 0 on success, 1 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("lsblk");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut fs_mode = false;
    for arg in &args {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("lsblk (user_utils) 0.1.0");
                return 0;
            }
            "-f" | "--fs" => fs_mode = true,
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            _ => {
                ui.err("device filtering by name is not supported");
                return 1;
            }
        }
    }

    let devices = list_devices();
    let mounts = read_mounts();
    if fs_mode {
        print_fs(&devices, &mounts);
    } else {
        print_default(&devices, &mounts);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_formats_binary_units() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(1023), "1023B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1024 * 1024 * 5), "5.0M");
    }

    #[test]
    fn device_kind_classifies_common_names() {
        assert_eq!(device_kind("sda", false), "disk");
        assert_eq!(device_kind("sda1", true), "part");
        assert_eq!(device_kind("loop0", false), "loop");
        assert_eq!(device_kind("zram0", false), "disk");
        assert_eq!(device_kind("dm-0", false), "lvm");
        assert_eq!(device_kind("sr0", false), "rom");
    }

    #[test]
    fn mountpoints_for_joins_multiple_matches() {
        let mounts = vec![
            ("/dev/sda1".to_string(), "/".to_string()),
            ("/dev/sda1".to_string(), "/mnt/dup".to_string()),
            ("/dev/sda2".to_string(), "/home".to_string()),
        ];
        assert_eq!(mountpoints_for("/dev/sda1", &mounts), "/,/mnt/dup");
        assert_eq!(mountpoints_for("/dev/sda2", &mounts), "/home");
        assert_eq!(mountpoints_for("/dev/sda3", &mounts), "");
    }

    #[test]
    fn list_devices_does_not_panic() {
        // Real assertion: this must run to completion on a real
        // /sys/block without panicking, whatever it finds there.
        let devices = list_devices();
        assert!(devices.len() < 10_000);
    }
}
