# xmv — Next-Generation Move Utility for Zainium OS (Zainium Dynamics)

> **Atomic. Undoable. Trash-Aware.** 
> A pure-Rust `mv` replacement built for Zainium OS.

---

## Why xmv?

| Feature | `mv` (GNU) | `xmv` |
|---|---|---|
| Atomic same-device rename | ✓ rename(2) | ✓ rename(2) + renameat2 |
| Atomic no-replace (no TOCTOU) | ✖ | ✓ renameat2(RENAME_NOREPLACE) |
| Atomic exchange of two paths | ✖ | ✓ renameat2(RENAME_EXCHANGE) |
| Cross-device: verify before delete | ✖ | ✓ XXH3-128 checksum |
| Cross-device: parallel copy | ✖ Single-threaded | ✓ rayon thread-pool |
| Undo last operation | ✖ | ✓ `--undo` (TOML journal) |
| Trash overwritten files | ✖ | ✓ `--trash-safe` (XDG Trash) |
| Memory safety | C / unsafe | ✓ Pure Rust, zero `unwrap()` |

---

## Operation Decision Tree

```
xmv src dest
 │
 ├── --undo → Reverse last journal entry
 ├── --exchange A B → renameat2(RENAME_EXCHANGE) ← O(1), atomic swap
 ├── --no-replace → renameat2(RENAME_NOREPLACE) ← no TOCTOU race
 │
 ├── Same filesystem?
 │ └── YES → rename(2) ← O(1), always atomic
 │
 └── Different filesystem?
 └── NO → copy_file_range (parallel)
 → XXH3-128 verify (default on)
 → delete source (only after verify passes)
```

---

## Installation

```bash
git clone https://github.com/zainium/xmv
cd xmv
cargo build --release
sudo cp target/release/xmv /usr/local/bin/
```

---

## Usage

```
xmv [OPTIONS] <SOURCE>... <DEST>
```

### Examples

```bash
# Standard move (same device — instant)
xmv file.txt /backup/file.txt

# Move directory recursively (cross-device — parallel copy)
xmv -R -j 8 /home/ali/projects /mnt/external/projects

# Atomic no-replace: fail if destination exists (no TOCTOU race)
xmv --no-replace new_config.toml /etc/app/config.toml

# Atomic exchange: swap two paths in one kernel operation
xmv --exchange /etc/nginx/nginx.conf /etc/nginx/nginx.conf.new

# Trash the destination before overwriting it
xmv --trash-safe new_version.db /var/app/data.db

# Cross-device move with full audit trail
xmv -R --verify --archive --journal ~/xmv.log src/ /mnt/nas/dest/

# Undo the last recorded operation
xmv --undo

# Undo using a specific journal file
xmv --undo --journal ~/xmv.log
```

### All Flags

| Flag | Description |
|---|---|
| `-R, --recursive` | Move directories recursively |
| `-n, --no-clobber` | Skip if destination exists |
| `--no-replace` | Atomic fail-if-dest-exists (renameat2) |
| `--exchange` | Atomically swap two paths (renameat2) |
| `--verify` | XXH3-128 checksum before deleting source (default: on) |
| `-a, --archive` | Preserve permissions, timestamps, xattrs |
| `--trash-safe` | Move overwritten destination to XDG Trash |
| `--journal <path>` | Custom journal path (default: XDG_STATE_HOME/xmv/) |
| `--undo` | Reverse last committed journal entry |
| `-j, --jobs N` | Parallel threads for cross-device copy |
| `--progress` | Show progress bar (default: on) |
| `-v, --verbose` | Print each operation |

---

## Undo Journal

Every operation is recorded in a TOML journal before the filesystem is mutated
(zex-utils uses **TOML only** — never JSON). Running `mv --undo` reverses the
most recent committed entry:

```toml
[[entries]]
op = "move"
src = "/home/ali/a.txt"
dest = "/backup/a.txt"
ts = 1700000000
committed = true

[[entries]]
op = "exchange"
path_a = "/etc/nginx.conf"
path_b = "/etc/nginx.conf.new"
ts = 1700000060
committed = true
```

Default path: `$XDG_STATE_HOME/mv/journal.toml`

---

## Platform Support

| Platform | Status |
|---|---|
| Linux ≥ 3.15 | ✓ Full — renameat2 + copy_file_range |
| Linux < 3.15 | ✓ Fallback — rename(2) + buffered copy |
| Redox OS | ✓ rename(2) + buffered copy |
| Windows / macOS / BSD | ✖ Not supported by design |

---

## Build & Test

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings
cargo audit
```

---

## License

GNU General Public License v3.0
