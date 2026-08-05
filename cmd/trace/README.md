# trace

**Process & syscall inspector for Zainium OS**  
Product of **Zainium Dynamics** (`zex-utils`).

### Features

- Process lookup by name or PID
- Memory / CPU / network snapshot
- Syscall summary tables
- Output formats: **table** (default) and **TOML** only (no JSON/YAML)
- Privilege hardening via `zex-seccomp`

### Usage

```bash
# Basic
trace --process firefox
trace --pid 1234

# TOML machine output
trace --process code --toml
trace --pid 1234 --format toml

# Live flag (reserved)
trace --process firefox --live

# System helpers
trace info
trace processes

# Multicall
zex-utils trace --process bash
```

### Notes

- Requires sufficient privileges to inspect other users' processes.
- Save output: `trace --pid 1 --toml -o /tmp/out`
