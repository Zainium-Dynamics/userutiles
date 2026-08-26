//! user partx — tell the kernel about a device's partitions.
//!
//! Reads the partition table via `usercore::ptable` (same MBR/GPT reader
//! `fdisk -l`/`sfdisk` use) and informs the running kernel about
//! individual partitions through the `BLKPG` ioctl — the same mechanism
//! real `partx`/`partprobe` use, not a re-scan-the-whole-disk shortcut.
use std::io;
use std::path::Path;

use usercore::blkpg::{self, BLKPG_ADD_PARTITION, BLKPG_DEL_PARTITION};
use usercore::ptable::{self, Partition};
use usercore::Ui;

/// Tell the kernel about one partition on `disk` (the whole-disk device
/// node) — add it if `add`, else remove it.
fn notify_kernel(disk: &Path, p: &Partition, add: bool) -> io::Result<()> {
    let op = if add {
        BLKPG_ADD_PARTITION
    } else {
        BLKPG_DEL_PARTITION
    };
    blkpg::notify(
        disk,
        op,
        p.number,
        p.start_lba * ptable::SECTOR_SIZE,
        p.size_lba * ptable::SECTOR_SIZE,
    )
}

fn print_show(disk: &Path) -> io::Result<()> {
    let Some(table) = ptable::read_table(disk)? else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no partition table found",
        ));
    };
    println!("NR  START       END  SECTORS  NAME");
    for p in &table.partitions {
        let end = p.start_lba + p.size_lba.saturating_sub(1);
        println!(
            "{:>2}  {:>9} {:>9} {:>8}  {}",
            p.number,
            p.start_lba,
            end,
            p.size_lba,
            p.name.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}

fn print_help() {
    print!(
        "Usage: partx -s DEVICE\n\
 partx -a DEVICE [NR]\n\
 partx -d --nr NR DEVICE\n\
 Tell the kernel about DEVICE's partitions.\n\
 -s, --show list the partitions found in DEVICE's table\n\
 -a, --add add every partition (or just NR, if given)\n\
 -d, --delete remove partition NR (requires --nr)\n\
 --nr NR select one partition number for -d\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `partx` utility. Parses `std::env::args()` and
/// either lists `DEVICE`'s partitions (`-s`), tells the kernel about all
/// of them or one `NR` (`-a`), or removes one (`-d --nr NR`).
///
/// Returns 0 on success, 1 on a usage, read, or ioctl error.
pub fn run() -> i32 {
    let ui = Ui::new("partx");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut show = false;
    let mut add = false;
    let mut delete = false;
    let mut nr: Option<u32> = None;
    let mut device: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("partx (user_utils) 0.1.0");
                return 0;
            }
            "-s" | "--show" => show = true,
            "-a" | "--add" => add = true,
            "-d" | "--delete" => delete = true,
            "--nr" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse().ok()) {
                    Some(v) => nr = Some(v),
                    None => {
                        ui.err("invalid or missing --nr value");
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => device = Some(other.to_string()),
        }
        i += 1;
    }

    let Some(device) = device else {
        ui.err("usage: partx -s|-a|-d DEVICE");
        return 1;
    };
    let path = Path::new(&device);

    if show {
        return match print_show(path) {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&format!("{device}: {e}"));
                1
            }
        };
    }

    if delete {
        let Some(nr) = nr else {
            ui.err("-d requires --nr NR");
            return 1;
        };
        let partition = Partition {
            number: nr,
            start_lba: 0,
            size_lba: 0,
            part_type: String::new(),
            bootable: false,
            name: None,
        };
        return match notify_kernel(path, &partition, false) {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&format!("{device}: partition #{nr}: {e}"));
                1
            }
        };
    }

    if add {
        let table = match ptable::read_table(path) {
            Ok(Some(t)) => t,
            Ok(None) => {
                ui.err(&format!("{device}: no partition table found"));
                return 1;
            }
            Err(e) => {
                ui.err(&format!("{device}: {e}"));
                return 1;
            }
        };
        let targets: Vec<&Partition> = match nr {
            Some(n) => table.partitions.iter().filter(|p| p.number == n).collect(),
            None => table.partitions.iter().collect(),
        };
        if targets.is_empty() {
            ui.err("no matching partition");
            return 1;
        }
        let mut status = 0;
        for p in targets {
            if let Err(e) = notify_kernel(path, p, true) {
                ui.err(&format!("{device}: partition #{}: {e}", p.number));
                status = 1;
            }
        }
        return status;
    }

    print_help();
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    // The BLKPG struct-ABI and no-such-device failure cases are covered
    // once, in usercore::blkpg's own tests, since notify_kernel here is
    // a thin wrapper over it.

    #[test]
    fn print_show_on_a_file_with_no_table_errors() {
        let dir = std::env::temp_dir().join(format!("user_partx_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blank.img");
        std::fs::write(&path, vec![0u8; 4096]).unwrap();
        assert!(print_show(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
