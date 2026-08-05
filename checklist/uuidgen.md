# uuidgen — Gap 2 FIXED, Gap 1 decided (2026-07-28)

**Status:** DONE. Mutual-exclusivity validation added; the v1-node-ID privacy question decided in favor of the status quo.

**Reference:** `util-linux-main/src/uu/uuidgen/src/uuidgen.rs`, `uuidgen.md`

## Gap 1: v1 UUID node ID is never a real MAC address — DECIDED: keep current behavior
- Decision: keep the privacy-safe random+multicast-bit node ID (status quo). Matching the reference/real `uuidgen` here would mean embedding a stable hardware identifier (real MAC) into every generated v1 UUID — a known, upstream-acknowledged privacy leak, not a strict improvement. No code change; this closes the item as "intentionally not matching the reference."

## Gap 2: no mutual-exclusivity validation across mode flags — FIXED
- Added a check right after `cmd/uuidgen/src/lib.rs`'s arg-parsing loop: if more than one of `-r`/`-t`/`-m`/`-s` is set, errors with `"--random, --time, --md5, and --sha1 are mutually exclusive"` and exits 1, instead of silently letting `-t` win.
- `run()` reads `std::env::args()` directly (no separate testable `parse_args`, unlike `cal`/`dmesg`), so this was verified via a manual binary run (`uuidgen -r -t` → error, exit 1) rather than a unit test.
- `cargo build -p zex_uuidgen` / `clippy` — clean.

## Checklist
- [x] Product decision on Gap 1 (keep status quo)
- [x] Implement Gap 2 mutual-exclusivity check
- [x] `CHANGELOG.md` entry
