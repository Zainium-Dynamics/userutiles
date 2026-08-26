# Changelog

All notable changes to zex-utils are logged here.
Format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Each entry should say *what* changed and *why* — see `checklist/` for the
detailed per-utility rationale and file list behind each item.

## [Unreleased]

### Added (2026-08-26 — blkid / lsblk / findfs / fdisk / sfdisk / partx / mkswap / fsck)
- Two new `usercore` modules: `blkprobe` (ext2/3/4, swap, xfs, vfat,
  iso9660 superblock probing — a from-scratch libblkid stand-in) and
  `ptable` (MBR + GPT partition table read/write — a from-scratch
  libfdisk stand-in, including correct GPT CRC-32 header/entry-array
  checksums and mixed-endian GUID handling).
- `blkid`, `lsblk`, `findfs`, `mkswap` built directly on `blkprobe`.
- `fdisk` (`-l` listing only — no interactive edit mode), `sfdisk`
  (dump/list/write, with `L`/`S`/`U` short type aliases), `partx`
  (`-s`/`-a`/`-d` via a hand-defined `BLKPG` ioctl) built on `ptable`.
- `fsck`: front-end only, dispatches to `fsck.<type>` on `PATH` by
  detected or forced type; no per-filesystem checker vendored.
- Cross-verified against the real, unmodified `blkid`/`fdisk`/`sfdisk`
  binaries on real ext4/vfat/xfs/swap images built by the actual `mkfs.*`
  tools — every field/checksum matched, including a full write-path
  round trip (our `sfdisk` writes a table, real `fdisk -l`/`sfdisk -d`
  read it back with zero warnings). See `checklist/blkid-partition-tools.md`.

### Added (2026-08-25 — mount / umount / pivot_root / switch_root / findmnt / losetup / swapon / swapoff)
- Own implementation against `mount(2)`/`umount2(2)`/`pivot_root(2)` and
  the loop/swap ioctl ABIs (no `libblkid`/`libmount` equivalent exists in
  this workspace, so `mount`'s fstype auto-detection is a fixed candidate
  list, not real superblock probing). See `checklist/mount-storage-p0.md`.
- `mount`: direct mounts, `-t`/`-o`, `/etc/fstab` lookup, `-a`,
  `--bind`/`--rbind`/`--move`.
- `umount`: mountpoint-or-device argument, `-f`/`-l`/`-R`/`-a`.
- `pivot_root`, `switch_root` (core move+chroot+exec mechanism;
  deliberately skips the old-root recursive-delete step real
  `switch_root` does as a RAM-reclaim optimization).
- `findmnt`: reads `/proc/self/mounts`; found and fixed a real bug in its
  own longest-prefix matching where `/` never matched anything since
  `format!("{}/", "/")` produces `"//"`.
- `losetup`, `swapon`, `swapoff`.
- All of `mount`/`umount`/`switch_root`'s target-modifying paths go
  through the same `usercore::protect` guard `chmod`/`chown`/`chattr` use.

### Added (2026-08-25 — chattr / lsattr)
- Ported `chattr`/`lsattr` from e2fsprogs 1.47.4's `misc/chattr.c` and
  `misc/lsattr.c`: full flag set, `-p`/`-v` project/version, recursive
  walk, `-l` long display. See `checklist/chattr-lsattr.md`. `chattr` also
  refuses to touch `/overlayer/syshub`/`/overlayer/zaisys`, the same guard
  `chmod`/`chown` already use.

### Fixed (2026-08-25 — cargo build regression, full clippy/fmt cleanup, real bugs)
- `.cargo/config.toml`: the previous commit's `[build] target =
  "targets/x86_64-zainium-linux-musl.json"` default broke every plain
  `cargo build`/`test`/`clippy`/`fmt` on the normal stable toolchain (that
  target only resolves with `-Z json-target-spec -Z build-std` on
  nightly, which `scripts/build-zainium.sh` already passes explicitly and
  never needed the config default for). Removed the default; kept the
  linker sections.
- `scripts/clippy.sh --strict`: `--exclude zex_diffutils` was stale after
  the rename (package is `user_diffutils`), so it silently stopped
  excluding the vendored crate. Fixed the name; added a targeted
  `#![allow(clippy::incompatible_msrv)]` in `cmd/diffutils` for its one
  real MSRV mismatch (vendored, not hand-maintained for lint cleanliness).
