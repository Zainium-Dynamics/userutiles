# scripts/lib.sh — shared helpers, sourced by the other scripts/*.sh files.
#
# toml_get() is a deliberately minimal TOML reader: it only understands the
# flat `key = "value"` lines inside utils.toml (no nested tables, arrays, or
# multi-line values). That's all utils.toml needs, and it keeps every script
# dependency-free (no python3/yq/toml-cli requirement to build zex-utils).
#
# Usage: toml_get <file> <section> <key>
#   toml_get utils.toml paths prefix   -> /overlayer/syshub
toml_get() {
  file="$1"; section="$2"; key="$3"
  awk -v section="[$section]" -v key="$key" '
    $0 == section { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      sub(/^[^=]*=[[:space:]]*/, "")
      gsub(/^"|"[[:space:]]*$/, "")
      print
      exit
    }
  ' "$file"
}

# setup_musl_cross <target-triple> — when <target-triple> matches
# [toolchain].musl_target in utils.toml, point cargo at the Zainium musl
# cross-compiler (zairoot's x86_64-zainium-linux-musl-gcc) for that target,
# via the target-specific CARGO_TARGET_<TRIPLE>_LINKER env var — this never
# touches .cargo/config.toml, so a plain `musl-gcc` build still works
# unchanged for anyone without zairoot mounted.
#
# zairoot's location comes from [toolchain].zairoot in utils.toml; override
# with the ZAIROOT env var (different mount point, CI, etc.) without editing
# the file. If the cross-gcc isn't found there, this is a silent no-op and
# cargo falls back to whatever .cargo/config.toml already configures.
setup_musl_cross() {
  target="$1"
  musl_target="$(toml_get utils.toml toolchain musl_target)"
  [ "$target" = "$musl_target" ] || return 0

  zairoot="${ZAIROOT:-$(toml_get utils.toml toolchain zairoot)}"
  musl_cc="$(toml_get utils.toml toolchain musl_cc)"
  gcc_bin="$zairoot/bin/$musl_cc"

  if [ -x "$gcc_bin" ]; then
    env_target="$(printf '%s' "$musl_target" | tr 'a-z-' 'A-Z_')"
    export "CARGO_TARGET_${env_target}_LINKER=$gcc_bin"
    export PATH="$zairoot/bin:$PATH"
    echo "    musl cross: $gcc_bin"
  else
    echo "    musl cross: zairoot toolchain not found at $gcc_bin — falling back to .cargo/config.toml's musl-gcc"
  fi
}
