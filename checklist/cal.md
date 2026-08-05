# cal — FIXED (2026-07-28)

**Status:** DONE. `-y`/`-Y`/`-n` are now mutually exclusive, matching the reference's clap `ArgGroup` behavior.

**Reference:** `util-linux-main/src/uu/cal/src/{cal.rs,main.rs}`, `cal.md`

## Fix
- [x] Added a conflict check right after `cmd/cal/src/lib.rs`'s arg-parsing loop: if more than one of `-y`/`-Y`/`-n` is set, `parse_args` returns `Err("not all of -y, -Y, and -n may be used at once")`, which `run()` already routes to `ui.err(...)` + exit 1.
- [x] Added regression tests `parse_args_rejects_combined_y_and_n` and `parse_args_rejects_combined_y_and_twelve`.
- [x] `cargo test -p zex_cal` — 18/18 pass; `cargo clippy -p zex_cal` — clean.
- [x] `CHANGELOG.md` entry added.
