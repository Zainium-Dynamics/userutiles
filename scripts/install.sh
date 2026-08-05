#!/bin/sh
# Install user_utils into Zainium overlayer (NOT /usr/bin) — one discrete
# binary per cmd/* crate.
#
# Defaults for PREFIX/PROFILE come from utils.toml — this script never
# hardcodes them. Env vars still override the .toml when set, for staged /
# cross-root installs.
#
# Usage:
#   ./scripts/install.sh
#   PREFIX=/overlayer/syshub ./scripts/install.sh
#   TARGET=x86_64-unknown-linux-musl PREFIX=/overlayer/syshub DESTDIR=/path/to/rootfs ./scripts/install.sh
#
# Env: PREFIX, ZEX_PREFIX, DESTDIR, TARGET, PROFILE
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
. ./scripts/lib.sh

CONFIG="utils.toml"
TOML_PREFIX="$(toml_get "$CONFIG" paths prefix)"
TOML_PROFILE="$(toml_get "$CONFIG" build profile)"

PREFIX="${PREFIX:-${ZEX_PREFIX:-$TOML_PREFIX}}"
BINDIR="${BINDIR:-$PREFIX/bin}"
DESTDIR="${DESTDIR:-}"
TARGET="${TARGET:-}"
PROFILE="${PROFILE:-$TOML_PROFILE}"

if [ "$PROFILE" = "release" ]; then
 REL=release
 CARGO_FLAGS="--release"
else
 REL=debug
 CARGO_FLAGS=""
fi

if [ -n "$TARGET" ]; then
 OUT="target/${TARGET}/${REL}"
 TARGET_FLAG="--target ${TARGET}"
else
 OUT="target/${REL}"
 TARGET_FLAG=""
fi

echo "user_utils install"
echo " DESTDIR=$DESTDIR"
echo " BINDIR=$DESTDIR$BINDIR"
echo " TARGET=${TARGET:-host} PROFILE=$PROFILE"
echo " OUT=$OUT"

mkdir -p "$DESTDIR$BINDIR"

cargo build $CARGO_FLAGS $TARGET_FLAG --workspace
for f in "$OUT"/*; do
 [ -f "$f" ] && [ -x "$f" ] || continue
 base=$(basename "$f")
 case "$base" in
 *.*) continue ;;
 esac
 install -m 755 "$f" "$DESTDIR$BINDIR/$base"
 echo " installed $base"
done

echo "Done. Ensure PATH includes: $BINDIR"
echo " (Zainium: $PREFIX/bin — never /usr/bin)"
