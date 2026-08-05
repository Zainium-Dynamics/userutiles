# Parity confirmed — no action planned

These utilities were compared flag-by-flag and behavior-by-behavior against
`util-linux-main` and found to be at parity, or better. Listed for
completeness so the gap-analysis pass is auditable; nothing here is queued
for work unless noted.

### blockdev
All 21 `ACTIONS` + `--report`/`-v`/`-q` present, identical ioctl codes/arg types, identical report line format and partition-offset lookup via `/sys/dev/block/<maj>:<min>/{partition,start}`. **Complete port.**

### chcpu
All flags (`-e/-d/-c/-g/-p/-r`) present with identical semantics; both read/write real `/sys/devices/system/cpu`; guard logic (refuse disabling last online CPU, refuse deconfiguring enabled CPU, exit code 64 on partial success) matches. **Complete port.**

### ctrlaltdel
All modes (`hard`/`soft`/query) present. zex-utils returns a graceful `Err` on an out-of-range sysctl value where the reference calls `unreachable!()` (a potential panic) — **zex-utils is more robust than the reference**, not a gap.

### mcookie
All flags (`-f/--file`, `-m/--max-size`, `-v/--verbose`) present. Only internal difference: zex-utils has an explicit `/dev/urandom`-then-`libc::rand`-fallback path; reference relies on the `rand` crate's OS RNG with no explicit fallback shown. No functional gap in normal operation.

### mesg
All flags/behavior present (`-v/--verbose`, `y`/`n` positional, exit codes 0/1/2). zex-utils accepts uppercase `Y`/`N` in addition to lowercase — a superset, not a gap.

### mountpoint
zex-utils implements `-q/--quiet`, `-d/--fs-devno`, `-x/--devno`, `-n/--nofollow` — **the reference implements none of these**, despite its own `.md` documenting `-d`/`-q`/`-x`. Additionally:
- Reference's mountpoint-detection heuristic hardcodes root inode `2` (`inode == 2 || parent.dev() != dev()`), which isn't universal (e.g. btrfs subvolumes). zex-utils correctly compares the directory's own inode to its parent's inode.
- Reference's `uumain` always returns `Ok(())` regardless of result, so its process exit code is always 0 even when the path isn't a mountpoint — diverging from real `mountpoint(1)` semantics. zex-utils correctly returns 0/1/32 per case.
**zex-utils exceeds the reference here; no action needed.**

### nologin
All compatibility no-op flags present (`-c`, `--init-file`, `-i`, `-l`, `--noprofile`, `--norc`, `--posix`, `--rcfile`, `-r`). Both read `/etc/nologin.txt` with the same fallback message. zex-utils treats an empty-but-present `/etc/nologin.txt` as "use default message" (`.filter(|s| !s.is_empty())`); reference only falls back on read *error*, so an empty file would print a blank line under the reference. Minor edge case, zex-utils's behavior is arguably closer to real util-linux. No action needed.

### renice
zex-utils supports full `-n/--priority`, `-g/--pgrp`, `-p/--pid`, `-u/--user`, multiple identifiers per invocation, username resolution via `getpwnam`, and prints old/new priority per target. **The reference only accepts two positional args (nice_value, pid) hardcoded to `PRIO_PROCESS`** — it doesn't implement `-g`/`-p`/`-u` despite its own `.md` documenting `[-g|-p|-u] identifier...`. zex-utils substantially exceeds the reference; no action needed.

### rev
Both support `-0/--zero` and file args; zex-utils additionally handles `-` for explicit stdin. One design-level difference worth being aware of (not a bug to fix reflexively):
- zex-utils reverses by Unicode scalar value for valid UTF-8 input, preserving multi-byte characters intact, falling back to raw bytes only for invalid UTF-8 (`cmd/rev/src/lib.rs:13-18`).
- Reference always does a raw byte-wise reverse (`rev.rs:57,61`), which scrambles multi-byte UTF-8 sequences into invalid byte garbage.
zex-utils's behavior is more correct for UTF-8 locales; keep as-is. No action needed.

### setpgid
Both support `-f/--foreground` and passthrough command+args; both call the real `setpgid(0,0)` and `tcsetpgrp`/`getpgrp` on `/dev/tty`. Only difference: zex-utils maps exec failures to precise exit codes (127 not-found, 126 permission-denied, 1 other) matching real `setpgid(1)`/shell conventions; reference always returns 1 regardless of error kind. zex-utils is more precise; no action needed.

### setsid
Both implement `-c/--ctty`, `-f/--fork`, `-w/--wait`. **Reference has a latent bug**: on the fork path, if `spawn()` fails and `-w/--wait` was not given, the exit-code-setting call is invoked with `set_error = wait_child = false`, so exec failure silently exits 0. zex-utils always sets the correct exit code on exec/spawn failure regardless of `-w`. zex-utils fixes a real reference bug; no action needed.
