#!/usr/bin/env bash
# Build the entire user_utils workspace: one discrete binary per cmd/* crate.
#
# Default PROFILE comes from utils.toml ([build].profile) — set the PROFILE
# env var to override it without editing the file.
#
# Usage:
#   ./scripts/build.sh
#   PROFILE=dev ./scripts/build.sh
#   TARGET=x86_64-unknown-linux-musl ./scripts/build.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
. ./scripts/lib.sh

PROFILE="${PROFILE:-$(toml_get utils.toml build profile)}"
TARGET="${TARGET:-}"

case "$PROFILE" in
  release) CARGO_FLAGS=(--release); OUT_DIR=release ;;
  dev)     CARGO_FLAGS=();          OUT_DIR=debug ;;
  *)       CARGO_FLAGS=(--profile "$PROFILE"); OUT_DIR="$PROFILE" ;;
esac
OUT="target${TARGET:+/$TARGET}/$OUT_DIR"

if [ -n "$TARGET" ]; then
  CARGO_FLAGS+=(--target "$TARGET")
  setup_musl_cross "$TARGET"
fi

echo "==> user_utils workspace (all discrete binaries)"
echo "    profile: $PROFILE  target: ${TARGET:-host}"

cargo build "${CARGO_FLAGS[@]}" --workspace

echo "==> $OUT/ (all bins)"
echo "Done."
