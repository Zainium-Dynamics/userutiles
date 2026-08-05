# Checklist index — util-linux parity pass

**Phase 1 (all 4 P0 build-breaking empty crates) is complete as of 2026-07-27.**
**Phase 2 (P1 bug fixes: `lscpu`, `lsmem`) is complete as of 2026-07-28.**
**Phase 3 (P2 items: `dmesg`, `uuidgen`, `cal`) is complete as of 2026-07-28.**
**Phase 4 (P3 items: `hexdump`, `fsfreeze`) is complete as of 2026-07-28 —
the originally-scoped 22-utility util-linux comparison pass is now fully
closed out.** See `../DEVPLAN.md` for what's next.

Generated 2026-07-27 alongside `../DEVPLAN.md`. Each file below tracks one
utility: what's missing vs. the `util-linux-main` reference, exactly which
files would need to change, and why. Checkboxes are unchecked until code
actually lands — flip them and add a note (+ a `CHANGELOG.md` entry) when
implementing.

| Utility | Priority | Status | File |
|---|---|---|---|
| `lsns` | P0 | **DONE** (2026-07-27) — implemented, tested, wired into multicall | [lsns.md](lsns.md) |
| `lslocks` | P0 | **DONE** (2026-07-27) — implemented, byte-for-byte diffed vs real binary | [lslocks.md](lslocks.md) |
| `lsipc` | P0 | **DONE** (2026-07-27) — implemented, byte-for-byte diffed vs real binary | [lsipc.md](lsipc.md) |
| `last` | P0 | **DONE** (2026-07-27) — implemented, byte-for-byte diffed vs real binary on real wtmp history | [last.md](last.md) |
| `lscpu` | P1 | **DONE** (2026-07-28) — `-B/--bytes` bug fixed | [lscpu.md](lscpu.md) |
| `lsmem` | P1 | **DONE** (2026-07-28) — flags added, bugs fixed, wired into multicall | [lsmem.md](lsmem.md) |
| `dmesg` | P2 | **DONE** (2026-07-28) — bounded relative-date parsing added | [dmesg.md](dmesg.md) |
| `uuidgen` | P2 | **DONE** (2026-07-28) — mutual-exclusivity fixed, node-ID gap decided | [uuidgen.md](uuidgen.md) |
| `cal` | P2 | **DONE** (2026-07-28) — mutual-exclusivity fixed | [cal.md](cal.md) |
| `hexdump` | P3 | **DONE** (2026-07-28) — real grammar gap found (via uucore source) + fixed | [hexdump.md](hexdump.md) |
| `fsfreeze` | P3 | **DONE** (2026-07-28) — decided to keep current (more correct) behavior | [fsfreeze.md](fsfreeze.md) |
| `blockdev`, `chcpu`, `ctrlaltdel`, `mcookie`, `mesg`, `mountpoint`, `nologin`, `renice`, `rev`, `setpgid`, `setsid` | — | at parity or better | [parity-confirmed.md](parity-confirmed.md) |

Not reviewed in this pass (no util-linux-main counterpart): the other ~133
zex-utils utilities (coreutils/findutils/checksum family/next-gen tools).
