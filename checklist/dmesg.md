# dmesg — relative date grammar ADDED (2026-07-28)

**Status:** DONE. `--since`/`--until` now accept a bounded set of relative expressions on top of the existing fixed formats.

**Reference:** `util-linux-main/src/uu/dmesg/src/{dmesg.rs,time_formatter.rs,json.rs}`, `dmesg.md`

## Decision
Relative-date parsing is useful enough to be worth a bounded, hand-rolled extension (no external `parse_datetime`-crate dependency, matching the "no uutils stack" convention) rather than either doing nothing or building a full GNU-date-grammar parser. Scope: `"now"`, `"today"`, `"yesterday"`, and `"N <unit>[s] ago"` for second/minute/hour/day/week — the minimum bar the checklist itself proposed, and what real dmesg/date users reach for most often. Full free-form GNU date grammar (weekday names, "next tuesday", bare month-day, etc.) remains out of scope.

## What changed
- `cmd/dmesg/src/lib.rs`: `parse_datetime` now falls through to a new `parse_relative_datetime` helper when none of the fixed formats match. Case-insensitive; unrecognized input still produces the same `"invalid time value"` error as before (no behavior change for genuinely bad input).
- Added tests: `parse_datetime_accepts_now_today_yesterday`, `parse_datetime_accepts_relative_ago_forms` (tolerance-based, not exact-equality, to avoid flakiness from two independent `Local::now()` samples a few ms apart), `parse_datetime_rejects_unknown_relative_forms`.

## Verification performed
- [x] `cargo test -p zex_dmesg` — 40/40 pass
- [x] `cargo clippy -p zex_dmesg --all-targets` — clean
- [x] Manual run: `dmesg -K /dev/null --since="2 days ago"` — parses and exits 0

## Benign difference (no action needed, unchanged from prior pass)
- Boot-time source: zex-utils reads `/proc/stat`'s `btime` line; reference reads a wtmp/utmpx `BOOT_TIME` record. Both real sources; only diverges if one is stale/missing while the other isn't.

## Checklist
- [x] Product decision on relative-date parsing scope (bounded extension, decided)
- [x] Hand-rolled extended parser: `"N units ago"`, `"now"`, `"today"`, `"yesterday"`
- [x] `CHANGELOG.md` entry
