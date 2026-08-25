# mount / umount / pivot_root / switch_root / findmnt / losetup / swapon / swapoff

**Status:** DONE (2026-08-25)

**Source:** own implementation against `mount(2)`/`umount2(2)`/`pivot_root(2)`
syscalls and the relevant Linux ABI (loop, swap ioctls/flags), not ported
from a specific C file — the real `mount`/`losetup`/etc rely on
`libblkid`/`libmount`, which this workspace doesn't have.

## What was built

- **`mount`**: direct `SOURCE TARGET` mounts, `-t`/`-o`, `/etc/fstab`
  lookup by device or mountpoint, `-a`, `--bind`/`--rbind`/`--move`. No
  `fstype` (or `auto`) tries a fixed candidate list
  (ext4/ext3/ext2/xfs/btrfs/vfat/exfat/ntfs3/iso9660/f2fs) in turn — a
  practical stand-in for libblkid-based detection.
- **`umount`**: mountpoint or device argument, `-f`/`-l`/`-R`/`-a`,
  deepest-mount-first ordering for `-R`/`-a`.
- **`pivot_root`**: thin `pivot_root(2)` wrapper (no libc binding exists
  for it — called via `libc::syscall(SYS_pivot_root, ...)`).
- **`switch_root`**: validates `NEWROOT` (self-bind-mounts it if it isn't
  already its own mountpoint), `mount --move` onto `/`, `chroot`, `exec
  INIT`. Deliberately skips the old-root recursive-delete step real
  `switch_root(8)` does — that's a RAM-reclaim optimization, not part of
  what switching root means, and skipping it means a bad invocation can
  never wipe the wrong tree.
- **`findmnt`**: reads `/proc/self/mounts`; `-t`/-S` filters, `TARGET`
  lookup finds the mount with the longest matching path prefix (correctly
  falls back to `/` for anything not under a more specific mount).
- **`losetup`**: `LOOP_SET_FD`/`LOOP_CLR_FD`/`LOOP_SET_STATUS64`/
  `LOOP_GET_STATUS64`/`LOOP_CTL_GET_FREE` ioctls and the `loop_info64`
  struct layout (all hand-defined — not in the `libc` crate), `-f
  [--show]`, `-d`, `-a` (via `/sys/block/loop*/loop/backing_file`),
  direct `DEVICE FILE` attach, `-o`/`-r`.
- **`swapon`/`swapoff`**: `swapon(2)`/`swapoff(2)`, `-a` via `/etc/fstab`
  (`swapon`) / `/proc/swaps` (`swapoff`), `-p`/`-d`, `-s` summary.
- All of `mount`/`umount`/`switch_root`'s target-modifying paths go
  through `usercore::protect::modification_denied`, same as
  `chmod`/`chown`/`chattr`.

## Verified

- `cargo test`: option/flag parsing (`mount`'s `-o` → `MS_*` mapping,
  `/etc/fstab` parsing), `loop_info64` name truncation, `/proc/swaps`
  parsing, `findmnt`'s longest-prefix matching (including the root
  fallback — caught and fixed a real bug where `/` never matched because
  `format!("{}/", "/")` produced `"//"`).
- Live, read-only checks work end to end on this machine: `mount`/
  `findmnt` with no args print the real `/proc/self/mounts` content
  correctly formatted; `swapon -s` matches real `/proc/swaps`; `losetup
  -a` scans `/sys/block/loop*` without error.
- Every privileged path (`mount`, `umount`, `losetup -f`, `swapon`,
  `swapoff`, `pivot_root`, `switch_root`'s bind-mount) was exercised
  unprivileged and confirmed to fail with a real `EPERM`/`EACCES` from the
  kernel, not a panic or a false success — this sandbox has no root, so
  that's as far as live-testing privileged mount/loop/swap operations
  could go.

## Known caveat (shared with chattr/lsattr, not new here)

Same as `checklist/chattr-lsattr.md`: on this dev machine `/overlayer` is
a symlink into the mounted `zairoot`, so the `protect` guard's
`canonicalize()` can resolve a literal `/overlayer/syshub/...` path to
somewhere that no longer starts with that prefix, making the guard a
no-op for real files here specifically. Not touched — shared
infrastructure, real Zainium installs don't have this symlink.
