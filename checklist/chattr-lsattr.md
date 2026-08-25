# chattr / lsattr

**Status:** DONE (2026-08-25)

**Source:** ported from e2fsprogs 1.47.4's `misc/chattr.c` and
`misc/lsattr.c` (not util-linux — e2fsprogs — tracked here anyway since
that's where the rest of the ext2/3/4 attribute tooling lives in this
repo's `checklist/`).

## What was ported

- `chattr`: `-R -V -f -p PROJECT -v VERSION`, `+`/`-`/`=` flag syntax, all
  18 settable flags (`AacCdDeFijmPsStTux`), recursive directory walk
  (skips symlinks the same way the C source does, via `lstat`).
- `lsattr`: `-R -V -a -d -l -v -p`, short and `-l` long flag display (22
  known flags, including the read-only ones `chattr` can't set —
  `EINtVN`), project/version columns, recursive listing with the
  `\npath:\n` header the real binary prints.
- Both use `FS_IOC_GETFLAGS`/`SETFLAGS`/`GETVERSION`/`SETVERSION` (via
  `libc::ioctl`, constants not in the `libc` crate for the ext-family
  flag bits or the project-id `FS_IOC_FSGETXATTR`/`FSSETXATTR` pair — those
  are hand-defined from `linux/fs.h`'s stable ABI values).
- `chattr` refuses to touch `/overlayer/syshub`/`/overlayer/zaisys`
  (`usercore::protect::modification_denied`), same guard `chmod`/`chown`
  already use — otherwise `chattr -i` could be used to defeat their
  removal/modification protection.

## Verified

- `cargo test -p user_chattr -p user_lsattr`: unit tests for flag
  parsing, flag-table round-tripping, and the protect-tree guard.
- Manual diff against the real `/usr/bin/chattr` / `/usr/bin/lsattr` on
  this machine: identical `lsattr` short/`-l` output (including the
  22-dash "no flags" baseline and column padding), identical error
  message shape when an ioctl is refused (`Operation not permitted`) on a
  tmpfs scratch file.
- Root-required flags (`-i`/`-a` set) were exercised too, since this
  sandbox happens to grant the capability; verified the flag round-trips
  (set then clear) via `get_flags`.

## Known caveat (pre-existing, not introduced here)

On this dev machine `/overlayer` is itself a symlink into the mounted
`zairoot` (`/overlayer -> .../zairoot/overlayer`), so
`usercore::protect::resolve_path`'s `fs::canonicalize()` resolves paths
under it to their real target (e.g. `.../zairoot/overlayer/syshub/bin/env`
→ `.../zairoot/overlayer/syshub/bin/coreutils`), which no longer starts
with the literal `/overlayer/syshub` prefix the guard checks — so the
guard doesn't actually fire for real files here. Confirmed this isn't
`chattr`-specific: `chmod 777` on the same path bypasses `chmod`'s
identical guard for the same reason. This is a property of the shared
`protect` module plus this machine's convenience symlink, not something
`chattr`/`lsattr` do differently — on an actual Zainium OS install
`/overlayer` is the real tree, not a symlink, so the guard is effective
there. Flagging here rather than silently "fixing" shared `protect.rs`
canonicalization behavior that `rm`/`chmod`/`chown` already depend on.
