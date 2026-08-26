# agetty

**Status:** DONE (2026-08-26)

## What was built

Opens the given tty (or reuses an already-open one with `-`, an
extension real `agetty` also accepts), makes it the controlling
terminal (`setsid(2)` + `TIOCSCTTY`), prints `/etc/issue` (via
`usercore::zainium::etc_path`, so it resolves the same
syshub-then-`/etc` way everything else does) with `\l`/`\n`/`\s`/`\r`/
`\m` escapes expanded, prompts for a username (or skips straight to a
given one via `-a`/`--autologin`, or none at all via `-n`/
`--skip-login`), then `exec`s the login program (`login` by default,
found on Zainium's `PATH`; `-l`/`--login-program` overrides it) with
that username — the same handoff real `agetty(8)` does to `login`.

Also implemented: `-o`/`--login-options` (with `\u` → username
substitution), `-H`/`--host` (fake hostname for the prompt/banner),
`-i`/`--noissue`, `-t`/`--timeout` (via `poll(2)` on stdin, not a
busy-wait). `-L`/`--local-line`/`--noclear` are accepted as no-ops —
there's no carrier-detect or screen-clear behavior here to skip in the
first place. Any other unrecognized flag is accepted and ignored rather
than a hard error, since real invocations often pass hardware-specific
flags (`-8`, `-f`, `-w`, …) this port has no use for.

Not implemented: actual baud-rate/line-discipline configuration
(positional `BAUD_RATE` args are accepted but not acted on), interactive
serial autodetection — this port assumes a virtual console or an
already-configured serial line, matching how `agetty` is used inside a
container/VM the vast majority of the time.

## Verified

- `cargo test`: `/etc/issue` escape expansion (known + unknown escapes),
  `-o` option `\u` substitution, missing-login-program handling, and
  the timeout path's non-blocking behavior on a pipe with no writer.
- Live smoke test, end to end: `agetty -a USER -l fake_login.sh -i -`
  (reusing this shell's own stdin instead of opening a real tty device,
  since this sandbox has none to spare) correctly skipped the prompt
  and handed off to the fake login program with the right argv.
- A second live run with the interactive prompt path (no `-a`) correctly
  printed the **real** `/etc/issue` on this machine with `\l`/`\s`/`\r`/
  `\n` expanded to the actual tty arg, OS release, and hostname, read a
  typed username from stdin, applied `-o "-h \u"` substitution, and
  handed both the substituted option and the username to the fake login
  program — the whole prompt → read → substitute → exec chain working
  together, not just the individual functions in isolation.
