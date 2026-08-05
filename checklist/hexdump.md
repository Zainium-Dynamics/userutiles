# hexdump — grammar gap VERIFIED AND FIXED (2026-07-28)

**Status:** DONE. Fetched `uucore` 0.2.2's actual source (`cargo fetch` in `util-linux-main`, then read `parser/parse_size.rs` directly) and diffed its `Parser::parse` default-settings grammar against our own `parse_size_u64` field-by-field. The gap was real, not hypothetical — rewrote to match.

**Reference:** `util-linux-main/src/uu/hexdump/src/hexdump.rs` (calls `uucore::parser::parse_size::parse_size_u64` with default `Parser` settings — no allow-list, `capital_b_bytes: false`, `b_byte_count: false`), `uucore-0.2.2/src/lib/features/parser/parse_size.rs`

## What was actually missing (confirmed by reading uucore's source directly)
The prior implementation only supported `K`/`M`/`G` (binary, 1024-based) suffixes plus `0x`-hex and bare decimal. Real gaps found:
1. **`KB`/`MB`/`GB`/... (decimal/SI, 1000-based) family entirely unsupported** — `"5KB"` previously errored outright instead of meaning `5000`.
2. **`T`/`P`/`E`/`Z`/`Y`/`R`/`Q` suffixes missing** (both binary and decimal forms) — capped at `G`.
3. **`b` (block = 512) suffix missing entirely.**
4. **Octal input unsupported** — a leading-zero multi-digit number (e.g. `"077"`) should parse as octal (63), not decimal.
5. **Case-sensitivity was wrong**: the old code uppercased the whole suffix before matching, so `"Kb"`/`"kb"` would have been silently accepted as if `"KB"`/`"kB"` (1000-based) — the reference treats those as genuinely invalid (only exact-case `"KB"`/`"kB"` bind to 1000; a lone lowercase `"b"` means block=512, and there's no `"Kb"` mapping at all).
6. **Overflow was silently saturating** (`saturating_mul`) instead of erroring — the reference errors on overflow (`checked_mul`/`SizeTooBig`).
7. Bare `"B"` was accepted as `=1`; the reference only allows that under an opt-in `capital_b_bytes` setting hexdump doesn't use, so bare `"B"` should be invalid.

## Deliberately not implemented
- The `%` suffix (fraction of total physical memory, read from `/proc/meminfo`) — not meaningful for a `-n`/`-s` byte-count argument, and not something a hexdump user would plausibly type. Noted in the function's doc comment.

## Files changed
- `cmd/hexdump/src/lib.rs`: rewrote `parse_size_u64` to mirror `uucore::parser::parse_size::Parser::parse`'s exact branch structure (number-system detection, numeric/unit split, full suffix table, checked overflow). Added `parse_size_u64_matches_uucore_default_parser_grammar`, a comprehensive regression test checking each category of the fix directly against `uucore`'s own test-suite expectations (binary family, decimal family, full unit range, block suffix, octal, case-sensitivity, overflow, empty-string).

## Verification performed
- [x] `cargo test -p zex_hexdump` — 16/16 pass
- [x] `cargo clippy -p zex_hexdump --all-targets` — clean
- [x] Manual run: `hexdump -C -n 2KB <file>` (previously would have errored — `KB` was unrecognized) now correctly limits to 2000 bytes, matching the real system `/usr/bin/hexdump -C -n 2KB` byte-for-byte on this host

## Checklist
- [x] Pulled `uucore`'s `parse_size_u64` source and diffed its accepted grammar against ours
- [x] Closed the real gaps found (all 7 items above)
- [x] `CHANGELOG.md` entry added
