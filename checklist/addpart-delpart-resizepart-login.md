# addpart / delpart / resizepart / login

**Status:** DONE (2026-08-26)

## addpart / delpart / resizepart

Thin wrappers over a new shared `usercore::blkpg` module (the `BLKPG`
ioctl logic that used to live only inside `partx`, now factored out so
these three don't duplicate the hand-defined `blkpg_ioctl_arg`/
`blkpg_partition` structs). `partx` itself was refactored to call the
shared module too.

- `addpart DEVICE PARTITION START LENGTH` — `BLKPG_ADD_PARTITION`.
- `delpart DEVICE PARTITION` — `BLKPG_DEL_PARTITION`.
- `resizepart DEVICE PARTITION LENGTH` — reads the partition's current
  start position from `usercore::ptable::read_table` (the ioctl needs
  both start and new length; only the length actually changes) and
  calls `BLKPG_RESIZE_PARTITION`.

**Verified:** `usercore::blkpg`'s struct-size assertions (152/24 bytes,
matching the documented kernel ABI) and no-such-device failure case
moved with the code and still pass. Each of the three new binaries was
smoke-tested against `/dev/null` (opens fine, isn't a block device) and
correctly reported the same real `ENOTTY`-class error, not a panic. As
with `partx`, the ioctl itself couldn't be exercised against a real
kernel partition table in this sandbox (no loop/block device access).

## login

Self-contained (no PAM — see below), no per-project code shared with
`elevate-umbra`'s `sulogin`. Reads `usercore::zainium::passwd_path()`/
`shadow_path()` — new `usercore::zainium` helpers that resolve to
`/overlayer/syshub/etc/{passwd,shadow}` (Zainium has no top-level
`/etc`; that syshub directory is what `elevate-umbra` manages in place
of a real `/etc/shadow`) if present, falling back to plain
`/etc/{passwd,shadow}` so the workspace still builds/tests on an
ordinary Linux host. Password verification goes through the system's
own `crypt(3)` (linked explicitly only on glibc — `musl` bundles
`crypt()` directly in its libc, no separate `-lcrypt` needed there), so
every hash format the platform's libc/libcrypt supports (MD5 `$1$`,
SHA-256/512 `$5$`/`$6$`, yescrypt `$y$`, …) works without reimplementing
any KDF.

**No PAM dependency, Linux PAM or `elevate-pam` either.** Real PAM
needs service-config files this workspace/Zainium doesn't ship, and
`elevate-pam` (in the separate `elevate-privilege` project) is
`publish = false` and explicitly only buildable as a member of that
project's own private monorepo — not something this repo can depend on
as an external crate. Direct passwd/shadow + `crypt(3)` is the correct,
portable choice here.

**Scope, stated plainly:** no PAM, no utmp/wtmp session accounting (so
`last` won't show logins made through this binary yet), no account
expiry (`chage`) enforcement beyond a locked (`!`/`*`) password hash.
`sulogin` is intentionally *not* built here — it already exists,
fully implemented, in `elevate-umbra` (`src/bin/sulogin.rs`), a separate
Zainium Dynamics component; duplicating it in this repo would just be
two implementations to keep in sync.

**Verified:**
- `cargo test`: passwd/shadow line parsing, the locked-account
  (`!`/`*`) check, and — the important one — `verify_password` checked
  against a hash produced by the *actual system* `crypt(3)` at test
  time (so this is meaningful on whatever libc/libcrypt the build
  machine has, not a hardcoded vector tied to one algorithm).
- **Independently cross-checked**: hashed a password with Python's
  standard-library `crypt` module (`crypt.crypt(pw,
  crypt.mksalt(crypt.METHOD_SHA512))`) — a completely separate
  implementation from Rust's FFI call — and confirmed our `crypt(3)`
  binding both accepts the correct password and rejects a wrong one
  against that externally-generated hash.
- Live smoke test: `login --help`; a full end-to-end attempt with a
  nonexistent username retried and correctly failed 3 times ("Login
  incorrect") without ever touching a real system's passwd/shadow
  destructively.
- `usercore::zainium::passwd_path`/`shadow_path`'s syshub-vs-`/etc`
  fallback logic has its own test (temporarily pointing `ZEX_PREFIX` at
  a scratch directory, both with and without an `etc/` inside it).
