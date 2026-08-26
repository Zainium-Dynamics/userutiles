//! user findfs — find a filesystem by LABEL or UUID.
//! Thin wrapper over `user_blkid`'s shared lookup logic.
use usercore::Ui;

fn print_help() {
    print!(
        "Usage: findfs LABEL=<label>\n\
 findfs UUID=<uuid>\n\
 Find a block device by its filesystem label or UUID.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `findfs` utility. Parses `std::env::args()` for a
/// single `LABEL=name` or `UUID=id` operand and prints the matching
/// device path.
///
/// Returns 0 if found, 1 on a usage error, 2 if nothing matched.
pub fn run() -> i32 {
    let ui = Ui::new("findfs");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("findfs (user_utils) 0.1.0");
        return 0;
    }
    let Some(spec) = args.first() else {
        ui.err("usage: findfs LABEL=<label>|UUID=<uuid>");
        return 1;
    };

    let (field, value) = match spec.split_once('=') {
        Some(("LABEL", v)) => ("LABEL", v),
        Some(("UUID", v)) => ("UUID", v),
        _ => {
            ui.err(&format!("invalid specification `{spec}'"));
            return 1;
        }
    };

    match user_blkid::find_by_field(field, value) {
        Some(dev) => {
            println!("{}", dev.display());
            0
        }
        None => 2,
    }
}
