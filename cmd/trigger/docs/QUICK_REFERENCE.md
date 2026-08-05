# QUICK REFERENCE - zai-trigger v0.2.0

## Installation
```bash
cargo build --release
./target/release/trigger --trigger <app|file>
```

## Basic Commands

| Task | Command | Example |
|------|---------|---------|
| Launch app | `zex --trigger <app>` | `zex --trigger code` |
| Run script | `zex --trigger <file>` | `zex --trigger script.py` |
| With args | `zex --trigger <target> <args>` | `zex --trigger code /path` |
| Dry run | `zex --trigger <target> --dry-run` | `zex --trigger code --dry-run` |

## Supported Apps (Default)
`code` `vscodium` `code-insiders` `firefox` `chrome` `chromium` `vim` `nano` `git`

## Supported File Types
`rs` `py` `sh` `js` `ts` `java` `go` `cpp` `c` `rb` `lua` `pl` `swift` `kt` `scala` `r` `php` `clj` `hs` `ex`

## Configuration
- Location: `~/.config/zai-trigger/config.toml`
- Template: `config.example.toml`
- Custom apps: Add in `[known_apps]`
- Custom handlers: Add in `[file_handlers]`

## Error Codes
| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | App not found |
| 3 | File not found |
| 5 | Execution failed |
| 7 | Root forbidden |

## Logging
```bash
RUST_LOG=debug zex --trigger code
RUST_LOG=info zex --trigger script.py
```

## Examples
```bash
# Applications
zex --trigger code ~/project
zex --trigger firefox

# Scripts
zex --trigger main.rs
zex --trigger app.py --verbose
zex --trigger deploy.sh production

# Dry run
zex --trigger code --dry-run
```

## Troubleshooting
| Issue | Solution |
|-------|----------|
| App not found | Check PATH: `which appname` |
| File not found | Verify file exists: `ls -l file` |
| Permission denied | Make executable: `chmod +x file` |
| Root error | Run without sudo or add `--no-sandbox` |

## Module Structure
- `main.rs` - CLI entry
- `trigger.rs` - Core logic
- `config.rs` - Configuration
- `error.rs` - Error types

---
**Documentation:** See USAGE_GUIDE.md | **Technical:** See TECHNICAL_SUMMARY.md 
**Migration:** See MIGRATION_REPORT.md | **Review:** See CODE_REVIEW_REPORT.md
