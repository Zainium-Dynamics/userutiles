# lslocks — IMPLEMENTED (2026-07-27)

**Status:** DONE. Full implementation, wired into the multicall binary, tested against real locks on this host and diffed against the real system `lslocks` binary — **byte-for-byte identical** in `-J`/`--output-all`/`-r`/`-H`/`-H -r` modes once cosmetic column-width differences are normalized (default table mode uses fixed widths; real `lslocks` uses libsmartcols dynamic terminal-width sizing).

**Reference:** `util-linux-main/src/uu/lslocks/src/{lslocks.rs,column.rs,display.rs,utils.rs,smartcols.rs,errors.rs}`, `lslocks.md`, `columns.{json,raw,txt}`

## What it does
1. Parses `/proc/locks` (`collect_proc_locks`) — one entry per system lock, with an `<id>: [-> ]<class> <mandatory> <mode> <pid> <maj>:<min>:<inode> <start> <end>` format.
2. Parses every `/proc/<pid>/fdinfo/<fd>` `lock:` line for every process (`collect_pid_locks`) — same field format, used for `HOLDERS` and to recover a `/proc/locks` entry's true owner when its pid field doesn't resolve locally.
3. Resolves each lock's `PATH`/`SIZE` by scanning the owning process's `/proc/<pid>/fd/*` for a symlink whose target inode matches (`find_fd_path_and_size`); falls back to `"<mountpoint>..."` (via `/proc/self/mountinfo`) when unresolvable, or drops the row entirely under `-i/--noinaccessible`.
4. Computes `BLOCKER` (the non-blocked lock sharing the same `/proc/locks` id) and `HOLDERS` (`pid,command,fd` for every fdinfo-sourced lock matching the same range/inode/device/mandatory/blocked/kind/mode) at render time.
5. Renders in three modes: fixed-width text table, `-r/--raw` (space-separated, `\xNN`-escaped whitespace), `-J/--json` (hand-rolled, no serde dependency, matching real key/type shapes: numbers, booleans, `null`, and a JSON array for `HOLDERS`).

## CLI surface implemented
`-b/--bytes`, `-i/--noinaccessible`, `-J/--json`, `-r/--raw` (mutually exclusive), `-n/--noheadings`, `-o/--output <list>` (supports bare list = replace defaults, `+list` = append to defaults/all), `--output-all`, `-p/--pid <pid>`, `-u/--notruncate`, `-H/--list-columns` (all 3 output-mode variants), combined short opts, `-h/--help`, `--version`.

## Files created
- `cmd/lslocks/Cargo.toml` — new crate `zex_lslocks`, only dep is `zexcore` (no libc needed)
- `cmd/lslocks/src/lib.rs` — CLI parsing, `/proc` parsing/resolution, 16 unit tests
- `cmd/lslocks/src/render.rs` — column definitions, `Cell` type (typed for JSON correctness), table/raw/json rendering, `-H` column reference, 6 unit tests
- `cmd/lslocks/src/main.rs` — thin binary entry point

## Files changed to wire it in
- `Cargo.toml` (workspace root) — uncommented `cmd/lslocks` (was disabled as a P0 placeholder when `lsns` was wired in)
- `multicall/Cargo.toml` — added `zex_lslocks = { path = "../cmd/lslocks" }`
- `multicall/src/main.rs` — added `"lslocks"` to `UTIL_NAMES` and the dispatch match
- `multicall/utils.list` — added `lslocks` entry

## Real bugs found and fixed during verification (empirically diffed against the live system `lslocks` binary)
1. **fdinfo `lock:` line off-by-one**: assumed (from a literal reading of the reference) that fdinfo lines have no leading `<id>: ` token; a live `/proc/<pid>/fdinfo/<fd>` read showed they do (`lock:\t1: FLOCK  ADVISORY  WRITE 2268 ...`). This silently broke *all* fdinfo-sourced lock parsing (every field shifted by one, causing a downstream integer-parse failure that dropped every entry) — meaning `HOLDERS` was always empty and pid/command cross-referencing never had anything to search. Fixed by always consuming the leading token, only using it as `id` on the `/proc/locks` (non-fdinfo) path. Added a regression test (`parse_fdinfo_lock_line_without_id_prefix_fails`).
2. **`fallback_file_name` mountinfo field offset**: was reading the mountinfo `root` field (index 3) instead of `mount point` (index 4), producing `/...` instead of `/run...` for unresolvable locks. Fixed to skip one more field.
3. **`fallback_file_name` separator**: initially inserted an extra `/` before `...` when the mount point didn't already end in one (e.g. `/run/...`); empirically the real binary emits `/run...` with no separator. Fixed to concatenate directly.
4. **SIZE for a zero-byte file**: real `lslocks` shows a blank SIZE cell (not `"0"`/`"0B"`) when the locked file is 0 bytes, in both human and `--bytes` modes — verified against a real 0-byte pipewire lock file. Implemented as `None | Some(0) => blank`.
5. **PATH/HOLDERS truncation only applies on a real terminal**: initially added a flat 100-char cap; empirically the real binary never truncates when stdout is piped (only when it's an interactive tty). Reworked `truncate()` to take an explicit `is_tty` (queried once via `std::io::IsTerminal` at the real call site, passed as a plain bool so the function stays unit-testable).

## Verification performed
- [x] `cargo build -p zex_lslocks` — clean
- [x] `cargo test -p zex_lslocks` — 16 + 6 = 22 tests pass
- [x] `cargo clippy -p zex_lslocks --all-targets` — zero warnings
- [x] `cargo build -p zex-utils` / `cargo build --workspace` — clean
- [x] `cargo test --workspace` — passes except the same 5 pre-existing, unrelated `cmd/trigger` environment-dependent failures noted in `checklist/lsns.md`
- [x] Diffed against real system `lslocks` for: default table, `--output-all`, `-b`, `-r`, `-J`, `-J --output-all` (byte-for-byte identical), `-H`, `-H -r` (byte-for-byte identical), `-H -J` (valid JSON, structurally matching), `-o COMMAND,PID,PATH`, `-p <pid>`
- [x] Verified dispatch via `zex-utils lslocks` and `zex-utils --list`

## Checklist
- [x] Create `cmd/lslocks/Cargo.toml`
- [x] Parse `/proc/locks`
- [x] Resolve PATH via `/proc/<pid>/fd` + fallback via mountinfo
- [x] Resolve COMMAND via cross-referencing fdinfo locks
- [x] Implement blocker/holder relationship logic
- [x] Implement `-b/--bytes`, `-i/--noinaccessible`, `-n/--noheadings`, `-p/--pid`, `-u/--notruncate`
- [x] Implement `-o/--output` + `--output-all` column selection
- [x] Implement `-H/--list-columns`
- [x] Implement `-J/--json` and `-r/--raw` (mutually exclusive)
- [x] Wire into `multicall/Cargo.toml` + `multicall/utils.list` + `main.rs`
- [x] `cargo build --workspace` succeeds
- [x] Manual test: byte-for-byte diffed against real `lslocks`
- [x] `CHANGELOG.md` entry added
