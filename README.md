# user_utils

Rust implementation of the Unix userland for Zainium OS — coreutils, a
growing part of util-linux, and a few Zainium-specific tools. One binary
per utility, no multi-call dispatcher, no uucore/uutils dependency.

175+ tools. Coverage: ~98% of GNU coreutils, ~34% of util-linux 2.42.
Gap list: [MISSING.md](MISSING.md). Per-tool verification notes:
[checklist/](checklist/).

## Build

```bash
./scripts/build.sh                 # everything, host target
cargo build --release -p user_ls   # a single crate
```

Binaries land in `target/release/<tool>`. Cross builds:

```bash
TARGET=x86_64-unknown-linux-musl ./scripts/build.sh
./scripts/build-zainium.sh   # Zainium's native musl target (needs a pinned nightly)
```

## Install

```bash
PREFIX=/overlayer/syshub ./scripts/install.sh
```

## Layout

- `core/` — usercore: shared UI, digests, block-device/partition
  probing, Zainium path resolution
- `cmd/<name>/` — one crate per utility (`src/lib.rs` exports
  `pub fn run() -> i32`, `src/main.rs` calls it)
- `targets/` — Rust target spec for Zainium's musl toolchain
- `scripts/` — build.sh, build-zainium.sh, install.sh, test.sh, clippy.sh

Adding a tool means adding a `cmd/<name>` crate and one line in the root
`Cargo.toml`.

## Zainium specifics

No FHS `/usr`, no top-level `/etc` — everything lives under
`/overlayer/syshub`. Path resolution goes through `usercore::zainium`,
which falls back to plain `/etc`, `$PATH`, etc. on an ordinary Linux host
so the workspace builds and tests outside Zainium too. `login` reads the
passwd/shadow files that way and verifies via the system's `crypt(3)` —
no PAM.

## License

GPL-3.0. Copyright (c) Zainium Dynamics.
