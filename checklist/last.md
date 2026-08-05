# last — IMPLEMENTED (2026-07-27)

**Status:** DONE. Full implementation, wired into the multicall binary, tested against this system's **real** `/var/log/wtmp` login/reboot/crash history spanning almost 2 months — byte-for-byte identical to the real `last` binary across default, `-x`, `-F`/full, `--time-format iso`, `--time-format notime`, `-R`, `-n`/`-<N>`, positional tty/username filters, and `-a` (after a fix).

**Reference:** `util-linux-main/src/uu/last/src/{last.rs,main.rs,platform/*.rs}`, `last.md`

## What it does
1. Reads wtmp-format records via the standard glibc utmpx API (`setutxent`/`getutxent`/`endutxent`), redirected to an arbitrary file via `utmpxname(3)` (default `/var/log/wtmp`, overridable with `-f`) — the same real mechanism `who(1)` already uses in this codebase, just pointed at the historical log instead of the live `/var/run/utmp`.
2. Reconstructs login/reboot/shutdown **sessions** from the raw record stream (`build_sessions`), tracking each open `USER_PROCESS` by pid until a matching `DEAD_PROCESS` (normal logout), a shutdown marker (`RUN_LVL`, `ut_user=="shutdown"`, `ut_line=="~"` — closes as **"down"**), or the next `BOOT_TIME` with no shutdown marker in between (closes as **"crash"**). Reboot/shutdown pseudo-sessions get the same treatment against each other.
3. Renders in 4 time formats (`short` default, `full`/`-F`, `iso`, `notime`) with real `localtime_r`/`mktime`/`getnameinfo` (for `-d`) calls — no fake data anywhere.
4. Supports `-s`/`-t` (since/until) filtering, including the real binary's `"gone - no logout"` state for a login session whose real resolution is known to occur after the queried `until` boundary (verified).

## CLI surface implemented
`-f/--file <file>`, `-x/--system`, `-d/--dns`, `-a/--hostlast`, `-R/--nohostname`, `-n/--limit <N>` (and bare `-<N>`), `-F/--fulltimes`, `--time-format <notime|short|full|iso>`, `-s/--since <date>`, `-t/--until <date>`, positional `[username...] [tty...]` filters, `-h/--help`, `--version`.

**Intentionally out of scope** (real native `last` extras not in the `util-linux-main` reference's documented CLI either): `-p/--present`, `-w/--fullnames`, `-T/--tab-separated`, `-i/--ip`.

## Files created
- `cmd/last/Cargo.toml` — new crate `zex_last`, deps: `zexcore`, `libc`
- `cmd/last/src/utmpx.rs` — raw record reading via `utmpxname`+`setutxent`/`getutxent`/`endutxent`
- `cmd/last/src/lib.rs` — session-building state machine, all formatting, CLI parsing, 9 unit tests
- `cmd/last/src/main.rs` — thin binary entry point

## Files changed to wire it in
- `Cargo.toml` (workspace root) — uncommented `cmd/last` (the last of the 4 P0 placeholders)
- `multicall/Cargo.toml` — added `zex_last` dependency
- `multicall/src/main.rs` — added to `UTIL_NAMES` + dispatch match
- `multicall/utils.list` — registered `last`

## Real bugs found and fixed during verification (empirically diffed against this system's actual `/var/log/wtmp`, ~2 months of real login/reboot/crash history)
1. **Reboot/shutdown marker line was `"~~"`, should be `"~"`**: `utmpdump`'s padded display (`[~~  ]`) made it look like the `ut_line` field literally contains two tildes; reading the raw parsed field directly (via a temporary debug test) showed it's actually a single `"~"`. This silently disabled *all* shutdown-marker detection, making every reboot show as `"crash"` even when a clean shutdown record existed. This was the most significant bug — fixed by checking `r.line == "~"`.
2. **Duration-to-parenthesis spacing in `short` format**: the gap before `(duration)` is 1 space narrower when the duration spans multiple days (`"D+HH:MM"`) than for a same-day `"HH:MM"`/`"down"`/`"crash"` — verified across several real single- and multi-day sessions and reproduced exactly (`duration_paren_gap`).
3. **`full`/`iso` format field width**: the `down`/`crash`/end-datetime field pads to a *fixed width* equal to the login-time string's own length (24 for `full`, 25 for `iso`) plus the same 1-or-2 gap rule above — a different rule than `short` format's. Verified across `down`, `crash`, and normal-end cases.
4. **`notime` mode duration spacing**: same 1-space-narrower-for-multi-day rule as `short` format.
5. **`wtmp begins` trailer**: initially always used `full` format; real always omits it entirely under `notime`, and switches to `iso` when that's the active `--time-format` — otherwise (including plain `short`) it stays `full`. Fixed.
6. **`-a/--hostlast` column position**: initially appended `"   {host}"` with a fixed 3-space gap; real actually pads the whole row (everything before the host) to a **fixed total width of 60 columns** before appending the untruncated host — verified across four differently-shaped rows, all landing the host at column 60. Fixed.

## Known minor unresolved edge case
Combining `-t/--until` with a **reboot pseudo-session** whose real resolution (crash/down) falls *before* the `until` boundary can still show `"still running"` in the real binary instead of its resolved state, for the second-to-last reboot before the cutoff specifically — this looks like a genuine quirk/limitation in real `last`'s own boot-pairing algorithm under `-t`, and the exact rule couldn't be cleanly reverse-engineered from observed behavior alone. `-s`/`-t` filtering and the `"gone - no logout"` state for **user** sessions are verified correct; this residual gap is narrow (a specific pseudo-session state under a specific flag combination) and documented here rather than chased further.

## Verification performed
- [x] `cargo build -p zex_last` — clean
- [x] `cargo test -p zex_last` — 9 tests pass
- [x] `cargo clippy -p zex_last --all-targets` — zero warnings
- [x] `cargo build -p zex-utils` / `cargo build --workspace` — clean
- [x] `cargo test --workspace` — passes except the same 5 pre-existing, unrelated `cmd/trigger` failures
- [x] Diffed against real `last` on real `/var/log/wtmp` (~2 months of history, multiple real reboots/crashes/clean-shutdowns) for: default, `-x`, `-F`, `--time-format iso`, `--time-format notime`, `-R`, `-n 5`, `-5`, positional `tty2`/`reboot` filters, `-a` — all byte-for-byte identical
- [x] Verified dispatch via `zex-utils last` and `zex-utils --list`

## Checklist
- [x] Implement utmpx struct + reader (via `utmpxname` redirection, not a hand-rolled binary parser — simpler and exactly matches glibc's own record layout)
- [x] Implement session pairing (login → matching logout) and duration computation
- [x] Implement reboot/shutdown/crash detection
- [x] Implement `-f`, `-x`, `-d`, `-a`, `-R`, `-n`, `--time-format`, `-s`/`-t`, positional filters
- [x] Wire into `multicall/Cargo.toml` + `multicall/utils.list` + `main.rs`
- [x] `cargo build --workspace` succeeds
- [x] Manual test against real `/var/log/wtmp`
- [x] `CHANGELOG.md` entry added
