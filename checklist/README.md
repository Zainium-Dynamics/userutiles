# Checklist index — util-linux parity pass

**Phase 1 (all 4 P0 build-breaking empty crates) is complete as of 2026-07-27.**
**Phase 2 (P1 bug fixes: `lscpu`, `lsmem`) is complete as of 2026-07-28.**
**Phase 3 (P2 items: `dmesg`, `uuidgen`, `cal`) is complete as of 2026-07-28.**
**Phase 4 (P3 items: `hexdump`, `fsfreeze`) is complete as of 2026-07-28 —
the originally-scoped 22-utility util-linux comparison pass is now fully
closed out.** See `../DEVPLAN.md` for what's next.

Generated 2026-07-27 alongside `../DEVPLAN.md`. Each file below tracks one
utility: what's missing vs. the `util-linux-main` reference, exactly which
files would need to change, and why. Checkboxes are unchecked until code
actually lands — flip them and add a note (+ a `CHANGELOG.md` entry) when
implementing.

| Utility | Priority | Status | File |
|---|---|---|---|
| `lsns` | P0 | **DONE** (2026-07-27) — implemented, tested, wired into multicall | [lsns.md](lsns.md) |
| `lslocks` | P0 | **DONE** (2026-07-27) — implemented, byte-for-byte diffed vs real binary | [lslocks.md](lslocks.md) |
| `lsipc` | P0 | **DONE** (2026-07-27) — implemented, byte-for-byte diffed vs real binary | [lsipc.md](lsipc.md) |
| `last` | P0 | **DONE** (2026-07-27) — implemented, byte-for-byte diffed vs real binary on real wtmp history | [last.md](last.md) |
| `lscpu` | P1 | **DONE** (2026-07-28) — `-B/--bytes` bug fixed | [lscpu.md](lscpu.md) |
| `lsmem` | P1 | **DONE** (2026-07-28) — flags added, bugs fixed, wired into multicall | [lsmem.md](lsmem.md) |
| `dmesg` | P2 | **DONE** (2026-07-28) — bounded relative-date parsing added | [dmesg.md](dmesg.md) |
| `uuidgen` | P2 | **DONE** (2026-07-28) — mutual-exclusivity fixed, node-ID gap decided | [uuidgen.md](uuidgen.md) |
| `cal` | P2 | **DONE** (2026-07-28) — mutual-exclusivity fixed | [cal.md](cal.md) |
| `hexdump` | P3 | **DONE** (2026-07-28) — real grammar gap found (via uucore source) + fixed | [hexdump.md](hexdump.md) |
| `fsfreeze` | P3 | **DONE** (2026-07-28) — decided to keep current (more correct) behavior | [fsfreeze.md](fsfreeze.md) |
| `blockdev`, `chcpu`, `ctrlaltdel`, `mcookie`, `mesg`, `mountpoint`, `nologin`, `renice`, `rev`, `setpgid`, `setsid` | — | at parity or better | [parity-confirmed.md](parity-confirmed.md) |
| `chattr`, `lsattr` | — | **DONE** (2026-08-25) — ported from e2fsprogs 1.47.4 (not util-linux; tracked here anyway), byte-for-byte diffed vs the real binaries | [chattr-lsattr.md](chattr-lsattr.md) |
| `mount`, `umount`, `pivot_root`, `switch_root`, `findmnt`, `losetup`, `swapon`, `swapoff` | P0 | **DONE** (2026-08-25) — see [MISSING.md](../MISSING.md) §2.1/2.2 | [mount-storage-p0.md](mount-storage-p0.md) |
| `blkid`, `lsblk`, `findfs`, `fdisk`, `sfdisk`, `partx`, `mkswap`, `fsck` | P0 | **DONE** (2026-08-26) — cross-verified against real `blkid`/`fdisk`/`sfdisk` | [blkid-partition-tools.md](blkid-partition-tools.md) |
| `addpart`, `delpart`, `resizepart`, `login` | P0 | **DONE** (2026-08-26) | [addpart-delpart-resizepart-login.md](addpart-delpart-resizepart-login.md) |

Not reviewed in this pass (no util-linux-main counterpart): the other ~133
zex-utils utilities (coreutils/findutils/checksum family/next-gen tools).
