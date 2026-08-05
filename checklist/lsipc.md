# lsipc — IMPLEMENTED (2026-07-27)

**Status:** DONE. Full implementation, wired into the multicall binary, tested against real System V IPC resources (created via `ipcmk`/cleaned up via `ipcrm`) and diffed against the real system `lsipc` binary — byte-for-byte identical for `-r`/`-e`/`-n`/`-J` across all 3 kinds, and for the `-i` pretty detail view (including the semaphore `Elements:` sub-table, fetched via real `semctl(2)` calls).

**Reference:** `util-linux-main/src/uu/lsipc/src/{lsipc.rs,column.rs,display.rs,shared_memory.rs,semaphore.rs,message_queue.rs,utils.rs,errors.rs}`, `lsipc.md`, `after-help.txt`

## What it does
1. Parses `/proc/sysvipc/{shm,sem,msg}` into typed entry lists (`model.rs`), and `/proc/sys/kernel/{shmmax,shmmni,shmall,sem,msgmni,msgmnb,msgmax}` for the `-g/--global` limits.
2. Resolves the effective output column set (`columns.rs`) exactly matching the reference's `filter_defaults`/`all_defaults`/`-o` append-or-replace semantics, including the "reappend COMMAND last" quirk when `-t` is combined with `-m`.
3. Resolves owner/group names via real `getpwuid`/`getgrgid`, and creator command lines via `/proc/<pid>/cmdline` (`render.rs`).
4. Renders in 6 modes: default table, `-l/--list` (empirically identical to default), `-e/--export`, `-n/--newline`, `-r/--raw`, `-J/--json` (hand-rolled, no serde dependency), plus the `-i/--id` "pretty" title:value view with a semaphore `Elements:` sub-table.
5. For semaphores, fetches live per-semaphore `GETVAL`/`GETNCNT`/`GETZCNT`/`GETPID` values via real `semctl(2)` calls — but only when actually displaying the `-i` detail view (an efficiency improvement over the reference, which does this eagerly for every semaphore set on every listing).

## CLI surface implemented
`-b/--bytes`, `-c/--creator`, `-e/--export`, `-g/--global`, `-i/--id <id>`, `-J/--json`, `-l/--list`, `-m/--shmems`, `-n/--newline`, `--noheadings`, `--notruncate`, `-o/--output <list>` (bare = replace, `+list` = append), `-P/--numeric-perms`, `-q/--queues`, `-r/--raw`, `-s/--semaphores`, `-t/--time`, `--time-format <short|full|iso>`, `-y/--shell`, combined short opts, `-h/--help`, `--version`. Mutual exclusivity enforced: `-m`/`-q`/`-s`; `-e`/`-J`/`-l`/`-n`/`-r`; `-g` vs `-i`; `-c`/`-i`/`-t` require a kind flag.

## Files created
- `cmd/lsipc/Cargo.toml` — new crate `zex_lsipc`, deps: `zexcore`, `libc` (for `semctl`/`getpwuid`/`getgrgid`/`localtime_r`/`gettimeofday`/`sysconf`)
- `cmd/lsipc/src/model.rs` — entry structs, `/proc/sysvipc/*` + `/proc/sys/kernel/*` parsing, semaphore element fetch
- `cmd/lsipc/src/columns.rs` — column tables, default/all/filter resolution, `-o` parsing, 9 unit tests
- `cmd/lsipc/src/render.rs` — cell-value resolution, permission/size/time formatting, all 6 output renderers
- `cmd/lsipc/src/lib.rs` — CLI parsing, validation, dispatch
- `cmd/lsipc/src/main.rs` — thin binary entry point

## Files changed to wire it in
- `Cargo.toml` (workspace root) — uncommented `cmd/lsipc`
- `multicall/Cargo.toml` — added `zex_lsipc` dependency
- `multicall/src/main.rs` — added to `UTIL_NAMES` + dispatch match
- `multicall/utils.list` — registered `lsipc`