- Full workspace now passes `cargo clippy --workspace --all-targets -D
  warnings` and `cargo fmt --all -- --check` with zero findings (was
  previously unverifiable — nothing built). Fixes across `core` (two
  `needless_range_loop`s in `blake2b`/`sha1`), `cmd/sys`, `cmd/trace`,
  `cmd/df`, `cmd/du` (grouped `du_path`'s flags into `DuOpts` to clear
  `too_many_arguments`), `cmd/rm` (MSRV-incompatible `ErrorKind::
  IsADirectory`; `force` was dead in `remove_dir_recursive` — now actually
  makes `-f` ignore a child that vanished mid-walk), `cmd/find` (`root`
  param was fully unused — removed; a test module was placed before other
  items), `cmd/comm`, `cmd/tr`, `cmd/tar`, `cmd/chgrp` (missing `# SAFETY`
  comment), `cmd/trigger/benches/discovery_bench.rs` (imported a
  never-renamed crate as `user_trigger`).
- Real bugs found once tests could finally run (`cargo test --workspace`):
  `cmd/sha1sum`'s test literal for `SHA1("abc")` was missing its last hex
  digit; `cmd/tail`'s `tail_lines_from(_, 0, _)` printed the last line
  instead of nothing; `cmd/test`'s 3-arg `EXPR1 -a EXPR2`/`-o` fell through
  to the binary-operator table instead of being evaluated as logical
  AND/OR; `cmd/nohup`'s two cwd-mutating tests raced each other (added a
  `Mutex` to serialize them); `cmd/trigger/src/platform.rs`'s Linux/Redox
  app and handler search paths were doubled
  (`/overlayer/syshub/overlayer/syshub/bin`) from the zex→user rename
  sweep, so app/handler discovery silently found nothing.
- `cmd/chroot`'s `resolve_shell()` now tries `bash`, then `sh`, then
  `dash` (was `sh` only) via Zainium's `PATH` directories before falling
  back to `$SHELL`/`/bin/sh`.

### Changed (2026-08-05 — project renamed, multicall removed)
- Project renamed `zex-utils` → `user_utils`. Every Cargo package/lib name
  containing `zex` was renamed to `user` (`zexcore` → `usercore`, all 149
  `zex_*` crates → `user_*`, `zex-seccomp` → `user-seccomp`). Installed
  Unix tool names (`cat`, `ls`, `mount`, …) are unchanged — only internal
  Rust identifiers and project branding moved.
- Removed the `multicall/` busybox-style dispatcher binary entirely.
  `user_utils` now ships **only** discrete per-tool binaries — every
  `cmd/*` crate already built its own standalone `[[bin]]`, so this is a
  subtraction, not new plumbing. `scripts/build-all.sh` is now
  `scripts/build.sh`; `scripts/install.sh` lost its `MODE=multicall` path.
- Added `targets/x86_64-zainium-linux-musl.json` + `scripts/build-zainium.sh`
  for cross-compiling against Zainium's native musl target spec (mirrors
  the pattern already used by `zex-native/elevate-privilege`).

### Fixed (2026-07-28 — Phase 4 complete, 22-utility pass fully closed out)
- `hexdump`: `-n`/`-s`'s size-suffix grammar was a real, confirmed gap, not
  just a suspected one — fetched `uucore` 0.2.2's actual source
  (`cargo fetch` against `util-linux-main`, then read
  `parser/parse_size.rs`) and diffed it field-by-field against our
  `parse_size_u64`. Found and fixed: the decimal/SI `KB`/`MB`/`GB`/...
  family was entirely unsupported (`"5KB"` errored instead of meaning
  5000); suffixes above `G` (`T`/`P`/`E`/`Z`/`Y`/`R`/`Q`) were missing
  entirely; the `b` (block=512) suffix was missing; octal input
  (leading-zero multi-digit numbers) wasn't recognized; case-sensitivity
  was wrong (`"Kb"`/`"kb"` were silently accepted as 1000-based when the
  reference treats them as invalid); overflow silently saturated instead
  of erroring; bare `"B"` was incorrectly accepted. Files:
  `cmd/hexdump/src/lib.rs`. Added a comprehensive regression test,
  `parse_size_u64_matches_uucore_default_parser_grammar`, checked directly
  against `uucore`'s own test-suite expectations. Verified against the
  real system `/usr/bin/hexdump -C -n 2KB` — now byte-for-byte identical
  (previously would have errored on the unrecognized `KB` suffix).
- `fsfreeze`: decided to **keep** the current exit-1-on-real-failure
  behavior rather than match the reference's exit-0-on-failure (which is
  arguably a bug in the reference — a caller scripting against `fsfreeze`
  couldn't detect a failed freeze/unfreeze that way). No code change; see
  `checklist/fsfreeze.md`.

