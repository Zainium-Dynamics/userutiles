# user_utils

Next-generation system utilities by Zainium Dynamics, for Zainium OS.

A from-scratch Rust implementation of the classic Unix userland — coreutils,
a growing slice of util-linux, and a few Zainium-native tools — shipped as
one discrete binary per utility (no multi-call dispatcher, no `uucore`/
`uutils` dependency, no stub implementations). Every `cmd/<name>` crate
builds its own standalone executable.

175+ utilities today. Coverage: ~98% of GNU coreutils, ~31% of util-linux
2.42 (see [`MISSING.md`](MISSING.md) for the gap list and
[`checklist/`](checklist/) for per-tool verification notes).

## Quick start

```bash
./scripts/build.sh                       # build every discrete binary
./target/release/ls -la                  # run one directly
PREFIX=/overlayer/syshub ./scripts/install.sh   # install into a Zainium root
```

Host builds land in `target/release/<tool>`; cross builds under
`target/<triple>/release/<tool>`.

## Layout

```
core/       usercore — shared UI, digests, block-device/partition probing,
            Zainium path resolution (no libblkid/libfdisk/PAM dependency)
cmd/        one crate per utility: src/lib.rs exports pub fn run() -> i32,
            src/main.rs is a two-line wrapper
targets/    custom Rust target spec for Zainium's native musl toolchain
scripts/    build.sh, build-zainium.sh, install.sh, test.sh, clippy.sh
```

Adding a tool means adding a `cmd/<name>` crate and one line in the root
`Cargo.toml` members list — there's no central dispatch table to keep in
sync. Data tools keep stdout clean for pipelines; diagnostics go to stderr
and respect `NO_COLOR`.

## Zainium filesystem layout

Zainium has no FHS `/usr` or top-level `/etc` — everything lives under
`/overlayer/syshub` (`bin`, `sbin`, `lib`, `etc`, …). Tools resolve this via
`usercore::zainium`, falling back to the ordinary `/etc`, `$PATH`, etc. on a
plain Linux host so the workspace builds and tests normally outside Zainium
too. `ZEX_PREFIX` overrides the install prefix; nothing hardcodes `/usr` or
`/bin`.

## Build

Defaults for `PREFIX`/`PROFILE` live in `utils.toml`; env vars
(`PREFIX`/`ZEX_PREFIX`, `PROFILE`, `TARGET`) override them.

```bash
cargo build --release --workspace              # everything, host target
TARGET=x86_64-unknown-linux-musl ./scripts/build.sh   # generic musl
./scripts/build-zainium.sh                     # Zainium's native musl target spec
cargo build --release -p user_ls               # a single crate
PROFILE=release-fast ./scripts/build.sh        # opt-level 3, for cp/mv/drive/checksums
```

`scripts/build-zainium.sh` needs a pinned nightly (`-Z build-std`, no
prebuilt `std` for the custom target JSON) — see the script header. Zainium
OS's real native libc is `relibc`, not musl; musl is a practical bring-up
target until relibc has full `std` support. See
[`targets/README.md`](targets/README.md) for details.

`scripts/install.sh` builds everything and installs each binary to
`$DESTDIR$PREFIX/bin/<name>` (default prefix `/overlayer/syshub`).

## What's here

**Coreutils surface** — `cat`, `cp`, `mv`, `rm`, `ls`, `sort`, `chmod`,
`chown`, `dd`, `tar`, checksums (md5/sha1/sha2/blake2b/cksum/sum), and the
rest of the standard set, reimplemented from scratch with real syscalls.
Full list: `ls cmd/`.

**Findutils** — `find`, `xargs`, `locate`, `updatedb`.

**util-linux subset** — namespace/lock/IPC inspection (`lsns`, `lslocks`,
`lsipc`, `last`, `lscpu`, `lsmem`), and — as of the current pass — the
mount/storage/partition stack: `mount`, `umount`, `findmnt`, `losetup`,
`pivot_root`, `switch_root`, `swapon`, `swapoff`, `blkid`, `lsblk`,
`findfs`, `fdisk`, `sfdisk`, `partx`, `addpart`, `delpart`, `resizepart`,
`mkswap`, `fsck`, `login`. Two new `usercore` modules — `blkprobe`
(filesystem superblock probing) and `ptable` (MBR/GPT read/write) — back
most of these; both were cross-verified against the real, unmodified
`blkid`/`fdisk`/`sfdisk` binaries on real filesystem images, not just their
own round-trip (see `checklist/blkid-partition-tools.md`). `chattr`/`lsattr`
are ported from e2fsprogs and live alongside this set for the same reason.
Remaining gaps are tracked in [`MISSING.md`](MISSING.md).

**Diffutils** — `diff`/`cmp`, vendored from uutils/diffutils 0.5.0
(MIT/Apache-2.0, no uucore) as `cmd/diffutils` + thin wrapper crates.

**Zainium-native** — `mv` is a full next-gen move (journal, cross-device,
undo — not GNU `mv`); `mkdir`/`tree` print guidance toward the Zainium-native
`struct`/`blueprint` instead of behaving like their GNU namesakes; `drive`
(storage management), `prio` (process priority/cgroups), `trigger`
(launch/discovery) round out the set.

## Notes on `login`

Reads the passwd/shadow database via `usercore::zainium` (which resolves to
Zainium's `elevate-umbra`-managed files, or plain `/etc` on a host with no
`/overlayer` tree) and verifies passwords through the system's own
`crypt(3)` — no PAM dependency, Linux PAM or otherwise: real PAM needs
service configs Zainium doesn't ship, and there's nothing portable to link
against instead. No PAM, no utmp/wtmp session accounting yet — see
`checklist/` for the exact scope.

## License

GPL-3.0. Copyright (c) Zainium Dynamics.
