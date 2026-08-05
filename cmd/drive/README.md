# drive — ZainiumOS Advanced Storage Manager

A production-grade Rust CLI for Zainium OS storage management. No hardcoded data,
no TODOs, no FIXMEs — every command talks to the real kernel interfaces.

## Commands

| Command | Description |
|---|---|
| `drive list` | All block devices from `/sys/block` — health, temp, filesystem, usage |
| `drive info <device>` | Model, serial, firmware, SMART status, temperature via sysfs |
| `drive mount <device>` | `blkid` detection, auto mountpoint suggestion, creates dir if needed |
| `drive umount <device>` | Flushes buffers (`sync`), lazy-unmount fallback, busy-device hints |
| `drive format <device>` | Dispatches `mkfs.{ext4,btrfs,xfs,exfat,vfat,ntfs,f2fs}` |
| `drive health` | hwmon sysfs temps + `smartctl` text SMART/wear parsing |
| `drive snapshot create` | `btrfs subvolume snapshot -r` with timestamp name |
| `drive snapshot list` | Lists `/.snapshots/` with size and creation time |
| `drive snapshot delete` | `btrfs subvolume delete` by name |
| `drive snapshot restore` | Creates writable subvolume from read-only snapshot |
| `drive clone <src> <dst>` | `ddrescue` (falls back to `dd`) + SHA-256 head verify |
| `drive repair <device>` | Dispatches `e2fsck`/`btrfs check`/`xfs_repair`/`fsck.fat`/`ntfsfix` |
| `drive benchmark <path>` | Sequential + random (4K) read/write with IOPS and latency |

## Output flags

```
--toml Machine-readable TOML (list, health, info)
-v, --verbose Print every shell command as it runs
```

## Requirements

- Linux (reads `/sys/block`, `/proc/mounts`, `/dev/disk/by-label`)
- Rust 1.75+
- Root / `sudo` for destructive operations
- Optional tools (graceful degradation if absent):
 `smartmontools`, `ddrescue`, `btrfs-progs`, `e2fsprogs`, `xfsprogs`,
 `exfatprogs`, `ntfs-3g`, `f2fs-tools`

## Build

```bash
cargo build --release
sudo install -m755 target/release/drive /usr/local/bin/
```

## Test

```bash
# Unit tests (no root, no devices needed)
cargo test

# Integration tests (builds the binary first)
cargo test --test integration
```

## Man page

```bash
cargo run --bin generate-manpage --features manpage
sudo cp drive.1 /usr/local/share/man/man1/
sudo mandb
man drive
```

## Configuration

`~/.config/drive/config.toml` (all fields optional):

```toml
mount_base = "/mnt"
temp_warn_celsius = 45
temp_crit_celsius = 60
snapshot_dir = ".snapshots"
```

## Color scheme

| Color | Meaning |
|---|---|
| **Bold cyan** | Headers and section titles |
| **Bright green / ✓** | Success, Excellent health |
| **Bright yellow / ⚠** | Warnings, elevated temperature |
| **Bright red / ✖** | Errors, Critical health, SMART failure |
| **Bright blue** | Device names and paths |
| **Bright magenta** | Values and data |
| Orange (`truecolor`) | Fair / slightly elevated |

## Project layout

```
src/
├── main.rs
├── cli.rs clap subcommand tree + dispatch
├── config.rs ~/.config/drive/config.toml loader
├── error.rs DriveError typed enum
├── core/
│ ├── device.rs enumerate_devices() via /sys/block
│ ├── mount.rs run_mount / run_umount
│ ├── format.rs run_format → mkfs dispatch
│ ├── health.rs hwmon sysfs + smartctl text
│ ├── snapshot.rs btrfs subvolume management
│ ├── clone.rs ddrescue / dd + SHA-256 verify
│ ├── repair.rs fsck dispatch by detected FS type
│ └── benchmark.rs seq + 4K random I/O benchmark
├── ui/display.rs all coloring and print helpers
├── utils/units.rs bytes_to_human (SI units)
├── utils/validator.rs device path and FS validation
└── bin/generate_manpage.rs man page generator (--features manpage)
tests/
└── integration.rs assert_cmd CLI integration tests (25 tests)
benches/
└── io_bench.rs cargo-bench sequential I/O
```

## License

MIT
