# user_utils

**Next-generation system utilities by Zainium Dynamics**  
for Zainium OS.

A Rust implementation of classic Unix userland tools, delivered as **one
discrete binary per utility** — no multi-call dispatcher, no symlink forest.
Each `cmd/*` crate builds its own standalone executable directly.

| | |
|---|---|
| **Organization** | Zainium Dynamics |
| **Language** | Rust (edition 2021) |
| **License** | GPL-3.0 |
| **Primary product** | discrete per-utility binaries (`cat`, `ls`, `mount`, …) |
| **Platform** | Zainium OS |
| **Independence** | No `uucore`, no `uutils`, no stubs |

---

## Highlights

- **Discrete binaries** — every utility is its own standalone executable, built directly from its `cmd/*` crate
- **170+ utilities**, each a `cmd/<name>` crate exporting `pub fn run() -> i32`
- **Zainium-first layout** — installs under `/overlayer/syshub`, never assumes `/usr/bin`
- **Coreutils, modernized** — the classic GNU coreutils surface (`cat`, `ls`, `cp`, `sort`, …), reimplemented from scratch in Rust with real syscalls and complete common flag sets, not a thin wrapper
- **util-linux suite** — system/diagnostic tools (`lsns`, `lslocks`, `lsipc`, `last`, `lscpu`, `lsmem`, `dmesg`, `uuidgen`, `cal`, `hexdump`, `blockdev`, …) each verified flag-by-flag against the [uutils/util-linux](https://github.com/uutils/util-linux) reference **and** byte-for-byte diffed against the real system binaries — see [`DEVPLAN.md`](DEVPLAN.md) / [`checklist/`](checklist/) for the full parity log, and [`MISSING.md`](MISSING.md) for what's still missing
- **Findutils suite** — `find`, `xargs`, `locate`, `updatedb` (pure user_utils, no uucore)
- **Checksum family** — MD5, SHA-1/2, BLAKE2b, POSIX `cksum` / BSD `sum` (in-tree digests)
- **Cross targets** — host, generic musl (`x86_64-unknown-linux-musl`), and the native Zainium musl target (`targets/x86_64-zainium-linux-musl.json`) — see [Cross-compiling for Zainium](#cross-compiling-for-zainium)
- **Next-gen tools** — `blueprint`, `struct`, `mv` (formerly `xmv`), `drive`, `prio`, `trigger`

---

## Quick start

```bash
# Build every discrete binary
./scripts/build.sh
# equivalent:
cargo build --release --workspace

# Run one directly
./target/release/cat README.md
./target/release/ls -la

# Install into a Zainium root (one binary per name, no symlinks)
PREFIX=/overlayer/syshub ./scripts/install.sh
```

**Artifacts**

| Build | Path |
|-------|------|
| Host release | `target/release/<tool>` |
| musl `x86_64` (generic) | `target/x86_64-unknown-linux-musl/release/<tool>` |
| musl `x86_64` (native Zainium target) | `target/x86_64-zainium-linux-musl/release/<tool>` |

---

## Architecture

```text
user_utils/
├── core/ # usercore (UI, errors, digests, Zainium paths)
├── cmd/ # one crate per utility (lib + thin bin) — each builds its own binary
│ ├── cat/, ls/, find/, xargs/, …
│ ├── blueprint/, struct/, mv/, drive/, prio/, trigger/
│ └── …
├── targets/ # custom Rust target specs for Zainium's native musl toolchain
└── scripts/ # build.sh, build-zainium.sh, install.sh, test.sh, clippy.sh
```

Adding a tool means adding a `cmd/<name>` crate that exports
`pub fn run() -> i32` from `src/lib.rs`, a thin `src/main.rs` that calls it,
and one line in the root `Cargo.toml` `members` list — no central dispatch
table to keep in sync.

### Shared core (`usercore`)

| Module | Role |
|--------|------|
| `ui` | Zainium terminal palette (cyan / green / magenta / status marks) |
| `error` / `exit` | Unified errors and exit codes |
| `digest` | Pure-Rust MD5, SHA-1/2, BLAKE2b (no external crypto crates) |
| `blkprobe` | Filesystem superblock probing (ext2/3/4, swap, xfs, vfat, iso9660) — a libblkid stand-in |
| `ptable` | MBR/GPT partition table read/write — a libfdisk stand-in |
| `zainium` | Default `PATH`, install prefix, locate DB resolution |

Data tools keep **stdout clean** for pipelines; diagnostics go to **stderr**
and respect `NO_COLOR`.

---

## Zainium filesystem layout

Zainium does **not** use FHS `/usr/bin`. Reference profile:

```text
PATH="/overlayer/syshub/bin:/overlayer/syshub/sbin"
LD_LIBRARY_PATH="/overlayer/syshub/lib"
```

| Setting | Resolution (first wins) |
|---------|-------------------------|
| Install prefix | `ZEX_PREFIX` → `/overlayer/syshub` |
| Default `PATH` (if unset) | `/overlayer/syshub/bin:/overlayer/syshub/sbin` |
| Locate database | `LOCATE_PATH` → `ZEX_LOCATEDB` → `$ZEX_PREFIX/var/lib/misc/locatedb` |

Nothing in the default install path hardcodes `/usr` or `/bin`.

---

## Build

Builds are driven by shell scripts under `scripts/` (matching the convention
used by zex-native/elevate) rather than a Makefile. Defaults for `PREFIX`
and `PROFILE` live in `utils.toml` at the repo root — the scripts never
hardcode them, they read `utils.toml` and fall back to it only when the
matching env var (`PREFIX`/`ZEX_PREFIX`, `PROFILE`) isn't set.

```toml
# utils.toml
[paths]
prefix = "/overlayer/syshub"

[build]
profile = "release"
```

### Default (every discrete binary)

```bash
cargo build --release --workspace
# equivalent:
./scripts/build.sh
```

### Target triple (`x86_64-…`)

```bash
# musl (static-pie friendly)
TARGET=x86_64-unknown-linux-musl ./scripts/build.sh

# GNU libc host triple
TARGET=x86_64-unknown-linux-gnu ./scripts/build.sh
```

Requires the target installed, e.g.:

```bash
rustup target add x86_64-unknown-linux-musl
```

Linker for musl is configured in `.cargo/config.toml` (`musl-gcc` fallback).
`scripts/build.sh` additionally auto-detects the real Zainium musl
cross-toolchain from `zairoot` (see `utils.toml`'s `[toolchain]` table) and
uses `x86_64-zainium-linux-musl-gcc` when present, producing a genuine
static binary linked against Zainium's own musl sysroot rather than the
host's.

### Cross-compiling for Zainium (native target spec)

For a fully static build against Zainium's own `x86_64-zainium-linux-musl`
target (not just the generic rustup musl triple pointed at Zainium's
cross-gcc), use the custom target spec under `targets/` — see
[`targets/README.md`](targets/README.md) for what the spec sets and why:

```bash
./scripts/build-zainium.sh
```

This needs a pinned nightly toolchain (`-Z build-std`, since no prebuilt
`std` ships for a custom target JSON) — see the script header for the
exact invocation and override env vars.

**Note on relibc:** Zainium OS's real native libc is `relibc-zainium`
(`x86_64-zainium-relibc`), not musl. musl is used here as a practical
bring-up/compat target — `relibc.toml` itself marks
`x86_64-zainium-linux-musl` as `buildable = false` / "independently-built",
i.e. explicitly not the final native target. Switching user_utils to build
natively against relibc is blocked on relibc gaining full `std` support (it's
currently `no_std + alloc` only, per its own README), since user_utils is
ordinary `std` Rust.

### Single crate

```bash
cargo build --release -p user_ls
cargo build --release -p user_xargs
cargo build --release -p user_sha256sum
```

### Throughput-optimized profile

`cp`, `mv`, `drive`, and the checksum utilities do bulk I/O where raw speed
can matter more than binary size. The default `release` profile is tuned for
a fleet of small discrete binaries (size + cold-start latency); opt into
`release-fast` (same settings, `opt-level = 3`) when that tradeoff matters:

```bash
cargo build --profile release-fast -p user_cp
PROFILE=release-fast ./scripts/build.sh
```

### Scripts summary

| Script | Action |
|--------|--------|
| `scripts/build.sh` | Every discrete `cmd/*` binary (`PROFILE`, `TARGET` env vars) |
| `scripts/build-zainium.sh` | Static build against the native `x86_64-zainium-linux-musl` target spec |
| `scripts/install.sh` | Installs every discrete binary into `PREFIX/bin` |
| `scripts/test.sh` | `cargo test --workspace` |
| `scripts/clippy.sh` | `cargo clippy --workspace` (pass `--strict` for `-D warnings`) |
| `cargo clean` | Remove `target/` |

---

## Installation

```bash
# Live overlayer
PREFIX=/overlayer/syshub ./scripts/install.sh

# Cross-built musl binary into a staged rootfs
TARGET=x86_64-unknown-linux-musl PREFIX=/overlayer/syshub DESTDIR=/path/to/rootfs \
  ./scripts/install.sh
```

`scripts/install.sh`:

- Builds every `cmd/*` crate
- Installs each resulting binary to `$DESTDIR$PREFIX/bin/<name>` (e.g. `ls`, `cat`, `mount`)
- Default `PREFIX` is `/overlayer/syshub` (override with `PREFIX` / `ZEX_PREFIX`)

After install, ensure:

```bash
export PATH="/overlayer/syshub/bin:/overlayer/syshub/sbin"
```

---

## Usage

```bash
ls -la
cat file
sha256sum < file
find / -name passwd
xargs -n1 echo
```

### Findutils

| Command | Description |
|---------|-------------|
| `find` | Search directory hierarchies |
| `xargs` | Build and run command lines from stdin |
| `updatedb` | Build the locate database |
| `locate` | Query the database by pattern |

```bash
updatedb -U /home -o /tmp/locatedb
locate -d /tmp/locatedb '*.conf'
```

### util-linux suite

System/diagnostic tools ported from the real `util-linux` package's
command surface. Each one was compared flag-by-flag against the
[uutils/util-linux](https://github.com/uutils/util-linux) Rust reference
implementation, and the ones with real system state to inspect were
additionally diffed byte-for-byte against the actual system binaries
(`lsns`, `lslocks`, `lsipc` against live IPC/lock/namespace state; `last`
against real `/var/log/wtmp` history; `lscpu`/`lsmem` against real
sysfs). Full gap analysis, decisions, and verification logs are in
[`DEVPLAN.md`](DEVPLAN.md) and [`checklist/`](checklist/); the remaining
gap against upstream util-linux is tracked in [`MISSING.md`](MISSING.md).

| Command | Description |
|---------|-------------|
| `lscpu` | Display CPU architecture information |
| `lsmem` | List memory ranges and their online status (NUMA node/zone aware) |
| `lsns` | List namespaces (mnt, net, pid, user, uts, ipc, cgroup, time) |
| `lslocks` | List local file locks, with holder/blocker relationships |
| `lsipc` | Show System V IPC usage (shared memory, semaphores, message queues) |
| `last` | Show a listing of last logged-in users, reboots, and crashes |
| `dmesg` | Display or filter the kernel ring buffer |
| `hexdump` | Display file contents in hex/octal/decimal/ASCII |
| `blockdev` | Query or set block device parameters |
| `cal` | Display a calendar |
| `chcpu` | Configure CPUs online/offline |
| `ctrlaltdel` | Query/set the Ctrl-Alt-Del handling behavior |
| `fsfreeze` | Freeze/unfreeze a filesystem |
| `mcookie` | Generate a magic cookie for X11 `xauth` |
| `mesg` | Control write access to your terminal |
| `mountpoint` | Check whether a path is a mount point |
| `nologin` | Politely refuse a login |
| `renice` | Alter the scheduling priority of running processes |
| `rev` | Reverse the characters in every line |
| `setpgid` | Run a program in a new process group |
| `setsid` | Run a program in a new session |
| `uuidgen` | Generate a new UUID (v1 time-based, v3/v5 name-based, v4 random) |
| `mount` | Mount a filesystem |
| `umount` | Unmount a filesystem |
| `findmnt` | List or search mounted filesystems |
| `losetup` | Set up and control loop devices |
| `pivot_root` | Change the root filesystem |
| `switch_root` | Switch to another filesystem as root (initramfs) |
| `swapon` | Enable a swap area |
| `swapoff` | Disable a swap area |
| `blkid` | Locate/print block device attributes (TYPE/UUID/LABEL) |
| `lsblk` | List block devices as a tree |
| `findfs` | Find a filesystem by LABEL or UUID |
| `fdisk` | List partition tables (`-l`) |
| `sfdisk` | Scriptable partition table tool (dump/list/write) |
| `partx` | Tell the kernel about a device's partitions |
| `mkswap` | Set up a swap area |
| `fsck` | Filesystem check front-end (dispatches to `fsck.<type>`) |
| `chattr` | Change ext2/3/4 file attributes (immutable, append-only, …) |
| `lsattr` | List ext2/3/4 file attributes |

`chattr`/`lsattr` are ported from **e2fsprogs**, not util-linux, but live
here since that's where the rest of the ext2/3/4-attribute tooling is —
see [`checklist/chattr-lsattr.md`](checklist/chattr-lsattr.md).

```bash
lsns
lslocks -o COMMAND,PID,PATH
lsipc -m
last -x -n 10
lscpu -J
```

---

## Utility catalogue

### Coverage (selection)

**Text & filters:** `cat` `head` `tail` `wc` `cut` `tr` `tee` `paste` `sort` `uniq` `expand` `unexpand` `fold` `fmt` `nl` `tac` `od` `comm` `join` `pr` `ptx` `csplit` `printf` `echo` `yes` `grep` `sed` `more`

**Paths & FS:** `ls` `dir` `vdir` `dircolors` `cp` `rm` `rmdir` `ln` `link` `unlink` `chmod` `chown` `chgrp` `chattr` `lsattr` `df` `du` `stat` `readlink` `realpath` `basename` `dirname` `pathchk` `mktemp` `mkfifo` `mknod` `truncate` `shred` `install` `sync` `dd` `tar` `chroot`

**Checksums:** `md5sum` `sha1sum` `sha224sum` `sha256sum` `sha384sum` `sha512sum` `b2sum` `sum` `cksum` `basenc` `base32` `base64`

**Identity & system:** `id` `whoami` `logname` `groups` `hostname` `uname` `arch` `hostid` `nproc` `date` `env` `printenv` `pwd` `tty` `which` `uptime` `who` `users` `pinky` `stty`

**Process & control:** `kill` `nice` `timeout` `nohup` `sleep` `true` `false` `test` `[` `expr` `seq` `factor` `numfmt` `ps` `free` `pgrep` `pkill` `stdbuf`

**Findutils:** `find` `xargs` `locate` `updatedb`

**util-linux (system/diagnostic):** `lscpu` `lsmem` `lsns` `lslocks` `lsipc` `last` `dmesg` `hexdump` `blockdev` `cal` `chcpu` `ctrlaltdel` `fsfreeze` `mcookie` `mesg` `mountpoint` `nologin` `renice` `rev` `setpgid` `setsid` `uuidgen` `mount` `umount` `findmnt` `losetup` `pivot_root` `switch_root` `swapon` `swapoff` `blkid` `lsblk` `findfs` `fdisk` `sfdisk` `partx` `mkswap` `fsck` — see [util-linux suite](#util-linux-suite) above for descriptions

Full list: `ls cmd/` (one crate per utility name).

### Diffutils

| Command | Description |
|---------|-------------|
| `diff` | Line-oriented file comparison (normal / unified / context / ed / side-by-side) |
| `cmp` | Byte-by-byte comparison |

Vendored from [uutils/diffutils 0.5.0](https://github.com/uutils/diffutils) (**MIT OR Apache-2.0**, no `uucore`), integrated as `cmd/diffutils` + thin `diff`/`cmp` crates.

```bash
diff -u old.txt new.txt
cmp a.bin b.bin
```

`diff3` / `sdiff` are not in 0.5.0 yet (upstream incomplete).

### Zainium naming (important)

| If you type… | Zainium does… |
|--------------|----------------|
| **`mv`** | Full next-gen move (formerly `xmv`) — journal, cross-device, undo |
| **`touch`** | GNU-compatible timestamps / empty create |
| **`mkdir`** | **Not** GNU mkdir — prints guidance to use **`struct`** |
| **`tree`** | **Not** tree(1) — prints guidance to use **`blueprint`** |

| Role | Tool |
|------|------|
| Safe create (mkdir -p + touch-style file, no overwrite) | **`struct`** |
| Project / structure layouts | **`blueprint`** |
| Move / rename | **`mv`** |
| Storage management | **`drive`** |
| Process priority / cgroup | **`prio`** |
| Launch / discovery | **`trigger`** |

---

## Design principles

1. **Correct for pipelines** — pure data on stdout; status on stderr 
2. **No dummy implementations** — real syscalls and complete common flag sets 
3. **No uutils stack** — independent of `uucore` / `uu_*` crates 
4. **Env over hardcoding** — `ZEX_PREFIX`, `PATH`, `LOCATE_PATH`, `NO_COLOR` 
5. **Zainium-first** — Zainium OS only; other platforms are not supported 

### Terminal palette (status UI)

| Element | Style |
|---------|--------|
| Headings | Bright cyan |
| Labels | Soft / bright green |
| Values | Bright magenta |
| Success | Bright green `✓` |
| Warning | Bright yellow `⚠` |
| Error | Bright red `✖` |

---

## Development

```bash
# Build a single util in debug mode
cargo build -p user_cat

# Run a single util directly
cargo run -p user_cat -- --help

# Adding a new tool: create cmd/<name>/ with a lib.rs exposing
# pub fn run() -> i32, a thin main.rs, and add "cmd/<name>" to the
# root Cargo.toml [workspace] members list.
```

---

## License

GPL-3.0 — see individual crate metadata for package-level notes.

**Copyright (c) Zainium Dynamics.** user_utils is a Zainium Dynamics product.

---

## Project status

Owned and developed by **Zainium Dynamics** for **Zainium OS**.
Discrete per-utility binaries are the supported distribution form for core
userland tools; next-gen utilities ship the same way, as standalone crates
under `cmd/`.
