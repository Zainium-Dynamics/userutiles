use colored::*;

/// Print the main header bar for a command
pub fn print_header(title: &str) {
    println!();
    println!("  {}", title.bold().cyan());
    println!("  {}", "─".repeat(title.len() + 2).truecolor(50, 80, 110));
}

/// Print a key-value row
pub fn print_row(label: &str, value: &str) {
    println!(
        "  {:<18} {}",
        format!("{label} :").truecolor(100, 220, 200),
        value
    );
}

/// ✓ success message
pub fn print_success(msg: &str) {
    println!("  {} {}", "✓".bright_green().bold(), msg.bright_green());
}

/// ⚠ warning message
pub fn print_warning(msg: &str) {
    println!(
        "  {} {}",
        "⚠".bright_yellow().bold(),
        msg.bright_yellow()
    );
}

/// ✖ error message
pub fn print_error(msg: &str) {
    println!("  {} {}", "✖".bright_red().bold(), msg.bright_red());
}

/// Shown when `drive` is run with no subcommand
pub fn print_info() {
    println!();
    println!(
        "  {} — {}",
        "drive".bold().bright_cyan(),
        "ZainiumOS Advanced Storage Manager".truecolor(160, 220, 255)
    );
    println!();
    println!(
        "  {} {}",
        "Usage:".truecolor(100, 220, 200),
        "drive [COMMAND] [OPTIONS]".bright_white()
    );
    println!();
    println!("  {}", "Commands:".bold().cyan());

    let cmds = [
        ("list", "Show all connected storage devices"),
        (
            "info <DEVICE>",
            "Detailed information about a specific drive",
        ),
        ("mount <DEVICE>", "Smart mount with filesystem detection"),
        ("umount <DEVICE>", "Safely unmount a device"),
        ("format <DEVICE>", "Format a partition"),
        ("health", "SMART health & temperature report"),
        ("snapshot", "Btrfs snapshot management"),
        ("clone <SRC> <DST>", "Clone a disk or partition"),
        ("repair <DEVICE>", "Filesystem repair tools"),
    ];

    for (cmd, desc) in &cmds {
        println!(
            "  {:<28} {}",
            cmd.bright_blue(),
            desc.truecolor(180, 180, 180)
        );
    }

    println!();
    println!("  {}", "Options:".bold().cyan());
    println!(
        "  {:<28} {}",
        "-h, --help".bright_blue(),
        "Print help".truecolor(180, 180, 180)
    );
    println!(
        "  {:<28} {}",
        "-V, --version".bright_blue(),
        "Print version".truecolor(180, 180, 180)
    );
    println!(
        "  {:<28} {}",
        "-v, --verbose".bright_blue(),
        "Verbose output".truecolor(180, 180, 180)
    );
    println!(
        "  {:<28} {}",
        "--toml".bright_blue(),
        "TOML machine output".truecolor(180, 180, 180)
    );

    println!();
    println!("  {}", "Examples:".bold().cyan());
    let examples = [
        "drive list",
        "drive info nvme0n1",
        "drive mount /dev/sdb1",
        "drive format /dev/sdb1 --fs btrfs --label DATA",
        "drive health",
        "drive snapshot create --volume /",
        "drive clone /dev/nvme0n1 /dev/sdb --verify",
        "drive repair /dev/sda1",
    ];
    for ex in &examples {
        println!("  {} {}", "$".truecolor(80, 80, 100), ex.bright_blue());
    }

    println!();
}
