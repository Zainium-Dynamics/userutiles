# lsmem — FIXED + wired into multicall (2026-07-28)

**Status:** DONE. All missing flags implemented, `human_size()` fixed, wired into the multicall binary. Verified against the real system `lsmem` binary — functional output (data/values) identical everywhere tested; only the fixed-width vs. real dynamic-smartcols column padding differs cosmetically (same accepted simplification as `lsns`/`lslocks`/`lsipc` from Phase 1).

**Reference:** `util-linux-main/src/uu/lsmem/src/{lsmem.rs,utils.rs,main.rs}`, `lsmem.md`

## What was added
- `-J/--json`, `-P/--pairs` output formats (hand-rolled JSON, no serde dependency, matching the project convention)
- `-o/--output <list>` / `--output-all` column selection (`RANGE, SIZE, STATE, REMOVABLE, BLOCK, NODE, ZONES` — all 7 real columns, was hardcoded to 5)
- `-S/--split <list>` — explicit split-by columns, on top of the automatic split-when-selected-as-output-column behavior (see bug fix below)
- `-s/--sysroot <dir>` — runs against an alternate root instead of hardcoded `/sys/devices/system/memory`
- `--summary[=never|always|only]` — controls table vs. summary line printing; bare `--summary` behaves like `=only` (matches real binary)
- NODE/ZONES sysfs reading (`read_node` via the `nodeN` symlink under each memory block, `valid_zones` file)

## Bugs fixed
1. **`human_size()` rounding**: replaced the ad-hoc exact-multiple-only formatter with the real util-linux 2^n scaling + one-decimal-rounding algorithm (same implementation pattern already used in `lslocks`/`lsipc`). Verified: `3355443200 -> "3.1G"`, `163840 -> "160K"` (previously would have diverged for non-power-of-2 sizes).
2. **Coalescing didn't account for NODE/ZONES**: adjacent blocks with differing NUMA node or valid-zones were being merged into one displayed row even when those columns were selected, which is wrong (a single row can't show two different ZONES values). Fixed: `coalesce()` now takes an explicit `split_keys` list, always including `STATE`/`REMOVABLE`, and adding `NODE`/`ZONES` automatically whenever they're part of the selected output columns — matches real `lsmem`'s behavior of auto-splitting ranges when those columns are requested, verified against real multi-zone output on this host (`--output-all` produces 3 rows, not 2, exactly matching real).
3. **ZONES capitalization**: the kernel's `valid_zones` file reports `"none"` lowercase but real zone names already capitalized (`"DMA32"`, `"Normal"`); real `lsmem` title-cases every word for display. Fixed with `capitalize_zones()`; verified `"none" -> "None"` against real output.
4. **Summary line format**: was a fixed 2-space gap after the label; real right-aligns the value to a **fixed total width of 38 columns** regardless of label length. Verified across all three summary lines (differing label lengths) — byte-for-byte identical after fix.
5. **`-J`/`-P` incorrectly appended the summary trailer**: real `lsmem -J`/`-P` show *only* the JSON/pairs data, no `Memory block size:` trailer, unless `--summary` is given explicitly. Fixed by tracking whether `--summary` was explicitly passed and defaulting to no-summary for JSON/pairs otherwise.
6. **Not wired into the multicall binary at all** (found during Phase 1, tracked here): despite being a fully implemented crate, `lsmem` was missing from `multicall/Cargo.toml`, `multicall/src/main.rs` (`UTIL_NAMES` + dispatch), and `multicall/utils.list` — `zex-utils lsmem` / a `lsmem` symlink did nothing. Fixed: added to all three.

## Verification performed
- [x] `cargo build -p zex_lsmem` — clean
- [x] `cargo test -p zex_lsmem` — 8 tests pass (2 new: `human_size_rounds_non_power_of_two`, `coalesce_splits_on_zones_when_requested`, plus `resolve_columns_defaults_and_output_all`, `resolve_split_keys_adds_node_zones_when_selected_as_columns`)
- [x] `cargo clippy -p zex_lsmem --all-targets` — zero warnings
- [x] `cargo build -p zex-utils` — clean, dispatch verified (`zex-utils lsmem` output diffed against real `lsmem`)
- [x] Diffed against real `lsmem` for: default, `--output-all`, `-J`, `-J -b`, `-P`, `--summary=only`, `--summary=never`, `-o RANGE,NODE,ZONES`, `-s /` — all functionally identical (data/values match; only column-width padding differs, a known accepted cosmetic simplification)

## Checklist
- [x] Add NODE/ZONES sysfs reading
- [x] Add `-o/--output` + `--output-all`
- [x] Add `-S/--split`
- [x] Add `-s/--sysroot`
- [x] Add `--summary`
- [x] Add `-J/--json`, `-P/--pairs`
- [x] Fix `human_size()` rounding/scaling
- [x] Regression tests added
- [x] Wire `lsmem` into `multicall/Cargo.toml` + `multicall/src/main.rs` + `multicall/utils.list`
- [x] `CHANGELOG.md` entry
