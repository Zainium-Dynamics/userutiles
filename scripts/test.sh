#!/usr/bin/env bash
# Run the whole workspace test suite (unit + integration tests for every
# cmd/* crate and core).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo test --workspace "$@"