This closes out every item from the original 22-utility util-linux
comparison pass (`DEVPLAN.md`'s P0 through P3 tiers). See
`checklist/hexdump.md` and `checklist/fsfreeze.md`.

### Added / Fixed (2026-07-28 — Phase 3 complete)
- `cal`: `-y`/`-Y`/`-n` are now mutually exclusive (previously silently
  prioritized `-y` > `-Y` > `-n` with no error). Files: `cmd/cal/src/lib.rs`.
  Added tests `parse_args_rejects_combined_y_and_n`,
  `parse_args_rejects_combined_y_and_twelve`.
- `uuidgen`: `-r`/`-t`/`-m`/`-s` are now mutually exclusive (previously
  silently gave `-t` priority with no error). Files:
  `cmd/uuidgen/src/lib.rs`. Also made an explicit product decision to
  **keep** the current privacy-safe random v1-UUID node ID rather than
  match the reference's real-MAC-address lookup, since that would embed a
  stable hardware identifier in every generated UUID — see
  `checklist/uuidgen.md`.
- `dmesg`: `--since`/`--until` now additionally accept `"now"`, `"today"`,
  `"yesterday"`, and `"N <unit>[s] ago"` (second/minute/hour/day/week) on
  top of the existing fixed timestamp formats — still a deliberately
  bounded, hand-rolled parser (no `parse_datetime` crate dependency), not
  full GNU date grammar. Files: `cmd/dmesg/src/lib.rs`. Added tests
  `parse_datetime_accepts_now_today_yesterday`,
  `parse_datetime_accepts_relative_ago_forms`,
  `parse_datetime_rejects_unknown_relative_forms`.

Verified: `cargo test -p zex_cal -p zex_uuidgen -p zex_dmesg` all pass,
`cargo clippy` clean on all three, `cargo build --workspace` clean. See
`checklist/cal.md`, `checklist/uuidgen.md`, `checklist/dmesg.md`. **This
closes out Phase 3** — all P2 items done.

### Fixed (2026-07-28 — Phase 2 complete)
- `lscpu`: `-B/--bytes` was parsed but never threaded through to cache-size
  formatting, so cache sizes always printed human-readable even with `-B`.
  Fixed by threading `bytes: bool` through `collect()` into
  `calculate_cache_totals`. Files: `cmd/lscpu/src/lib.rs`. Added regression
  test `bytes_flag_reaches_cache_totals_formatting`.
- `lsmem`: added `-J/--json`, `-P/--pairs`, `-o/--output`/`--output-all`
  (all 7 real columns, was hardcoded to 5), `-S/--split`, `-s/--sysroot`,
  `--summary[=never|always|only]`, and NODE/ZONES sysfs reading. Fixed
  `human_size()` to use the real util-linux 2^n scaling/rounding algorithm
  (previously diverged for non-power-of-2 sizes); fixed coalescing to split
  on NODE/ZONES when those columns are selected (previously merged rows
  that couldn't validly share a single ZONES value); fixed ZONES
  capitalization (`"none"` -> `"None"`); fixed the summary line format to
  right-align values to a fixed 38-column width; fixed `-J`/`-P` to omit
  the summary trailer by default (previously always appended it). Also
  wired `lsmem` into the multicall binary for the first time — despite
  being a complete crate, it was missing from `multicall/Cargo.toml`,
  `multicall/src/main.rs`, and `multicall/utils.list` (found during Phase
  1), so `zex-utils lsmem` did nothing until this fix. Files:
  `cmd/lsmem/src/lib.rs` (rewritten), `multicall/Cargo.toml`,
  `multicall/src/main.rs`, `multicall/utils.list`. Verified against the
  real `lsmem` binary: functional output identical across default,
  `--output-all`, `-J`, `-J -b`, `-P`, `--summary=only/never`,
  `-o RANGE,NODE,ZONES`, `-s /`. See `checklist/lscpu.md` and
  `checklist/lsmem.md` for full detail. **This closes out Phase 2.**

### Added (2026-07-27 — Phase 1, last — Phase 1 complete)
- `last`: implemented from scratch (was a completely empty, workspace-
  breaking crate — the last of the 4 P0 placeholders from `DEVPLAN.md` §0).
  Why: reconstructs login/reboot/shutdown session history from wtmp-format
  records. Files: `cmd/last/Cargo.toml` (new), `cmd/last/src/utmpx.rs` (new,
  raw record reading via `utmpxname` redirection), `cmd/last/src/lib.rs`
  (new, session-building state machine + all formatting, 9 unit tests),
  `cmd/last/src/main.rs` (new), `Cargo.toml` (workspace root — uncommented
  the `cmd/last` member, closing out the P0 placeholder list entirely),
  `multicall/Cargo.toml` (added `zex_last` dependency),
  `multicall/src/main.rs` (added to `UTIL_NAMES` + dispatch match),
  `multicall/utils.list` (registered `last`). Verified byte-for-byte
  identical to the real system `last` binary against this host's actual
  `/var/log/wtmp` (~2 months of real login/reboot/crash/clean-shutdown
  history) across default, `-x`, `-F`, `--time-format iso/notime`, `-R`,
  `-n`/`-<N>`, positional filters, and `-a`. Along the way, found and fixed
  a significant parsing bug (the reboot/shutdown marker's `ut_line` is a
  single `"~"`, not `"~~"` as `utmpdump`'s padded display suggested — this
  had silently disabled all shutdown-vs-crash detection) plus several
  empirically-derived formatting rules (duration-to-paren spacing that
  narrows by one space for multi-day durations; a fixed field width in
  `full`/`iso` mode; the `wtmp begins` trailer's format-selection rule; and
  `-a`'s fixed-column-60 host placement). One narrow, documented edge case
  remains around `-t/--until` combined with reboot pseudo-sessions — see
  `checklist/last.md`. **This closes out Phase 1** — all four originally
  build-breaking empty crates (`lsns`, `lslocks`, `lsipc`, `last`) are now
  fully implemented, tested, and wired into the multicall binary.

### Added (2026-07-27 — Phase 1, lsipc)
- `lsipc`: implemented from scratch (was a completely empty, workspace-
  breaking crate). Why: P0 in `DEVPLAN.md` — reports System V IPC (shared
  memory, semaphores, message queues) usage from `/proc/sysvipc/*` and
  `/proc/sys/kernel/*`. Files: `cmd/lsipc/Cargo.toml` (new),
  `cmd/lsipc/src/{lib,model,columns,render,main}.rs` (new, 9 unit tests),
  `Cargo.toml` (workspace root — uncommented the `cmd/lsipc` member),
  `multicall/Cargo.toml` (added `zex_lsipc` dependency),
  `multicall/src/main.rs` (added to `UTIL_NAMES` + dispatch match),
  `multicall/utils.list` (registered `lsipc`). Verified byte-for-byte
  identical to the real system `lsipc` binary (using real shm/sem/msg
  resources created via `ipcmk`) for `-r`/`-e`/`-n`/`-J` across all three
  IPC kinds and for the `-i` pretty detail view including the semaphore
  `Elements:` sub-table (fetched via real `semctl(2)` calls). Along the
  way, found and fixed a column-alignment bug (several columns had
  left/right reversed from a first guess at the reference's flags) and a
  `-i` pretty-view bug where a shared-memory segment's blank `STATUS`
  incorrectly omitted its label line entirely instead of showing it blank.
  Also made a deliberate real-parity choice to default to the global
  summary when no IPC kind flag is given, matching the real binary rather
  than the reference's degenerate empty-column behavior in that case. See
  `checklist/lsipc.md` for full detail.

### Added (2026-07-27 — Phase 1, lslocks)
- `lslocks`: implemented from scratch (was a completely empty, workspace-
  breaking crate). Why: P0 in `DEVPLAN.md` — lists local file locks by
  cross-referencing `/proc/locks` with every process's
  `/proc/<pid>/fdinfo/<fd>` `lock:` lines. Files: `cmd/lslocks/Cargo.toml`
  (new), `cmd/lslocks/src/lib.rs` (new, 16 unit tests), `cmd/lslocks/src/render.rs`
  (new, column/table/raw/json rendering, 6 unit tests), `cmd/lslocks/src/main.rs`
  (new), `Cargo.toml` (workspace root — uncommented the `cmd/lslocks`
  member), `multicall/Cargo.toml` (added `zex_lslocks` dependency),
  `multicall/src/main.rs` (added to `UTIL_NAMES` + dispatch match),
  `multicall/utils.list` (registered `lslocks`). Verified byte-for-byte
  identical to the real system `lslocks` binary in `-J`, `--output-all`,
  `-r`, `-H`, `-H -r` modes. Along the way, found and fixed a real parsing
  bug (fdinfo `lock:` lines carry the same leading `<id>: ` token as
  `/proc/locks`, contrary to an initial reading of the reference — this had
  been silently zeroing out the entire `HOLDERS` column and all
  pid/command cross-referencing) plus three smaller empirically-verified
  formatting corrections (mountinfo field offset, fallback path separator,
  blank SIZE for zero-byte locked files). See `checklist/lslocks.md` for
  full detail.

### Added (2026-07-27 — Phase 1, lsns)
- `lsns`: implemented from scratch (was a completely empty, workspace-breaking
  crate). Why: P0 in `DEVPLAN.md` — real, self-contained `/proc` namespace
  lister with no counterpart in zex-utils before now. Files:
  `cmd/lsns/Cargo.toml` (new), `cmd/lsns/src/lib.rs` (new, 8 unit tests),
  `cmd/lsns/src/main.rs` (new), `multicall/Cargo.toml` (added `zex_lsns`
  dependency), `multicall/src/main.rs` (added to `UTIL_NAMES` + dispatch
  match), `multicall/utils.list` (registered `lsns`). Verified by diffing
  output against the real system `lsns(8)` binary — matched exactly bar
  cosmetic column widths. See `checklist/lsns.md` for full detail.

### Fixed (2026-07-27)
- `nologin`: `cmd/nologin/src/lib.rs:57` called `.filter()` directly on a
  `Result` (not a valid method, `E0599`). Why: this was a **pre-existing
  compile error that broke the entire multicall binary build** — discovered
  while wiring `lsns` in and running `cargo build -p zex-utils` for the
  first time this session. Fixed by inserting `.ok()` before `.map()`.
- `mountpoint`: `cmd/mountpoint/src/lib.rs:55` assigned a `&str` into an
  `Option<String>` field via `.clone()` instead of `.to_string()` (`E0308`),
  same build-breaking impact as the `nologin` bug above. Fixed.

### Changed (2026-07-27)
- Root `Cargo.toml`: commented out the `cmd/last`, `cmd/lsipc`, `cmd/lslocks`
  workspace members (still genuinely empty crates — no code exists for them
  yet). Why: a workspace member with no `Cargo.toml` hard-fails `cargo build
  --workspace`/`-p <anything>`, so this was blocking even the `lsns` build.
  Each line will be restored as its crate gets a real implementation (Phase
  1 continues with `lslocks`, then `lsipc`, then `last`, per `DEVPLAN.md`).

### Added
- `DEVPLAN.md` — prioritized development plan for closing util-linux parity
  gaps, based on a full flag/behavior comparison of the 22 zex-utils
  utilities that have a direct counterpart in the `util-linux-main`
  (uutils/util-linux) reference tree.
- `checklist/` — one file per utility under review, tracking specific gap
  items, target files, and status.

### Findings (no code changed yet — tracked for the next implementation pass)
- Discovered `cmd/last`, `cmd/lsipc`, `cmd/lslocks`, `cmd/lsns` are empty
  crates (no `Cargo.toml`/`src`) despite being listed as workspace members in
  root `Cargo.toml`, which breaks `cargo build --workspace` / `test.sh` /
  `build-all.sh`. See `DEVPLAN.md` §0.
- Discovered `lscpu -B/--bytes` is parsed but not wired into cache-size
  formatting (silent no-op). See `checklist/lscpu.md`.
- Discovered `lsmem` is missing `-o`/`--output-all`, `-S/--split`, `-J`,
  `-P`, `-s/--sysroot`, `--summary`, and NUMA node/zone columns, plus an
  incorrect `human_size()` rounding algorithm. See `checklist/lsmem.md`.
- Minor gaps recorded for `dmesg`, `cal`, `uuidgen`, `hexdump`, `fsfreeze` —
  see their respective `checklist/*.md` files.
- Confirmed `blockdev`, `chcpu`, `ctrlaltdel`, `mcookie`, `mesg`, `mountpoint`,
  `nologin`, `renice`, `rev`, `setpgid`, `setsid` are at parity with (or, in
  `mountpoint`/`setsid`/`setpgid`'s case, better than) the reference
  implementation — no action planned.

<!--
Template for future entries once implementation work lands:

## [Unreleased]
### Added
- <utility>: implemented from scratch. Why: <reason, e.g. "P0, empty
  workspace-breaking crate">. Files: cmd/<utility>/Cargo.toml (new),
  cmd/<utility>/src/lib.rs (new), cmd/<utility>/src/main.rs (new),
  Cargo.toml (workspace member already present), multicall/Cargo.toml
  (add dependency), multicall/utils.list (register name).

### Fixed
- <utility>: <bug>. Why: <root cause>. Files: cmd/<utility>/src/lib.rs:<line>.

### Changed
- <utility>: <behavior change>. Why: <reason>. Files: <paths>.
-->
