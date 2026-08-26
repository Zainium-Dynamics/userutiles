//! user addpart — tell the kernel a partition was added.
use std::path::Path;

use usercore::blkpg::{self, BLKPG_ADD_PARTITION};
use usercore::ptable::SECTOR_SIZE;
use usercore::Ui;

fn print_help() {
    print!(
        "Usage: addpart DEVICE PARTITION START LENGTH\n\
 Tell the kernel that PARTITION (number) exists on DEVICE, starting at\n\
 START and running for LENGTH — both in 512-byte sectors.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `addpart` utility. Parses `std::env::args()` for
/// `DEVICE PARTITION START LENGTH` (sectors) and tells the kernel about
/// it via the `BLKPG` ioctl.
///
/// Returns 0 on success, 1 on a usage or ioctl error.
pub fn run() -> i32 {
    let ui = Ui::new("addpart");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("addpart (user_utils) 0.1.0");
        return 0;
    }
    let [device, pno, start, length] = args.as_slice() else {
        ui.err("usage: addpart DEVICE PARTITION START LENGTH");
        return 1;
    };
    let (Ok(pno), Ok(start), Ok(length)) = (
        pno.parse::<u32>(),
        start.parse::<u64>(),
        length.parse::<u64>(),
    ) else {
        ui.err("PARTITION, START, and LENGTH must be numbers");
        return 1;
    };

    match blkpg::notify(
        Path::new(device),
        BLKPG_ADD_PARTITION,
        pno,
        start * SECTOR_SIZE,
        length * SECTOR_SIZE,
    ) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!("{device}: partition #{pno}: {e}"));
            1
        }
    }
}