## Deliberate scope decisions (documented, not gaps)
- **No POSIX message queue (`MQUMNI`/`MQUMAX`/`MQUMNB`) rows in `-g`**: the real native `lsipc` additionally reports `/dev/mqueue`-backed POSIX message queues in its global summary; neither this port nor the `util-linux-main` (uutils) reference implements that (different subsystem, out of scope for this pass — see `DEVPLAN.md`'s 22-utility comparison boundary).
- **No `shmctl`/`semctl`/`msgctl` `IPC_INFO` syscall fallback**: only the `/proc/sysvipc/*` + `/proc/sys/kernel/*` path is implemented. This is also what the reference does as its primary path; the raw-syscall fallback only matters on a system without `/proc/sysvipc` mounted, which doesn't apply to any real Linux/Zainium target.
- **Default-to-global-summary when no kind flag is given**: the reference's `lsipc()` has a literal bug here — with no `-m`/`-q`/`-s`/`-g`, it calls `describe()` for all three kinds with an empty resolved column list (nothing would print). The real native binary instead shows the global summary by default. This port matches the **real binary's** behavior (verified), not the reference's degenerate case.

## Real bugs found and fixed during verification (empirically diffed against the live system `lsipc` binary)
1. **Column alignment was backwards for several columns**: initially guessed which columns were left- vs right-aligned; the real `KEY`/`ID` columns are left-aligned and `OWNER`/`PERMS` are right-aligned — the opposite of my first guess. Fixed by reading the reference's exact `COLUMN_INFOS` flags and matching them precisely.
2. **`STATUS` blank-line handling in the `-i` pretty view**: a shared memory segment with no status flags set was completely omitting its `Status:` line; the real binary always shows the label with a blank value for `STATUS` specifically (unlike other genuinely-absent fields like `ATTACH`/`DETACH`, which are correctly omitted). Fixed by making `STATUS` always resolve to `Some("")` rather than `None` when empty.
3. **`id {id} not found` message missing the `lsipc:` prefix** compared to the real binary. Fixed.

## Verification performed
- [x] `cargo build -p zex_lsipc` — clean
- [x] `cargo test -p zex_lsipc` — 9 tests pass
- [x] `cargo clippy -p zex_lsipc --all-targets` — zero warnings
- [x] `cargo build -p zex-utils` / `cargo build --workspace` — clean
- [x] `cargo test --workspace` — passes except the same 5 pre-existing, unrelated `cmd/trigger` failures
- [x] Created real shm/sem/msg resources via `ipcmk`, diffed against real `lsipc` for: default table (all 3 kinds), `-r`, `-e`, `-n`, `-J` (byte-for-byte identical), `-i` pretty view for all 3 kinds including semaphore elements (byte-for-byte identical), `-c`, `-t` (all 3 kinds — column order verified correct, only cosmetic width differs), `-P`, `-b`, `-o`, `-y`, `--time-format iso/full`, `-g` alone and combined with each kind flag, `-g -J`, error paths (id not found, mutual-exclusion violations)
- [x] Cleaned up test IPC resources via `ipcrm`
- [x] Verified dispatch via `zex-utils lsipc` and `zex-utils --list`

## Checklist
- [x] Parse `/proc/sysvipc/{shm,sem,msg}`
- [x] Parse `/proc/sys/kernel/*` limits
- [x] Implement `-m/-q/-s` kind selection (mutually exclusive)
- [x] Implement `-g/--global` summary
- [x] Implement `-i/--id`, `-c/--creator`, `-t/--time`
- [x] Implement `-o/--output` column selection per kind
- [x] Implement `-b/--bytes`, `-P/--numeric-perms`, `--noheadings`, `--notruncate`, `-y/--shell`
- [x] Implement output formats: `-e/--export`, `-J/--json`, `-l/--list`, `-n/--newline`, `-r/--raw` (mutually exclusive)
- [x] Implement `--time-format`
- [x] Wire into `multicall/Cargo.toml` + `multicall/utils.list` + `main.rs`
- [x] `cargo build --workspace` succeeds
- [x] Manual test: byte-for-byte diffed against real `lsipc` using live IPC resources
- [x] `CHANGELOG.md` entry added
