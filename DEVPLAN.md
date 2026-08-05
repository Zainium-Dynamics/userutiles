# user_utils (formerly zex-utils) — Development Plan (util-linux parity gap closure)

> **2026-08-05:** project renamed `zex-utils` → `user_utils` and the
> `multicall/` dispatcher binary was removed (discrete per-tool binaries
> only). Entries below predate this and refer to the project by its old
> name and the now-removed multicall binary — left as-is, they're an
> accurate record of what was true when each entry was written.

**Date:** 2026-07-27
**Author of this pass:** Claude Code (gap-analysis + planning session, no code changed yet)
**Reference tree used:** `/run/media/alizain/ZAINIUM_DRIVE/util-linux-main` (uutils/util-linux, Rust reimplementation using `uucore`+`clap`)
**Scope of this pass:** the 22 zex-utils utilities that have a direct counterpart in util-linux-main:
`blockdev, cal, chcpu, ctrlaltdel, dmesg, fsfreeze, hexdump, last, lscpu, lsipc, lslocks, lsmem, lsns, mcookie, mesg, mountpoint, nologin, renice, rev, setpgid, setsid, uuidgen`

This document is the living plan. Per-utility work items live in `checklist/`.
Once code actually changes, log it in `CHANGELOG.md` — this file only tracks *intent and priority*.

---

## Progress log

