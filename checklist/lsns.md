# lsns — IMPLEMENTED (2026-07-27)

**Status:** DONE. Full implementation, wired into the multicall binary, tested against real `/proc` and diffed against the real system `lsns` binary (output matched exactly modulo column widths and natural process-count drift between two separate invocations).

**Reference:** `util-linux-main/src/uu/lsns/src/{lsns.rs,errors.rs,smartcols.rs,main.rs}`, `lsns.md`

## What it does
`lsns` lists Linux namespaces (`cgroup`, `ipc`, `mnt`, `net`, `pid`, `user`, `uts`, `time`) and which processes belong to each, by:
1. Walking `/proc/[pid]` for every numeric entry (`list_processes`).
2. Reading `/proc/[pid]/ns/{cgroup,ipc,mnt,net,pid,user,uts,time}` — `fs::metadata(...).st_ino()` on each gives the namespace's inode (its identity).
3. Reading `/proc/[pid]/cmdline` (NUL bytes replaced with spaces, matching real `lsns`/`ps` display), falling back to bracketed `/proc/[pid]/comm` for kernel threads/zombies (`read_process_command`).
4. Grouping processes by shared namespace inode; representative PID = lowest PID in the group (`collect_assigned_namespaces`).
5. Discovering **persistent** namespaces (no attached process) by parsing `/proc/self/mountinfo` for `nsfs` mount entries (`add_persistent_namespaces` + `parse_nsfs_root`).
6. Printing a table: `NS TYPE NPROCS PID USER COMMAND`, sorted by namespace inode.

## CLI surface implemented
- `-n`, `--noheadings` — suppress the table header
- `-P`, `--persistent` — list only namespaces with no attached process
- combined short opts (`-nP`), `-h`/`--help`, `--version`

## Files created
- `cmd/lsns/Cargo.toml` — new crate `zex_lsns`, deps: `zexcore`, `libc` (for `getpwuid`)
- `cmd/lsns/src/lib.rs` — full implementation + 8 unit tests (namespace grouping, mountinfo parsing, cmdline-joining edge cases)
- `cmd/lsns/src/main.rs` — thin binary entry point

## Files changed to wire it in
- `Cargo.toml` (workspace root) — `cmd/lsns` was already a member; also **commented out** `cmd/last`, `cmd/lsipc`, `cmd/lslocks` (still genuinely empty — see their own checklists) so `cargo build --workspace`/`cargo test --workspace` can run again. Re-add each line as it gets implemented.
- `multicall/Cargo.toml` — added `zex_lsns = { path = "../cmd/lsns" }`
- `multicall/src/main.rs` — added `"lsns"` to `UTIL_NAMES` and `"lsns" => zex_lsns::run()` to the dispatch match
- `multicall/utils.list` — added `lsns` entry

## Incidental fixes (pre-existing bugs found while getting `cargo build -p zex-utils` to compile at all)
- `cmd/nologin/src/lib.rs:57` — `.filter()` was called directly on a `Result`, which doesn't have that method (E0599). Fixed by inserting `.ok()` before `.map()`/`.filter()`. This was breaking the multicall binary build **regardless of lsns** — `zex-utils` (the actual product) did not compile before this fix.
- `cmd/mountpoint/src/lib.rs:55` — `Some(other.clone())` where `other: &str` but the field is `Option<String>` (E0308). Fixed to `Some(other.to_string())`. Same build-breaking impact as above.

## Verification performed
- [x] `cargo build -p zex_lsns` — clean
- [x] `cargo test -p zex_lsns` — 8/8 pass
- [x] `cargo clippy -p zex_lsns --all-targets` — zero warnings
- [x] `cargo build -p zex-utils` (multicall) — clean (only pre-existing unrelated unused-import warnings in other crates)
- [x] `cargo build --workspace` — clean
- [x] `cargo test --workspace` — passes except 5 pre-existing, unrelated `cmd/trigger` test failures (environment-dependent app/handler discovery tests that fail in this sandbox because no desktop apps are installed — not caused by this change, not part of the util-linux comparison scope)
- [x] Manual run against real `/proc` on this host — output diffed against the real system `/usr/bin/lsns`: identical NS/TYPE/NPROCS/PID/USER/COMMAND data (only cosmetic column-width differences, since real `lsns` uses libsmartcols dynamic sizing and this uses fixed-width `format!`, matching the project's existing convention e.g. `lsmem`)
- [x] Verified dispatch via `zex-utils lsns`, `zex-utils --list`, and direct symlink invocation

## Checklist
- [x] Create `cmd/lsns/Cargo.toml`
- [x] Implement `/proc/[pid]/ns/*` inode reading + grouping
- [x] Implement persistent-namespace discovery via `/proc/self/mountinfo`
- [x] Implement `-n/--noheadings`
- [x] Implement `-P/--persistent`
- [x] Table output matches reference column set
- [x] Wire into `multicall/Cargo.toml` + `multicall/utils.list` + `main.rs`
- [x] `cargo build --workspace` succeeds
- [x] Manual test: compared against real system `lsns`
- [x] `CHANGELOG.md` entry added
