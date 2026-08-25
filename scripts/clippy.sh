#!/usr/bin/env bash
# Lint the whole workspace.
#
# Usage:
#   ./scripts/clippy.sh            # warnings only, non-blocking
#   ./scripts/clippy.sh --strict   # -D warnings, excluding vendored zex_diffutils
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ "${1:-}" = "--strict" ]; then
  # cmd/diffutils is vendored from uutils/diffutils — it must still build and
  # pass tests, but we don't hand-maintain its lint cleanliness, so it's
  # excluded from the strict (-D warnings) pass.
  cargo clippy --workspace --all-targets --exclude user_diffutils -- -D warnings
else
  cargo clippy --workspace --all-targets
fi
