# zai-trigger Usage Guide

## Overview
`zai-trigger` is a universal launcher that executes both applications and script files with a clean, user-friendly interface.

## Installation

### Build from Source
```bash
cargo build --release
# Binary: target/release/trigger
```

### Setup Configuration (Optional)
```bash
mkdir -p ~/.config/trigger
cp config.example.toml ~/.config/trigger/config.toml
# Edit as needed
```

---

## Basic Usage

### Launch an Application
```bash
zex --trigger code
# or
./trigger --trigger code
```

**Output:**
```
Nice choice! Launching application...

→ Resolving application...
 - Package Detection : Visual Studio Code found (zxpkg)
 - Desktop Resolution : /usr/share/applications/code.desktop
 - Binary Path : /usr/bin/code

→ Launching Visual Studio Code...

✓ Application launched successfully.

 App : Visual Studio Code
 Command : code
 Running as : ali-zain (non-root)
 Status : Active

Session started.
```

---

### Run a Script File
```bash
zex --trigger script.py
zex --trigger main.rs
zex --trigger app.sh
```

**Output:**
```
Nice choice! Launching application...

→ Resolving file...
 - File Type Detection : Python script
 - Handler : python3

→ Executing script.py...

✓ Script executed successfully.

 File : script.py
 Handler : python3
 Exit code : 0

Session started.
```

---

### Pass Arguments
```bash
zex --trigger code /path/to/project
zex --trigger script.py --arg1 --arg2
zex --trigger app.sh param1 param2
```

---

### Dry-Run Mode (Check Without Executing)
```bash
zex --trigger code --dry-run
# Output: "Dry run: Would launch Visual Studio Code"
```

---

## Supported Applications

**Built-in (Default):**
- `code` - Visual Studio Code
- `vscodium` - Code - OSS
- `code-insiders` - VS Code Insiders
- `firefox` - Mozilla Firefox
- `chrome` - Google Chrome
- `chromium` - Chromium
- `vim` - Vim Editor
- `nano` - Nano Editor
- `git` - Git Version Control

**Add More:** Edit `~/.config/zai-trigger/config.toml`

---

## Supported File Types

| Extension | Handler | Description |
|-----------|---------|-------------|
| `.rs` | `rustc` | Rust source file |
| `.py` | `python3` | Python script |
| `.sh` | `bash` | Bash script |
| `.js` | `node` | JavaScript file |
| `.ts` | `ts-node` | TypeScript file |
| `.java` | `java` | Java source file |
| `.go` | `go run` | Go source file |
| `.cpp` | `g++` | C++ source file |
| `.c` | `gcc` | C source file |
| `.rb` | `ruby` | Ruby script |
| `.lua` | `lua` | Lua script |
| `.pl` | `perl` | Perl script |
| `.swift` | `swift` | Swift source file |
| `.kt` | `kotlinc` | Kotlin source file |
| `.scala` | `scala` | Scala source file |
| `.r` | `Rscript` | R script |
| `.php` | `php` | PHP script |
| `.clj` | `clojure` | Clojure script |
| `.hs` | `runhaskell` | Haskell source file |
| `.ex` | `elixir` | Elixir script |

**Add More Handlers:** Edit `~/.config/zai-trigger/config.toml`

---

## Smart Suggestions

If you mistype an app name, get smart suggestions:

```bash
zex --trigger vsc
```

**Output:**
```
Nice choice! Launching application...

 ✖ Application 'vsc' not found.

 Smart suggestions:
 - Did you mean: code ?
 - Did you mean: vscodium ?
```

---

## Root User Warning

Running GUI applications as root is discouraged:

```bash
sudo zex --trigger code
```

**Output:**
```
⚠ Warning: Running GUI applications as root is not recommended.

 This can cause permission issues with your config files and is a security risk.

 Better way:
 → Run without sudo: zex --trigger code

 If you must run as root, use these flags:
 - For Visual Studio Code: zex --trigger code --no-sandbox --user-data-dir=/root/.vscode
 - For browsers: zex --trigger code --no-sandbox

Error: Cannot safely run GUI app as root without proper flags.
```

