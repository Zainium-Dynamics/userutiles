//! user fdisk — list partition tables.
//!
//! Only the read-only `-l` listing is implemented — real `fdisk`'s
//! interactive edit REPL is a large stateful undertaking disproportionate
//! to what's left to build here. For scripted/non-interactive editing,
//! use `sfdisk` instead, which this workspace does implement.
use std::path::{Path, PathBuf};

use usercore::ptable::{self, Label, PartitionTable};
use usercore::Ui;

/// Human-readable name for a handful of common MBR type bytes / GPT type
/// GUIDs — real `fdisk`'s own type-name table is much larger; this
/// covers the everyday cases.
fn type_name(label: &Label, part_type: &str) -> &'static str {
    match label {
        Label::Dos => match part_type {
            "83" => "Linux",
            "82" => "Linux swap / Solaris",
            "ef" => "EFI (FAT-12/16/32)",
            "07" => "HPFS/NTFS/exFAT",
            "0c" | "0b" => "W95 FAT32",
            "05" | "0f" => "Extended",
            _ => "Unknown",
        },
        Label::Gpt => match part_type {
            "0FC63DAF-8483-4772-8E79-3D69D8477DE4" => "Linux filesystem",
            "0657FD6D-A4AB-43C4-84E5-0933C84B4F4F" => "Linux swap",
            "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => "EFI System",
            "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => "Microsoft reserved",
            "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => "Microsoft basic data",
            _ => "Unknown",
        },
    }
}

fn human_size(sectors: u64) -> String {
    let bytes = sectors * ptable::SECTOR_SIZE;
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes}B")
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

fn print_table(device: &Path, table: &PartitionTable) {
    let label_str = match table.label {
        Label::Dos => "dos",
        Label::Gpt => "gpt",
    };
    println!("Disk {}:", device.display());
    println!("Disklabel type: {label_str}");
    println!();
    println!("Device       Boot     Start       End  Sectors  Size Type");
    for p in &table.partitions {
        let end = p.start_lba + p.size_lba.saturating_sub(1);
        let boot = if p.bootable { "*" } else { " " };
        println!(
            "{}{:<2}   {:<4} {:>9} {:>9} {:>8} {:>5} {}",
            device.display(),
            p.number,
            boot,
            p.start_lba,
            end,
            p.size_lba,
            human_size(p.size_lba),
            type_name(&table.label, &p.part_type),
        );
    }
}

/// Every top-level (non-partition) device name under `/sys/block`, as
/// full `/dev/<name>` paths — the same set `fdisk -l` (no args) scans.
fn list_whole_disks() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|n| PathBuf::from(format!("/dev/{n}")))
        .collect()
}

fn print_help() {
    print!(
        "Usage: fdisk -l [DEVICE...]\n\
 List the partition table of each DEVICE (or every disk found under\n\
 /sys/block, with none given).\n\
 Interactive partition editing is not implemented here — use sfdisk for\n\
 scripted editing.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `fdisk` utility. Parses `std::env::args()` for
/// `-l [DEVICE...]` and prints each device's MBR/GPT partition table.
///
/// Returns 0 on success (even if a device had no recognizable table —
/// matching real `fdisk -l`, which reports and continues), 1 on a usage
/// error or unreadable device.
pub fn run() -> i32 {
    let ui = Ui::new("fdisk");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut list_mode = false;
    let mut devices: Vec<String> = Vec::new();
    for arg in &args {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("fdisk (user_utils) 0.1.0");
                return 0;
            }
            "-l" | "--list" => list_mode = true,
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => devices.push(other.to_string()),
        }
    }

    if !list_mode {
        ui.err("interactive editing is not implemented — use `fdisk -l` or `sfdisk`");
        return 1;
    }

    let targets: Vec<PathBuf> = if devices.is_empty() {
        list_whole_disks()
    } else {
        devices.iter().map(PathBuf::from).collect()
    };

    let mut status = 0;
    for dev in &targets {
        match ptable::read_table(dev) {
            Ok(Some(table)) => print_table(dev, &table),
            Ok(None) => {}
            Err(e) => {
                if !devices.is_empty() {
                    ui.err(&format!("cannot open {}: {e}", dev.display()));
                    status = 1;
                }
            }
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use usercore::ptable::Partition;

    #[test]
    fn type_name_covers_common_ids() {
        assert_eq!(type_name(&Label::Dos, "83"), "Linux");
        assert_eq!(type_name(&Label::Dos, "82"), "Linux swap / Solaris");
        assert_eq!(
            type_name(&Label::Gpt, "0FC63DAF-8483-4772-8E79-3D69D8477DE4"),
            "Linux filesystem"
        );
        assert_eq!(type_name(&Label::Dos, "ff"), "Unknown");
    }

    #[test]
    fn human_size_reads_as_expected() {
        assert_eq!(human_size(2048), "1.0M");
    }

    #[test]
    fn print_table_does_not_panic_with_no_partitions() {
        let table = PartitionTable {
            label: Label::Dos,
            partitions: vec![],
        };
        print_table(Path::new("/dev/null"), &table);
    }

    #[test]
    fn print_table_does_not_panic_with_a_partition() {
        let table = PartitionTable {
            label: Label::Gpt,
            partitions: vec![Partition {
                number: 1,
                start_lba: 2048,
                size_lba: 1000,
                part_type: "0FC63DAF-8483-4772-8E79-3D69D8477DE4".to_string(),
                bootable: false,
                name: Some("root".to_string()),
            }],
        };
        print_table(Path::new("/dev/sda"), &table);
    }
}
