# lscpu — FIXED (2026-07-28)

**Status:** DONE. `-B/--bytes` now correctly threaded through to cache-size formatting; verified with a regression test.

**Reference:** `util-linux-main/src/uu/lscpu/src/{lscpu.rs,sysfs.rs,main.rs}`, `lscpu.md`

## Bug

`cmd/lscpu/src/lib.rs`:
- `-B/--bytes` is parsed into `out_opts.bytes` around lines 68-75.
- But `collect()` (line 84) is called without `out_opts`, and
  `calculate_cache_totals(&cpu_topology, false)` (line 157) **hardcodes**
  `false` for the bytes flag instead of passing through the user's choice.
- Result: cache sizes always print human-readable (e.g. `"32K"`) even when
  `-B` is passed, silently ignoring the flag. Reference correctly threads
  `out_opts.bytes` into `calculate_cache_totals` (reference `lscpu.rs:157, 216-220`).

## Fix

- [x] Threaded `bytes: bool` through `collect()` and into `calculate_cache_totals` (`cmd/lscpu/src/lib.rs`)
- [x] Added `bytes_flag_reaches_cache_totals_formatting` regression test (builds a synthetic `CpuTopology` with one cache, asserts `-B`'s output starts with the raw byte count `"32768"` and non-`-B`'s doesn't)
- [x] `cargo test -p zex_lscpu` — 10/10 pass; `cargo clippy -p zex_lscpu` — clean
- [x] `CHANGELOG.md` entry added

## Minor (not blocking)
- Reference uses a regex-anchored `/proc/cpuinfo` parser (`lscpu.rs:287-299`); zex-utils uses simple line-splitting (`find_cpuinfo_value`, `lib.rs:305-315`). Functionally equivalent for well-formed input, less robust to odd formatting — no action needed unless a real malformed-`/proc/cpuinfo` bug report surfaces.
