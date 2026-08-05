# targets/

Custom Rust target specs for cross-compiling `user_utils` directly against
Zainium OS's own musl toolchain, rather than rustup's generic
`x86_64-unknown-linux-musl` triple.

Same convention as `zex-native/relibc/targets/` and
`zex-native/elevate-privilege/targets/` — one `.json` per target triple,
consumed via `cargo +<nightly> build -Z build-std -Z json-target-spec
--target targets/<name>.json` (see `scripts/build-zainium.sh`).

| File | Triple | Notes |
|------|--------|-------|
| `x86_64-zainium-linux-musl.json` | `x86_64-zainium-linux-musl` | Fully static by default (`crt-static-default: true`) — `user_utils` only ships plain executables, no `cdylib`, so this differs from `elevate-privilege`'s copy of the same spec (which sets it `false` to allow PAM `.so` modules). |

## Why a custom target spec at all

Rustup's stock `x86_64-unknown-linux-musl` target, pointed at Zainium's
`x86_64-zainium-linux-musl-gcc` cross-compiler via
`.cargo/config.toml`/`utils.toml`'s `[toolchain]` table
(`scripts/build.sh TARGET=x86_64-unknown-linux-musl`), already produces a
working static-musl binary linked against Zainium's sysroot — that path
needs no nightly and no `-Z build-std`, and is the one to reach for day to
day.

This custom target spec exists for full alignment with the rest of
Zainium OS's Rust toolchain fleet, which is standardized on the
`x86_64-zainium-linux-musl` triple name (not the generic
`x86_64-unknown-linux-musl`) across `relibc`, `elevate-privilege`, and
`zainix-kernel`. Use `scripts/build-zainium.sh` when that alignment
matters more than avoiding the nightly + `-Z build-std` requirement.

## Requires

- A pinned nightly toolchain (no prebuilt `std` ships for a custom target
  JSON) — see `scripts/build-zainium.sh`'s `ZAINIUM_NIGHTLY`.
- Zainium's `x86_64-zainium-linux-musl-gcc` cross-compiler on `PATH` (from
  `zairoot`, see `utils.toml`'s `[toolchain] zairoot`).
