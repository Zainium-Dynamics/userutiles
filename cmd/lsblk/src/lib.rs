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

/// (device node, mount point) for every mount; a source given as a symlink
/// (`/dev/disk/by-uuid/...`, as Zainium mounts its root) is resolved to the node.
fn read_mounts() -> Vec<(String, String)> {
    fs::read_to_string("/proc/self/mounts")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let src = f.next()?.to_string();
            let mp = f.next()?.replace("\\040", " ");
            let src = if src.starts_with("/dev/") {
                fs::canonicalize(&src).map(|p| p.to_string_lossy().into_owned()).unwrap_or(src)
            } else {
                src
            };
            Some((src, mp))
        })
        .collect()
}

/// Every column `-o` understands: (name, right-aligned).
const COLUMNS: &[(&str, bool)] = &[
    ("NAME", false),
    ("KNAME", false),
    ("PATH", false),
    ("MAJ:MIN", false),
    ("RM", true),
    ("SIZE", true),
    ("RO", true),
    ("TYPE", false),
    ("FSTYPE", false),
    ("LABEL", false),
    ("UUID", false),
    ("MODEL", false),
    ("SERIAL", false),
    ("TRAN", false),
    ("MOUNTPOINT", false),
    ("MOUNTPOINTS", false),
];

struct Opts {
    bytes: bool,
    paths: bool,
    tree: bool,
    raw: bool,
}

fn sys_attr(name: &str, attr: &str) -> String {
    let base = disk_of(name);
    read_trimmed(&Path::new("/sys/block").join(base).join("device").join(attr)).unwrap_or_default()
}

/// The whole disk a partition belongs to (nvme0n1p2 -> nvme0n1, sda1 -> sda).
fn disk_of(name: &str) -> String {
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return name.to_string();
    };
    entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|d| name.starts_with(d.as_str()))
        .max_by_key(|d| d.len())
        .unwrap_or_else(|| name.to_string())
}

/// How the disk is attached, from where it sits in /sys.
fn transport(name: &str) -> String {
    let disk = disk_of(name);
    if disk.starts_with("nvme") {
        return "nvme".into();
    }
    let path = fs::canonicalize(Path::new("/sys/block").join(&disk)).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    if path.contains("/usb") {
        "usb"
    } else if path.contains("/ata") {
        "sata"
    } else if path.contains("/virtio") {
        "virtio"
    } else if disk.starts_with("mmcblk") {
        "mmc"
    } else {
        ""
    }
    .into()
}

fn cell(d: &Device, col: &str, mounts: &[(String, String)], o: &Opts, probe: &mut Option<Option<usercore::blkprobe::Probe>>) -> String {
    let mut fs_info = || probe.get_or_insert_with(|| usercore::blkprobe::probe_path(Path::new(&d.devnode)).ok().flatten()).clone();
    match col {
        "NAME" => if o.paths { d.devnode.clone() } else { d.name.clone() },
        "KNAME" => d.name.clone(),
        "PATH" => d.devnode.clone(),
        "MAJ:MIN" => d.maj_min.clone(),
        "RM" => (d.removable as u8).to_string(),
        "SIZE" => if o.bytes { d.size.to_string() } else { human_size(d.size) },
        "RO" => (d.readonly as u8).to_string(),
        "TYPE" => d.kind.to_string(),
        "FSTYPE" => fs_info().map(|p| p.fstype).unwrap_or_default(),
        "LABEL" => fs_info().and_then(|p| p.label).unwrap_or_default(),
        "UUID" => fs_info().and_then(|p| p.uuid).unwrap_or_default(),
        "MODEL" => if d.kind == "part" { String::new() } else { sys_attr(&d.name, "model") },
        "SERIAL" => if d.kind == "part" { String::new() } else { sys_attr(&d.name, "serial") },
        "TRAN" => if d.kind == "part" { String::new() } else { transport(&d.name) },
        "MOUNTPOINT" => mountpoints_for(&d.devnode, mounts).split(',').next().unwrap_or("").to_string(),
        "MOUNTPOINTS" => mountpoints_for(&d.devnode, mounts),
        _ => String::new(),
    }
}

fn print_table(devices: &[Device], cols: &[&str], headings: bool, nodeps: bool, mounts: &[(String, String)], o: &Opts) {
    // (tree prefix, device) in display order
    let mut rows: Vec<(String, &Device)> = Vec::new();
    for d in devices {
        rows.push((String::new(), d));
        if !nodeps {
            for (i, c) in d.children.iter().enumerate() {
                let branch = if !o.tree { "" } else if i == d.children.len() - 1 { "└─" } else { "├─" };
                rows.push((branch.to_string(), c));
            }
        }
    }
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|(prefix, d)| {
            let mut probe = None;
            cols.iter()
                .map(|c| {
                    let v = cell(d, c, mounts, o, &mut probe);
                    if *c == "NAME" { format!("{prefix}{v}") } else { v }
                })
                .collect()
        })
        .collect();
    if o.raw {
        if headings {
            println!("{}", cols.join(" "));
        }
        for r in &table {
            println!("{}", r.iter().map(|v| v.replace(' ', "\\x20")).collect::<Vec<_>>().join(" "));
        }
        return;
    }
    let mut widths: Vec<usize> = cols.iter().map(|c| if headings { c.len() } else { 0 }).collect();
    for r in &table {
        for (w, v) in widths.iter_mut().zip(r) {
            *w = (*w).max(v.chars().count());
        }
    }
    let right = |c: &str| COLUMNS.iter().any(|(n, r)| *n == c && *r);
    let line = |vals: Vec<String>| -> String {
        let mut s = String::new();
        for (i, (v, c)) in vals.iter().zip(cols).enumerate() {
            if i > 0 {
                s.push(' ');
            }
            let pad = widths[i].saturating_sub(v.chars().count());
            if right(c) {
                s.push_str(&" ".repeat(pad));
                s.push_str(v);
            } else {
                s.push_str(v);
                s.push_str(&" ".repeat(pad));
            }
        }
        s.trim_end().to_string()
    };
    if headings {
        println!("{}", line(cols.iter().map(|c| c.to_string()).collect()));
    }
    for r in table {
        println!("{}", line(r));
    }
}

