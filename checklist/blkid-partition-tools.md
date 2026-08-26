# blkid / lsblk / findfs / fdisk / sfdisk / partx / mkswap / fsck

**Status:** DONE (2026-08-26)

**Source:** own implementation, not ported from a specific C file —
these tools' real backing libraries (`libblkid`, `libfdisk`) aren't
vendored in this workspace. On-disk struct layouts and magic offsets
were taken from util-linux's own reference source
(`libblkid/src/superblocks/{ext,swap,xfs,vfat,iso9660}.c`) and the
Linux/UEFI ABI (`linux/blkpg.h`, the GPT header/entry-array spec), then
verified against the *real* `blkid`/`fdisk`/`sfdisk` binaries on real
filesystem images (not just our own round-trip) — see Verified below.

## What was built

- **`usercore::blkprobe`** (new core module): superblock probing for
  ext2/3/4, swap (v0/v1), xfs, vfat/FAT12/16/32, iso9660 — signature,
  UUID, and label, matching what `blkid` reports for each.
- **`usercore::ptable`** (new core module): MBR (4 primary entries) and
  GPT (header + entry array, primary *and* backup copy with correct
  CRC-32) read *and* write. GPT GUID formatting/parsing handles the
  spec's mixed-endian encoding correctly.
- **`blkid`**: lists `LABEL`/`UUID`/`TYPE` for given devices or every
  device in `/proc/partitions`; `-L`/`-U` lookup; `-s TAG -o value`.
- **`lsblk`**: `/sys/block` tree (disk → partitions), major:minor, size,
  removable/read-only, type, mountpoints; `-f` shows fstype/label/UUID.
- **`findfs`**: thin wrapper over `blkid`'s shared `find_by_field` (the
  same `cmd/X` → `cmd/Y` path-dependency pattern already used by
  `dir`→`ls` and `pkill`→`pgrep`).
- **`fdisk`**: `-l [DEVICE...]` listing only — the interactive edit REPL
  real `fdisk` has is a large stateful undertaking, disproportionate to
  what remained; `sfdisk` covers scripted editing instead, and `fdisk`
  says so if invoked without `-l`.
- **`sfdisk`**: `-d` (dump, in a re-readable script format), `-l`
  (human-readable list), and the write path — reads a script from stdin
  and writes a fresh MBR or GPT table, with `L`/`S`/`U` short type
  aliases sfdisk itself accepts.
- **`partx`**: `-s` (list, via the same `ptable` reader), `-a`/`-d` (tell
  the kernel about a partition via the `BLKPG` ioctl — hand-defined
  `blkpg_ioctl_arg`/`blkpg_partition` structs, since `libc` doesn't
  expose them).
- **`mkswap`**: writes a real v1 `SWAPSPACE2` header (page-size-aware,
  via `sysconf(_SC_PAGESIZE)`), `-L`/`-U`, random UUID by default.
- **`fsck`**: front-end only — detects filesystem type (`-t`, or
  `blkprobe`) and dispatches to `fsck.<type>` found on Zainium's `PATH`;
  no per-filesystem checker (`e2fsck`, `xfs_repair`, …) is vendored here,
  matching how real `fsck(8)` itself is architected.

## Verified

- `cargo test`: `blkprobe` and `ptable` each have round-trip tests (write
  then read back through *our own* code) plus one test per filesystem
  type/table format.
- **Cross-verified against the real, unmodified `blkid`/`fdisk`/`sfdisk`
  binaries on this machine** (not just our own code reading our own
  output):
  - Built real ext4/vfat(FAT32)/xfs images with the actual `mkfs.ext4`/
    `mkfs.vfat`/`mkfs.xfs`/`mkswap` tools, then ran our `blkprobe`
    against them — `TYPE`/`UUID`/`LABEL` matched real `blkid`'s output
    exactly on every field we support.
  - Wrote MBR and GPT tables with our own `ptable::write_mbr`/
    `write_gpt`, then had the real `fdisk -l` and `sfdisk -d` read them
    back — correct label type, start/end/sectors, boot flag, partition
    type name, GPT partition name, and (for GPT) **no CRC/backup-header
    warnings**, confirming the header and entry-array checksums are
    computed correctly.
  - Fed a script into our own `sfdisk`'s write path (stdin → new MBR/GPT
    table on a scratch image), then verified the result with real
    `fdisk -l` and `sfdisk -d` the same way — full write-path round trip
    through independent, unmodified tooling.
- Every privileged/hardware-dependent path (`partx`'s `BLKPG` ioctl) was
  confirmed to fail cleanly (real `ENOTTY`/permission error, not a panic)
  against a non-block-device target; the `BlkpgPartition`/
  `BlkpgIoctlArg` struct sizes are asserted against the documented
  kernel ABI (152 / 24 bytes) in a unit test. This sandbox has no loop
  device or writable block device access, so the ioctl itself couldn't
  be exercised live against a real kernel partition table — flagging
  this honestly rather than claiming untested ground.

## Known gaps (documented, not silently missing)

- `blkprobe` omits `BLOCK_SIZE` and vfat's `LABEL_FATBOOT` (real `blkid`
  reports both) — confirmed via the same cross-verification above; every
  other field matches.
- `blkprobe`'s vfat path is magic-string-only; it doesn't do libblkid's
  cluster-count fallback for a FAT filesystem with no magic string at
  all (rare in practice).
- `ptable`'s MBR support is primary partitions only — no extended/logical
  partition chain.
- `fdisk` has no interactive mode; `partx -a`/`-d` couldn't be exercised
  against a real kernel (see above).
