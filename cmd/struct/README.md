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
| `src/core/engine.rs` | Last name contains `.` | Create missing parents, then create an empty file |
| `src/services/auth` | Last name has no `.` | Create the full directory tree |
| `~/.config/systemd/user` | Dots only in parents or a leading dot | Create the full directory tree |
| `etc/conf.d/` | Trailing `/` | Create the full directory tree |
| `bin/trigger` | Last name has no `.` | Create a directory tree by default |
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
 2. If the last name in <PATH> has a '.' (engine.rs), create it as a file.
 3. Otherwise create a directory tree (mkdir -p); dots in parent dirs,
    a leading dot (.config) or a trailing '/' (conf.d/) stay directories.
 4. Use -t to force a file target (bin/trigger, .bashrc).

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
