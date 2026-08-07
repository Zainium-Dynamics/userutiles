/// Generates drive.1 man page into the current directory (or OUT_DIR if set).
/// Run with: cargo run --bin generate-manpage --features manpage
///
/// Then install: sudo cp drive.1 /usr/local/share/man/man1/
/// sudo mandb

#[cfg(feature = "manpage")]
fn main() -> std::io::Result<()> {
    use clap::CommandFactory;
    use clap_mangen::Man;
    use std::fs;
    use std::path::PathBuf;

    // Re-use the same Cli definition
    #[path = "../cli.rs"]
    mod cli;
    // We need just the struct; pull in required modules as stubs
    // The actual command factory is built via clap derive.

    let out_dir = std::env::var("OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // Build the clap App from our Cli struct
    let app = clap::command!()
        .name("drive")
        .version(env!("CARGO_PKG_VERSION"))
        .author("ZainiumOS")
        .about("ZainiumOS Advanced Storage Manager")
        .long_about(
            "drive is a production-grade Linux storage management tool.\n\
 It wraps blkid, mount, mkfs, smartctl, btrfs, e2fsck, ddrescue\n\
 and presents them through a consistent, color-coded CLI.",
        )
        // Subcommands are described inline so the man page is self-contained
        .subcommand(clap::Command::new("list").about("Show all connected storage devices"))
        .subcommand(
            clap::Command::new("info")
                .about("Detailed information about a specific drive")
                .arg(
                    clap::Arg::new("device")
                        .required(true)
                        .help("Device name (e.g. nvme0n1)"),
                ),
        )
        .subcommand(
            clap::Command::new("mount")
                .about("Smart mount with filesystem detection and mountpoint suggestion")
                .arg(
                    clap::Arg::new("device")
                        .required(true)
                        .help("Device path (e.g. /dev/sdb1)"),
                )
                .arg(
                    clap::Arg::new("mountpoint")
                        .short('m')
                        .long("mountpoint")
                        .help("Custom mountpoint (auto-detected if omitted)"),
                ),
        )
        .subcommand(
            clap::Command::new("umount")
                .about("Safely unmount a device, flushing all buffers")
                .arg(clap::Arg::new("device").required(true).help("Device path")),
        )
        .subcommand(
            clap::Command::new("format")
                .about("Format a partition with the chosen filesystem")
                .arg(clap::Arg::new("device").required(true))
                .arg(
                    clap::Arg::new("fs")
                        .short('f')
                        .long("fs")
                        .default_value("ext4")
                        .help("Filesystem: ext4, btrfs, xfs, exfat, vfat, ntfs, f2fs"),
                )
                .arg(
                    clap::Arg::new("label")
                        .short('l')
                        .long("label")
                        .help("Volume label"),
                )
                .arg(
                    clap::Arg::new("yes")
                        .short('y')
                        .long("yes")
                        .action(clap::ArgAction::SetTrue)
                        .help("Skip confirmation"),
                ),
        )
        .subcommand(
            clap::Command::new("health")
                .about("SMART health, temperature and wear report")
                .arg(
                    clap::Arg::new("device")
                        .help("Specific device to check (all if omitted)")
                        .required(false),
                ),
        )
        .subcommand(
            clap::Command::new("snapshot")
                .about("Btrfs snapshot management")
                .subcommand(
                    clap::Command::new("create")
                        .about("Create a new read-only snapshot")
                        .arg(
                            clap::Arg::new("volume")
                                .short('v')
                                .long("volume")
                                .default_value("/")
                                .help("Subvolume path"),
                        )
                        .arg(
                            clap::Arg::new("name")
                                .short('n')
                                .long("name")
                                .help("Custom name (timestamp used if omitted)"),
                        ),
                )
                .subcommand(clap::Command::new("list").about("List existing snapshots"))
                .subcommand(
                    clap::Command::new("delete")
                        .about("Delete a snapshot by name")
                        .arg(clap::Arg::new("name").required(true)),
                )
                .subcommand(
                    clap::Command::new("restore")
                        .about("Restore a snapshot (creates writable subvolume)")
                        .arg(clap::Arg::new("name").required(true)),
                ),
        )
        .subcommand(
            clap::Command::new("clone")
                .about("Clone a disk or partition (ddrescue or dd)")
                .arg(
                    clap::Arg::new("source")
                        .required(true)
                        .help("Source device path"),
                )
                .arg(
                    clap::Arg::new("target")
                        .required(true)
                        .help("Target device path"),
                )
                .arg(
                    clap::Arg::new("verify")
                        .long("verify")
                        .action(clap::ArgAction::SetTrue)
                        .help("Verify data integrity after clone"),
                ),
        )
        .subcommand(
            clap::Command::new("repair")
                .about("Filesystem repair (e2fsck / btrfs check / xfs_repair / …)")
                .arg(clap::Arg::new("device").required(true))
                .arg(
                    clap::Arg::new("dry-run")
                        .long("dry-run")
                        .action(clap::ArgAction::SetTrue)
                        .help("Only report issues, make no changes"),
                ),
        )
        .subcommand(
            clap::Command::new("benchmark")
                .about("Sequential & random I/O performance benchmark")
                .arg(
                    clap::Arg::new("target")
                        .required(true)
                        .help("Device or file path"),
                )
                .arg(
                    clap::Arg::new("block-size-kib")
                        .short('b')
                        .long("block-size-kib")
                        .default_value("4096")
                        .help("Block size in KiB"),
                )
                .arg(
                    clap::Arg::new("duration-secs")
                        .short('d')
                        .long("duration-secs")
                        .default_value("5")
                        .help("Duration of each test in seconds"),
                ),
        );

    let man = Man::new(app);
    let mut buf = Vec::new();
    man.render(&mut buf)?;

    let out_path = out_dir.join("drive.1");
    fs::write(&out_path, buf)?;
    println!("Man page written to: {}", out_path.display());
    println!("Install with:");
    println!(
        "  sudo cp {} /usr/local/share/man/man1/drive.1",
        out_path.display()
    );
    println!("  sudo mandb");

    Ok(())
}

#[cfg(not(feature = "manpage"))]
fn main() {
    eprintln!("Build with --features manpage to generate the man page.");
    eprintln!("  cargo run --bin generate-manpage --features manpage");
    std::process::exit(1);
}
