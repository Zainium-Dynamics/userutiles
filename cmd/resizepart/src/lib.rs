//! user resizepart — tell the kernel a partition was resized.
use std::path::Path;

use usercore::blkpg::{self, BLKPG_RESIZE_PARTITION};
use usercore::ptable::{self, SECTOR_SIZE};
use usercore::Ui;

fn print_help() {
    print!(
        "Usage: resizepart DEVICE PARTITION LENGTH\n\
 Tell the kernel that PARTITION (number) on DEVICE is now LENGTH\n\
 512-byte sectors long (its start position is read from DEVICE's\n\
 current partition table and left unchanged).\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `resizepart` utility. Parses `std::env::args()`
/// for `DEVICE PARTITION LENGTH` (sectors), reads `PARTITION`'s current
/// start position from `DEVICE`'s table, and tells the kernel about the
/// new length via the `BLKPG` ioctl.
///
/// Returns 0 on success, 1 on a usage, read, or ioctl error.
pub fn run() -> i32 {
    let ui = Ui::new("resizepart");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("resizepart (user_utils) 0.1.0");
        return 0;
    }
    let [device, pno, length] = args.as_slice() else {
        ui.err("usage: resizepart DEVICE PARTITION LENGTH");
        return 1;
    };
    let (Ok(pno), Ok(length)) = (pno.parse::<u32>(), length.parse::<u64>()) else {
        ui.err("PARTITION and LENGTH must be numbers");
        return 1;
    };

    let path = Path::new(device);
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
    let Some(partition) = table.partitions.iter().find(|p| p.number == pno) else {
        ui.err(&format!(
            "{device}: no partition #{pno} in the current table"
        ));
        return 1;
    };

    match blkpg::notify(
        path,
        BLKPG_RESIZE_PARTITION,
        pno,
        partition.start_lba * SECTOR_SIZE,
        length * SECTOR_SIZE,
    ) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!("{device}: partition #{pno}: {e}"));
            1
        }
    }
}