**To Allow Root Execution:**
```bash
sudo zex --trigger code --no-sandbox --user-data-dir=/root/.code
```

---

## Error Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Application not found |
| 3 | File not found |
| 4 | Permission denied |
| 5 | Execution failed |
| 6 | Configuration error |
| 7 | Root execution forbidden |
| 8 | I/O error |
| 9 | UTF-8 encoding error |

**Usage:**
```bash
zex --trigger nonexistent
echo $? # Output: 2
```

---

## Configuration File

**Location:** `~/.config/zai-trigger/config.toml`

### Example Configuration

```toml
[known_apps]
code = { name = "code", description = "Visual Studio Code" }
my_app = { name = "my_app", description = "My Custom Application" }

[file_handlers]
py = { extension = "py", handler = "python3", description = "Python script" }
rs = { extension = "rs", handler = "rustc", description = "Rust source file" }
sh = { extension = "sh", handler = "bash", description = "Bash script" }

# Levenshtein distance for suggestions (default: 3)
levenshtein_threshold = 3
```

### Adding Custom Applications

```toml
[known_apps]
myapp = { name = "myapp", description = "My Cool Application" }
```

Then use:
```bash
zex --trigger myapp
```

### Adding Custom File Handlers

```toml
[file_handlers]
custom = { extension = "custom", handler = "custom-runner", description = "Custom format" }
```

Then use:
```bash
zex --trigger file.custom
```

---

## Logging

Enable logging for debugging:

```bash
RUST_LOG=debug ./trigger --trigger code
RUST_LOG=info ./trigger --trigger script.py
```

---

## Examples

### Run Python Script
```bash
zex --trigger hello.py --name World
```

### Compile and Run Rust
```bash
zex --trigger main.rs
```

### Execute Bash Script
```bash
zex --trigger deploy.sh production
```

### Open Project in VSCode
```bash
zex --trigger code ~/my-project
```

### Edit File with Vim
```bash
zex --trigger file.txt
# (requires vim in config or PATH)
```

### Run Node.js Script
```bash
zex --trigger app.js --port 3000
```

---

## Troubleshooting

### "Application not found"
- Check if the app is installed
- Add it to config.toml if it exists on your system
- Verify it's in PATH: `which appname`

### "File not found"
- Check file path and extension
- Ensure file exists: `ls -l filename`
- Add handler for extension in config.toml

### "Permission denied"
- Make sure file is readable: `chmod +x file.sh`
- Check current user permissions

### "Cannot safely run GUI app as root"
- Run without sudo: `zex --trigger code`
- Or use sandbox flags if needed: `sudo zex --trigger code --no-sandbox`

### Log Debug Info
```bash
RUST_LOG=debug zex --trigger filename
```

---

## Advanced Usage

### Multiple Arguments with Different Types
```bash
zex --trigger code /path/to/project --extensions-dir=/tmp
```

### Combining with Shell Pipes
```bash
# This works naturally since trigger inherits stdin/stdout
echo "data" | zex --trigger script.py

# Or redirect
zex --trigger process.sh < input.txt > output.txt
```

### As a Shebang Interpreter
Add to script:
```bash
#!/usr/bin/env trigger
# Won't work - use direct interpreter instead
```

---

## Performance

- **Startup Time:** ~5-10ms (minimal)
- **Memory Usage:** ~2-3 MB
- **Binary Size:** ~8-10 MB (with debug symbols)

---

## Development

### Run Tests
```bash
cargo test
```

### Check Code
```bash
cargo check
cargo clippy
```

### Build Debug Version
```bash
cargo build
./target/debug/trigger --trigger code
```

### Build Release Version
```bash
cargo build --release
./target/release/trigger --trigger code
```

---

## License
See LICENSE file

---

## Support
For issues and suggestions, check the repository.
