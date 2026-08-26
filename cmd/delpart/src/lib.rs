//! user delpart — tell the kernel a partition was removed.
use std::path::Path;

use usercore::blkpg::{self, BLKPG_DEL_PARTITION};
use usercore::Ui;

fn print_help() {
    print!(
        "Usage: delpart DEVICE PARTITION\n\
 Tell the kernel that PARTITION (number) no longer exists on DEVICE.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `delpart` utility. Parses `std::env::args()` for
/// `DEVICE PARTITION` and tells the kernel to forget it via the `BLKPG`
/// ioctl.
///
/// Returns 0 on success, 1 on a usage or ioctl error.
pub fn run() -> i32 {
    let ui = Ui::new("delpart");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("delpart (user_utils) 0.1.0");
        return 0;
    }
    let [device, pno] = args.as_slice() else {
        ui.err("usage: delpart DEVICE PARTITION");
        return 1;
    };
    let Ok(pno) = pno.parse::<u32>() else {
        ui.err("PARTITION must be a number");
        return 1;
    };

    match blkpg::notify(Path::new(device), BLKPG_DEL_PARTITION, pno, 0, 0) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!("{device}: partition #{pno}: {e}"));
            1
        }
    }
}
