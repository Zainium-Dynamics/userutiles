# fsfreeze — DECIDED, no change (2026-07-28)

**Status:** implemented, full flag set (`-f/--freeze`, `-u/--unfreeze`, mountpoint operand). One behavioral divergence, likely not worth "fixing".

**Reference:** `util-linux-main/src/uu/fsfreeze/src/{fsfreeze.rs,main.rs}`, `fsfreeze.md`

## Divergence
- zex-utils (`cmd/fsfreeze/src/lib.rs:50-59`) returns exit code `1` when the `FIFREEZE`/`FITHAW` ioctl fails.
- Reference (`fsfreeze.rs:33-39`) calls `uucore::show_error!` on ioctl failure but still returns `Ok(())` from `uumain`, so the reference tool actually exits `0` even when freeze/unfreeze fails.
- zex-utils's behavior (non-zero exit on real failure) is arguably more correct/useful for scripting than the reference's.

## Decision
- [x] **Keep zex-utils's current behavior** (exit 1 on real ioctl failure). No byte-for-byte reference exit-code parity requirement exists anywhere in this project; the reference's exit-0-on-failure is arguably a bug (a caller scripting against `fsfreeze` can't detect a failed freeze/unfreeze), and zex-utils's current behavior is strictly more useful. No code change.
- [x] `CHANGELOG.md` note added.
