#!/usr/bin/env bash
# =============================================================================
# Cross-build user_utils for Zainium OS's native x86_64-zainium-linux-musl
# target, using the custom target spec at targets/x86_64-zainium-linux-musl.json
# (see targets/README.md for why this exists alongside the plain
# TARGET=x86_64-unknown-linux-musl path in scripts/build.sh).
# =============================================================================
# Unlike zex-native/elevate-privilege's build-zainium.sh, there's no
# dynamic/static split here: every cmd/* crate is a plain executable (no
# cdylib), so this always produces fully static binaries
# (targets/x86_64-zainium-linux-musl.json sets crt-static-default: true).
#
# Needs a pinned nightly + `-Z build-std` (no prebuilt std ships for a
# custom target spec) — same pin as elevate-privilege's build-zainium.sh,
# for one consistent nightly across Zainium OS's Rust components.
#
# Usage:
#   ./scripts/build-zainium.sh
#
# Env overrides:
#   ZAINIUM_TOOLCHAIN_BIN  bin/ dir of the x86_64-zainium-linux-musl
#                          cross-compiler (default: read from utils.toml's
#                          [toolchain].zairoot, same as scripts/build.sh)
#   ZAINIUM_NIGHTLY        pinned nightly toolchain (default: nightly-2026-05-24)
#   PROFILE                cargo profile (default: release)
# =============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
. ./scripts/lib.sh

ZAIROOT="${ZAIROOT:-$(toml_get utils.toml toolchain zairoot)}"
ZAINIUM_TOOLCHAIN_BIN="${ZAINIUM_TOOLCHAIN_BIN:-$ZAIROOT/bin}"
ZAINIUM_NIGHTLY="${ZAINIUM_NIGHTLY:-nightly-2026-05-24}"
ZAINIUM_TARGET_JSON="${ROOT}/targets/x86_64-zainium-linux-musl.json"
ZAINIUM_LINKER="x86_64-zainium-linux-musl-gcc"
DIST="${ROOT}/dist/zainium"
PROFILE="${PROFILE:-release}"

log() { printf '==> %s\n' "$*"; }
warn() { printf '!!  %s\n' "$*" >&2; }

if [[ ! -d "$ZAINIUM_TOOLCHAIN_BIN" ]]; then
  warn "Zainium toolchain not found at $ZAINIUM_TOOLCHAIN_BIN"
  warn "set ZAINIUM_TOOLCHAIN_BIN (or ZAIROOT) to the toolchain's location"
  exit 1
fi
export PATH="${ZAINIUM_TOOLCHAIN_BIN}:${PATH}"

log "user_utils: x86_64-zainium-linux-musl (+${ZAINIUM_NIGHTLY}, -Z build-std)"

cargo "+${ZAINIUM_NIGHTLY}" build --"$PROFILE" \
  -Z json-target-spec \
  --target "$ZAINIUM_TARGET_JSON" \
  -Z build-std=core,alloc,std,panic_abort \
  --config "target.x86_64-zainium-linux-musl.linker=\"${ZAINIUM_LINKER}\"" \
  --workspace

OUTDIR="target/x86_64-zainium-linux-musl/${PROFILE}"
install -d "$DIST/overlayer/syshub/bin"

count=0
for f in "$OUTDIR"/*; do
  [[ -f "$f" && -x "$f" ]] || continue
  base="$(basename "$f")"
  case "$base" in
    *.*) continue ;;
  esac
  install -m 755 "$f" "$DIST/overlayer/syshub/bin/$base"
  count=$((count + 1))
done

log "installed $count binaries to $DIST/overlayer/syshub/bin"
log "done"
echo "  rsync -a $DIST/overlayer/ <target-zairoot>/overlayer/  # to deploy"