fn print_help() {
    print!(
        "Usage: lsblk [options] [DEVICE...]\n\
 List block devices as a tree, from /sys/block.\n\
 -d, --nodeps        disks only, no partitions\n\
 -f, --fs            filesystem columns: NAME FSTYPE LABEL UUID MOUNTPOINTS\n\
 -o, --output LIST   columns, comma separated (see below)\n\
 -n, --noheadings    no header line\n\
 -p, --paths         full device paths in NAME\n\
 -b, --bytes         SIZE in bytes\n\
 -l, --list          list instead of tree\n\
 -r, --raw           raw output: no alignment, no tree\n\
 --help, --version\n\
 Columns: NAME KNAME PATH MAJ:MIN RM SIZE RO TYPE FSTYPE LABEL UUID MODEL SERIAL TRAN MOUNTPOINT MOUNTPOINTS\n"
    );
}

/// Entry point for the `lsblk` utility. Lists every block device under
/// `/sys/block` (disks, then their partitions as a tree); `-o` picks the
/// columns, `-d` drops partitions, `-n` the header, `-p` shows full paths,
/// `-b` sizes in bytes, `-l`/`-r` print a list, `-f` the filesystem columns.
/// Device arguments limit the output to those devices.
///
/// Returns 0 on success, 1 on a usage error.
pub fn run() -> i32 {
    // `lsblk | head` ends quietly instead of panicking on the closed pipe
    // SAFETY: restoring the default action of a signal has no other effect.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let ui = Ui::new("lsblk");
    let args = usercore::args::expand_short(std::env::args().skip(1), "dfnpblrahV", "o");

    let mut cols: Vec<String> = ["NAME", "MAJ:MIN", "RM", "SIZE", "RO", "TYPE", "MOUNTPOINTS"].map(String::from).to_vec();
    let mut headings = true;
    let mut nodeps = false;
    let mut opts = Opts { bytes: false, paths: false, tree: true, raw: false };
    let mut wanted: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" | "-V" => {
                println!("lsblk (user_utils) 0.2.0");
                return 0;
            }
            "-f" | "--fs" => cols = ["NAME", "FSTYPE", "LABEL", "UUID", "MOUNTPOINTS"].map(String::from).to_vec(),
            "-d" | "--nodeps" => nodeps = true,
            "-n" | "--noheadings" => headings = false,
            "-p" | "--paths" => opts.paths = true,
            "-b" | "--bytes" => opts.bytes = true,
            "-l" | "--list" => opts.tree = false,
            "-r" | "--raw" => {
                opts.raw = true;
                opts.tree = false;
            }
            "-a" | "--all" => {}
            _ if a == "-o" || a == "--output" || a.starts_with("--output=") => {
                let v = match a.split_once('=') {
                    Some((_, v)) => Some(v.to_string()),
                    None => {
                        i += 1;
                        args.get(i).cloned()
                    }
                };
                let Some(v) = v else {
                    ui.err("option requires an argument -- 'o'");
                    return 1;
                };
                // -o +COL adds to the default columns
                let (base, list) = match v.strip_prefix('+') {
                    Some(rest) => (cols.clone(), rest.to_string()),
                    None => (Vec::new(), v),
                };
                let mut picked = base;
                for c in list.split(',').map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()) {
                    if !COLUMNS.iter().any(|(n, _)| *n == c) {
                        ui.err(&format!("unknown column: {c}"));
                        return 1;
                    }
                    picked.push(c);
                }
                cols = picked;
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            dev => wanted.push(dev.trim_start_matches("/dev/").to_string()),
        }
        i += 1;
    }

    let mut devices = list_devices();
    if !wanted.is_empty() {
        let mut picked = Vec::new();
        for mut d in devices {
            if wanted.contains(&d.name) {
                picked.push(d);
            } else {
                d.children.retain(|c| wanted.contains(&c.name));
                for c in d.children {
                    picked.push(c);
                }
            }
        }
        if picked.is_empty() {
            ui.err(&format!("{}: not a block device", wanted.join(", ")));
            return 1;
        }
        devices = picked;
    }
    let mounts = read_mounts();
    let col_refs: Vec<&str> = cols.iter().map(String::as_str).collect();
    print_table(&devices, &col_refs, headings, nodeps, &mounts, &opts);
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