**2026-07-28 — Phase 4 COMPLETE. Original 22-utility comparison pass fully
closed out.** `hexdump`'s size-suffix grammar gap turned out to be real
(confirmed by fetching and reading `uucore` 0.2.2's actual source, not just
inferring from the reference's call site) — fixed with a faithful port of
`uucore::parser::parse_size::Parser::parse`'s grammar. `fsfreeze`'s
exit-code question was decided in favor of keeping the current (more
correct) behavior, no code change. Every P0/P1/P2/P3 item from the original
scope (`lsns`, `lslocks`, `lsipc`, `last`, `lscpu`, `lsmem`, `dmesg`,
`uuidgen`, `cal`, `hexdump`, `fsfreeze`) is now done. See
`checklist/hexdump.md` and `checklist/fsfreeze.md`.

**What's left, if continuing:** the "Non-goals" scope boundary from §4 below
— the other ~133 zex-utils utilities (coreutils/findutils/checksum
family/next-gen tools) were never compared against anything in this pass,
since util-linux-main only covers the 22 utilities already handled. A
follow-up pass would need a different reference point (e.g. GNU
coreutils/findutils behavior) to have something to diff against. Also
still open: the project-wide broken-pipe-panic finding logged during Phase
1 (`println!` panics instead of exiting cleanly when piped into something
like `head`) — out of scope for a quick fix (100+ crates) but worth a
dedicated pass.

**2026-07-28 — Phase 3 COMPLETE.** All three P2 items done: `cal` and
`uuidgen` now reject conflicting mode flags instead of silently
prioritizing one (matching the reference's clap `ArgGroup` behavior);
`dmesg --since`/`--until` gained a bounded relative-date grammar
(`now`/`today`/`yesterday`/`"N units ago"`). Also made an explicit product
call on `uuidgen`'s v1-node-ID privacy question: keep the current
privacy-safe random node ID rather than match the reference's real-MAC
lookup. All changes verified via `cargo test`/`clippy` per-crate plus
manual binary runs; `cargo build --workspace` stays clean. See
`checklist/cal.md`, `checklist/uuidgen.md`, `checklist/dmesg.md`.

**Next: Phase 4** — the two remaining P3 verification items: `hexdump`
(diff `uucore::parse_size_u64`'s accepted suffix grammar against ours —
unverified, may be a non-issue) and `fsfreeze` (confirm whether byte-for-byte
reference exit-code parity is actually required anywhere, or keep the
current arguably-more-correct behavior). Both are low-risk/low-priority;
after those, the originally-scoped 22-utility util-linux comparison pass
is fully closed out.

**2026-07-28 — Phase 2 COMPLETE.** `lscpu`'s `-B/--bytes` no-op bug fixed
(one-line thread-through + regression test). `lsmem` got all its missing
flags (`-J`, `-P`, `-o`/`--output-all`, `-S`, `-s`, `--summary`) plus five
real bugs fixed (`human_size()` rounding, NODE/ZONES-aware coalescing,
ZONES capitalization, summary-line width, `-J`/`-P` trailer suppression),
and — the P1 finding from Phase 1 — was finally wired into the multicall
binary (it was a complete crate that had simply never been registered).
Verified against the real `lscpu`/`lsmem` binaries; functional output
matches everywhere tested, only fixed-width-vs-dynamic-smartcols column
padding differs cosmetically (same accepted simplification from Phase 1).
See `checklist/lscpu.md` and `checklist/lsmem.md`.

**Next: Phase 3** — P2 items: `dmesg` (`--since`/`--until` date grammar,
pending a product decision on scope), `uuidgen` (2 minor gaps), `cal`
(mutual-exclusivity validation). See `DEVPLAN.md` §1 P2 tier.

**2026-07-27 — Phase 1 COMPLETE.** All 4 originally build-breaking empty
crates (`lsns`, `lslocks`, `lsipc`, `last`) are implemented, tested, and
wired into the multicall binary. Every one was verified by diffing actual
output against the real system binary (not just against the uutils
reference) — `lsns`/`lslocks`/`lsipc` using synthetic/live resources
created for the purpose, `last` against this host's own ~2-month real
`/var/log/wtmp` history. Each implementation surfaced at least one real bug
that a read-through of the reference alone wouldn't have caught (see each
utility's `checklist/*.md` "Real bugs found" section) — empirical
diffing against the actual binaries was consistently more valuable than
trusting the reference or first-draft assumptions. Two more bugs unrelated
to any of the four were also found and fixed along the way, in
`cmd/nologin` and `cmd/mountpoint`, which had been silently breaking the
`cargo build -p zex-utils` multicall binary before this pass.

**Next: Phase 2** — `lscpu`'s `-B/--bytes` no-op bug, then `lsmem`'s
missing flags + formatting bug (including its own not-wired-into-multicall
finding). See `checklist/lscpu.md` and `checklist/lsmem.md`.

**New finding (2026-07-28, not yet actioned, project-wide not just Phase 1):**
piping any of `lsns`/`lslocks`/`lsipc`/`last` (and pre-existing `lsmem`, at
minimum) into a pipe reader that closes early — e.g. `lsmem | head -1` —
crashes with a Rust panic (exit 101) on the broken-pipe write error instead
of exiting cleanly, which is what every real Unix tool does. `core/src/ui.rs`
already has `write_stdout`/`flush_stdout` helpers that map a broken-pipe
write error to success, but a repo-wide grep shows **zero** `cmd/*` crates
actually use them — every utility just calls `println!`/`print!` directly,
which panics on a broken pipe. This violates the README's own "Correct for
pipelines" design principle project-wide, not just in code from this phase.
Out of scope to fix broadly here (would touch on the order of 100+ crates);
flagging so a future pass can either (a) make `println!`-panic-on-broken-pipe
a lint/CI check, or (b) route the hot print loops through the existing safe
helpers, prioritizing the tools most commonly used in pipelines first
(`ls`, `cat`, `find`, the checksum family, etc.).

**2026-07-27 — Phase 1, step 3 (`lsipc`): DONE.** Implemented, wired into
multicall, byte-for-byte diffed against the real system `lsipc` binary
(using real shm/sem/msg resources created via `ipcmk`, cleaned up
afterward) across default/`-r`/`-e`/`-n`/`-J`/`-i` modes for all three IPC
kinds. Fixed a column-alignment bug and a `-i` pretty-view STATUS-blanking
bug found during verification. Made one deliberate real-parity choice
(default-to-global-summary when no kind flag given) documented in
`checklist/lsipc.md`. Only Phase 1 item remaining: `last`.

**2026-07-27 — Phase 1, step 2 (`lslocks`): DONE.** Implemented, wired into
multicall, byte-for-byte diffed against the real system `lslocks` binary
across default/`--output-all`/`-r`/`-J`/`-H` modes. Found and fixed a real
parsing bug along the way: `/proc/<pid>/fdinfo/<fd>` `lock:` lines carry the
same leading `<id>: ` token as `/proc/locks` (an initial reading of the
uutils reference assumed otherwise), which had silently zeroed out `HOLDERS`
and all pid/command cross-referencing. Also empirically corrected three
formatting details (mountinfo field offset, fallback-path separator, blank
`SIZE` for zero-byte locked files) that diverge between the real C
`lslocks(1)` binary and what a literal reading of the Rust reference alone
would have produced — the real binary was treated as the tiebreaker source
of truth in each case. See `checklist/lslocks.md`.

**2026-07-27 — Phase 1, step 1 (`lsns`): DONE.** Implemented, tested against
real `/proc` and diffed against the real system `lsns(8)` (exact data match),
wired into the multicall dispatch, 8 unit tests, zero clippy warnings. See
`checklist/lsns.md`. While getting `cargo build -p zex-utils` to compile for
the first time this session, found and fixed **two more pre-existing,
build-breaking bugs unrelated to lsns**: `cmd/nologin/src/lib.rs:57`
(`.filter()` on a `Result`) and `cmd/mountpoint/src/lib.rs:55`
(`&str`→`Option<String>` type mismatch) — the multicall binary did not
compile at all before these fixes. Also commented out the `cmd/last`,
`cmd/lsipc`, `cmd/lslocks` workspace-member lines in the root `Cargo.toml`
(still genuinely empty) so the workspace loads; each gets uncommented as its
real implementation lands, next in Phase 1 order.

**New finding (not yet actioned):** `lsmem` — despite being a fully
implemented crate — is **not wired into the multicall binary at all**: it's
absent from `multicall/Cargo.toml`, `multicall/src/main.rs` (`UTIL_NAMES` +
dispatch match), and `multicall/utils.list`. `zex-utils lsmem` / a `lsmem`
symlink currently do nothing. This wasn't caught by the original
flag-comparison pass (which only read `cmd/lsmem` source, not the multicall
wiring). Recommend folding this fix into the P1 `lsmem` work item alongside
the `-B/--bytes` bug and missing flags (see `checklist/lsmem.md`).

---

## 0. Headline finding — build-breaking placeholder crates

`cmd/last`, `cmd/lsipc`, `cmd/lslocks`, `cmd/lsns` are **empty directories** (no
`Cargo.toml`, no `src/`) yet all four are listed as workspace members in the
root `Cargo.toml` (lines 62, 69, 70, 72). `cargo build --workspace`,
`scripts/build-all.sh`, and `scripts/test.sh` will fail to load the workspace
manifest as soon as anyone runs them, because a workspace member with no
`Cargo.toml` is a hard cargo error. Plain `cargo build --release` is unaffected
today only because `default-members = ["multicall"]` and the multicall crate
doesn't depend on these four — but that's incidental, not a fix.

This must be treated as a P0 correctness bug in the build, independent of
whether/when the 4 tools get real implementations.

---

## 1. Priority tiers

### P0 — Build integrity + fully-missing utilities
Nothing else should land until these are resolved, since `--workspace` builds
are currently broken and 4 advertised utilities silently don't exist.

| Utility | State | Why P0 |
|---|---|---|
| `last` | crate is empty | workspace member with no manifest (build-breaking) + advertised in README's util catalogue implicitly via "~95 utilities" claim |
| `lsipc` | crate is empty | same |
| `lslocks` | crate is empty | same |
| `lsns` | crate is empty | same |

Interim option if full implementation is deferred: give each crate a minimal
`Cargo.toml` + `src/lib.rs` that prints "not yet implemented, see
checklist/<name>.md" and returns a non-zero exit code, purely to stop the
workspace build from being broken, then implement for real in the same phase
or a follow-up. Recommendation below is to just implement them (they're
read-only introspection tools, well scoped, and the reference gives a
complete algorithm to port).

### P1 — Real bugs in shipped utilities
| Utility | Bug | Impact |
|---|---|---|
| `lscpu` | `-B/--bytes` is parsed but never threaded through to `calculate_cache_totals` — silently a no-op | cache sizes always print human-readable even when `-B` is passed; scripts parsing `-B` output get wrong data |
| `lsmem` | missing `-o/--output`/`--output-all` column selection, `-S/--split`, `-J`/`-P` output formats, `-s/--sysroot`, `--summary`, and NUMA node/zone columns entirely | any script/tool depending on these flags gets "unknown option" and exit 1; NUMA-aware tooling gets no node/zone data at all |
| `lsmem` | `human_size()` doesn't implement the real util-linux 2^n scaling+rounding algorithm | non-power-of-2 sizes render differently than real lsmem/reference |

### P2 — Behavioral gaps worth closing
| Utility | Gap |
|---|---|
| `dmesg` | `--since`/`--until` only accept 5 fixed strptime formats; reference (and real dmesg) accept a much broader GNU-date-like grammar (relative times like `"1 hour ago"`, `"now"`) |
| `uuidgen` | `-t` (v1 UUID) never attempts a real MAC address as the node ID — always fully random+multicast-bit. Reference tries `getifaddrs()` first. Currently documented as an intentional choice in `lib.rs:108-112` — needs an explicit product decision, not just a silent gap |
| `uuidgen` | no mutual-exclusivity validation across `-r/-t/-m/-s` — reference errors via clap `ArgGroup`, zex-utils silently prioritizes `-t` over the rest |
| `cal` | no mutual-exclusivity validation across `-y/-Y/-n` — same class of gap as uuidgen above |

### P3 — Needs verification / minor polish
| Utility | Item |
|---|---|
| `hexdump` | `-n`/`-s` size-suffix parsing was not verified byte-for-byte against `uucore::parser::parse_size::parse_size_u64` (couldn't inspect the crate source in this pass) — needs a follow-up diff of accepted suffix grammars (`T`/`P`/`E`, binary vs decimal prefixes) |
| `fsfreeze` | exit code on ioctl failure: zex-utils returns 1, reference effectively returns 0 (reference calls `show_error!` but still `Ok(())`s). zex-utils's behavior is arguably *more correct* — flagged for awareness only, no fix recommended unless byte-for-byte reference parity is a hard requirement |

### No action needed — already at parity or exceeding the reference
`blockdev`, `chcpu`, `ctrlaltdel`, `mcookie`, `mesg`, `nologin`, `renice`,
`setpgid`, `setsid` are complete ports with no functional gaps (see
`checklist/parity-confirmed.md` for the specific notes, including 3 cases
where zex-utils actually **fixes bugs present in the reference**: `mountpoint`
exit-code correctness + real inode-based mountpoint detection vs. the
reference's hardcoded-inode-2 heuristic, `setsid`'s exec-failure exit-code bug,
and `setpgid`'s coarser exit-code granularity).
`rev` has one open design question (byte-wise vs Unicode-codepoint-wise
reversal) rather than a gap — see `checklist/parity-confirmed.md`.

---

## 2. Phased plan

**Phase 1 — Stop the bleeding (P0)**
1. Implement `lsns` (namespace lister) — self-contained `/proc` walk, no IPC/locking dependencies. Good first target.
2. Implement `lslocks` (`/proc/locks` + per-pid `fdinfo`) — also self-contained.
3. Implement `lsipc` (System V IPC via `/proc/sysvipc/*`) — largest of the four (shm/sem/msg + column system), do last within this phase.
4. Implement `last` (wtmp/utmpx reader) — needs a utmpx record reader; check whether `core/` (zexcore) already has any utmp helpers before writing one from scratch, since zex-utils avoids external crates like `uucore`'s `entries`/`utmpx` support.

Each of these gets full flag parity with what's listed in its `checklist/<name>.md`
(derived from the reference's `.md` doc + source). Do NOT pull in `uucore`,
`smartcols-sys`, or other reference dependencies — reimplement natively per
project convention (see README "Design principles" #3).

**Phase 2 — Fix P1 bugs**
5. `lscpu`: thread `out_opts.bytes` through to `calculate_cache_totals` (one-line-ish fix, plus a regression test).
6. `lsmem`: add `-o/--output`/`--output-all` column selection, `-S/--split`, `-J`/`-P`, `-s/--sysroot`, `--summary`; add NODE/ZONES columns (reads `node*` sysfs subdir + `valid_zones`); replace `human_size()` with the correct 2^n scaling+rounding algorithm.

**Phase 3 — Close P2 behavioral gaps**
7. `dmesg`: broaden `--since`/`--until` parsing (decide whether to hand-roll a wider subset or accept the scope as-is — needs a product call, see open question below).
8. `uuidgen`: add `ArgGroup`-equivalent mutual-exclusivity check for `-r/-t/-m/-s`; make a product decision on whether `-t` should attempt a real interface MAC (privacy vs. correctness tradeoff — util-linux's real uuidgen documents this as a known privacy leak, which may be *why* zex-utils avoided it deliberately).
9. `cal`: add mutual-exclusivity check for `-y/-Y/-n`.

**Phase 4 — Verification pass (P3)**
10. `hexdump`: diff the exact suffix grammar against `uucore`'s `parse_size_u64` (vendor-read the crate via `cargo doc`/crates.io source or `~/.cargo/registry` if available) and close any gap found.
11. `fsfreeze`: decide (with user) whether byte-for-byte reference exit-code parity is wanted, or whether to keep zex-utils's arguably-more-correct behavior and just document the divergence.

---

## 3. Open questions for the user before implementation starts

- **uuidgen `-t` node ID**: real MAC address (matches reference, matches real util-linux, but leaks a stable hardware identifier into generated UUIDs — a known privacy concern) vs. keep current random+multicast-bit behavior (privacy-safe, diverges from reference)?
- **dmesg date grammar**: is GNU-date-like relative parsing ("2 hours ago") actually needed for Zainium's use cases, or is the current fixed-format set intentional/sufficient?
- **Phase ordering**: is the Phase 1 order (lsns → lslocks → lsipc → last) acceptable, or should `last` be prioritized higher/lower for a specific reason (e.g. Zainium doesn't ship wtmp/utmpx yet, which would block `last` entirely until that exists)?

---

## 4. Non-goals of this pass

- The other ~133 zex-utils utilities with no util-linux-main counterpart (coreutils/findutils/checksum family/next-gen tools) were **not** reviewed here — out of scope per agreed plan. A future pass could compare those against GNU coreutils/findutils behavior instead.
- No code was changed in this pass. Everything above is planning input.
