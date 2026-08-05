# blueprint: Project Architecture Scaffolding

`blueprint` is a native Rust CLI utility engineered to map and generate comprehensive structural blueprints of project directories. It walks through complex directory trees and outputs a strict, human-readable text representation of the architecture.

## Features

* **Architectural Mapping**: Generates a complete directory tree outline for any project.
* **Intelligent Archiving**: Automatically detects and safely archives previous blueprint generations (e.g., `blueprint-1.txt`, `blueprint-2.txt`) without overwriting.
* **Smart Filtering**: Natively ignores non-essential directories such as hidden files, `target`, and `node_modules` to ensure clean architectural mapping.
* **Workspace Aware**: Automatically handles workspace crates and calculates file/directory counts.
* **Safe Execution**: Enforces strict write-permission checks and utilizes lossy conversion for non-UTF8 paths to prevent panics.

## Usage

Generate a blueprint for the current directory:
```bash
blueprint .

```

Generate a blueprint for a specific project path:

```bash
blueprint /path/to/project

```

## Output Behavior

The tool writes the structure to a root-level file named `<PROJECT_NAME>_blueprint.txt`. If an older blueprint already exists, it is automatically rotated and archived as `blueprint-1.txt`, `blueprint-2.txt`, etc.

## Example Output

For a project named `core_engine`, the generated text file will contain:

```text
Project Tree

core_engine/
├── Cargo.toml
├── Cargo.lock
└── src/
 └── main.rs

```

## Notes & Safety Contract

* **Pathological Structure Protection**: A strict recursion depth limit is enforced to prevent memory exhaustion on infinitely nested directory structures.
* **Permissions**: Exits safely with a standard error code if the target directory lacks write permissions.
* **Zero Dependencies**: Follows the core toolchain philosophy of minimal external crate usage.

```

```
