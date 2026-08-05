# struct: Smart Filesystem Creator

`struct` is a zero-dependency, native filesystem utility written in Rust. It combines the practical workflows of `mkdir -p` and `touch` into a single, deterministic command while strictly enforcing a zero-overwrite safety model.

The binary is named `struct`.

## Usage

```bash
struct <PATH>
struct -t <PATH>

```

## Rules

`struct` accepts one target per invocation.

| Input | Detection | Operation |
| --- | --- | --- |
| `src/core/engine.rs` | Path contains `.` | Create missing parents, then create an empty file |
| `src/services/auth` | Path does not contain `.` | Create the full directory tree |
| `bin/trigger` | Path does not contain `.` | Create a directory tree by default |
| `struct -t bin/trigger` | `-t` forces file mode | Create missing parents, then create extensionless file `trigger` |

Before creating anything, `struct` checks whether the terminal target already exists. Existing files, directories, symlinks, and other filesystem objects abort the operation immediately.

## Safety Contract

* **Zero Dependencies:** No external crates are used.
* **Manual Parsing:** Argument parsing is handled directly via `std::env::args_os`.
* **Architecture:** Native Zainium OS build target.
* **Strict Targeting:** Enforces one target per process invocation.
* **Immutable Existing State:** Existing terminal paths are strictly preserved and never overwritten.
* **Smart Trees:** Parent directories are generated automatically when the terminal target is absent.
* **Atomic Operations:** File creation uses atomic create-new semantics.
* **Validation:** Directory creation avoids accepting an already-existing final directory as a success state.

## Examples

Create a Rust source file and all missing parent directories:

```bash
struct src/core/engine.rs

```

Create a directory tree:

```bash
struct src/services/auth

```

Create an extensionless file:

```bash
struct -t bin/trigger

```

Abort on an existing target:

```text
struct: error: file already exists
path: src/core/engine.rs
aborted: existing filesystem objects are never overwritten

```

## Help Output

```text
struct - Smart filesystem creator

USAGE:
 struct <PATH>
 struct -t <PATH>

RULES:
 1. If <PATH> already exists, abort immediately.
 2. If <PATH> contains '.', create it as a file.
 3. If <PATH> does not contain '.', create it as a directory tree.
 4. Use -t to force an extensionless file target.

EXAMPLES:
 struct src/core/engine.rs # creates parents, then engine.rs
 struct src/services/auth # creates directory tree
 struct -t bin/trigger # creates extensionless file

EXIT STATUS:
 0 success
 1 invalid input or filesystem error

```

```</PATH></PATH></PATH></PATH></PATH></PATH></PATH>

```
