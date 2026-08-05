use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::core::{
    benchmark::run_benchmark,
    clone::run_clone,
    device::run_list,
    format::run_format,
    health::run_health,
    mount::{run_mount, run_umount},
    repair::run_repair,
    snapshot::run_snapshot,
};
use crate::ui::display::print_info;

#[derive(Parser)]
#[command(
 name = "drive",
 about = "ZainiumOS Advanced Storage Manager",
 version = env!("CARGO_PKG_VERSION"),
 long_about = None,
 after_help = "Examples:\n drive list\n drive info nvme0n1\n drive mount /dev/sdb1\n drive health\n drive snapshot create --volume /\n drive clone /dev/nvme0n1 /dev/sdb --verify\n drive benchmark /dev/nvme0n1"
)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Machine-readable TOML output (user_utils uses .toml only — never JSON)
    #[arg(long, global = true)]
    pub toml: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show all connected storage devices
    List,

    /// Detailed information about a specific drive
    Info {
        /// Device name (e.g. nvme0n1, sda)
        device: String,
    },

    /// Smart mount with filesystem detection and suggestions
    Mount {
        /// Device path (e.g. /dev/sdb1)
        device: String,

        /// Custom mountpoint (auto-detected if omitted)
        #[arg(short, long)]
        mountpoint: Option<String>,
    },

    /// Safely unmount a device, flushing all buffers
    Umount {
        /// Device path (e.g. /dev/sdb1)
        device: String,
    },

    /// Format a partition with chosen filesystem
    Format {
        /// Device path (e.g. /dev/sdb1)
        device: String,

        /// Filesystem type: ext4, btrfs, xfs, exfat, vfat, ntfs
        #[arg(short = 'f', long = "fs", default_value = "ext4")]
        filesystem: String,

        /// Volume label
        #[arg(short, long)]
        label: Option<String>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// SMART health & temperature report
    Health {
        /// Specific device to check (all if omitted)
        device: Option<String>,
    },

    /// Btrfs snapshot management
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },

    /// Clone a disk or partition
    Clone {
        /// Source device path
        source: String,

        /// Target device path
        target: String,

        /// Verify data integrity after clone
        #[arg(long)]
        verify: bool,
    },

    /// Filesystem repair tools
    Repair {
        /// Device path to repair
        device: String,

        /// Dry-run — only report issues, no changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Sequential & random I/O performance benchmark
    Benchmark {
        /// Device or file path to benchmark
        target: String,

        /// Block size in KiB (default: 4096)
        #[arg(short = 'b', long, default_value = "4096")]
        block_size_kib: u64,

        /// Duration of each test in seconds (default: 5)
        #[arg(short = 'd', long, default_value = "5")]
        duration_secs: u64,
    },
}

#[derive(Subcommand)]
pub enum SnapshotAction {
    /// Create a new snapshot
    Create {
        /// Subvolume path (e.g. /)
        #[arg(long, default_value = "/")]
        volume: String,

        /// Custom snapshot name (timestamp if omitted)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// List existing snapshots
    List {
        /// Subvolume path
        #[arg(short, long, default_value = "/")]
        volume: String,
    },

    /// Delete a snapshot by name
    Delete {
        /// Snapshot name
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Restore a snapshot
    Restore {
        /// Snapshot name
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let verbose = self.verbose;
        let toml = self.toml;

        match self.command {
            None => {
                print_info();
                Ok(())
            }
            Some(Commands::List) => run_list(toml),
            Some(Commands::Info { device }) => crate::core::device::run_info(&device, toml),
            Some(Commands::Mount { device, mountpoint }) => {
                run_mount(&device, mountpoint.as_deref(), verbose)
            }
            Some(Commands::Umount { device }) => run_umount(&device, verbose),
            Some(Commands::Format {
                device,
                filesystem,
                label,
                yes,
            }) => run_format(&device, &filesystem, label.as_deref(), yes),
            Some(Commands::Health { device }) => run_health(device.as_deref(), toml),
            Some(Commands::Snapshot { action }) => run_snapshot(action),
            Some(Commands::Clone {
                source,
                target,
                verify,
            }) => run_clone(&source, &target, verify),
            Some(Commands::Repair { device, dry_run }) => run_repair(&device, dry_run),
            Some(Commands::Benchmark {
                target,
                block_size_kib,
                duration_secs,
            }) => run_benchmark(&target, block_size_kib, duration_secs),
        }
    }
}
