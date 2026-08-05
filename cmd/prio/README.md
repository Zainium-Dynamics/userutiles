# prio

**ZainiumOS Process Priority & Resource Scheduler**

`prio` is a production-grade Zainium OS CLI tool that combines process niceness,
I/O scheduling class, cgroup memory limits, automatic CPU-temperature
throttling, and time-bound priority management into a single, ergonomic
command with premium terminal output.

```
prio --cpu 95 --io realtime --max-ram 6G --time 30m ./video_renderer
```

---

## Features

- **Niceness control** — set `-n -20` through `+19` for any command or PID
- **CPU level shorthand** — `--cpu 0-100` mapped linearly to niceness
- **I/O scheduling** — `realtime`, `high`, `normal`, or `idle` via `ioprio_set(2)`
- **Memory limits** — cgroup v2/v1 auto-detected; supports fractional sizes (`2.5G`)
- **Quick boost** — `--boost <PID|name>` snaps a running process to `-12` niceness
- **Auto-throttling** — `--auto` monitors CPU temperature and load average, adjusting priority dynamically
- **Time-bound mode** — `--time 30m` auto-reverts the process to its original niceness after the duration
- **Process list** — `--list` renders the top 15 processes sorted by priority
- **Premium output** — ZainiumOS colour scheme with cyber-tech feel; no colour when piped

---

## Installation

### From source (recommended)

```bash
git clone https://github.com/zainiumos/prio
cd prio
cargo build --release
sudo install -m 755 target/release/prio /usr/local/bin/prio
```

### Capabilities (optional, for non-root boost)

Grant `CAP_SYS_NICE` so users can raise priority without `sudo`:

```bash
sudo setcap cap_sys_nice+ep /usr/local/bin/prio
```

---

## Usage

```
prio [OPTIONS] <COMMAND>
prio [OPTIONS] --pid <PID>
```

| Flag | Description |
|------|-------------|
| `-n, --nice <LEVEL>` | Niceness −20 (highest) to +19 (lowest) |
| `-c, --cpu <LEVEL>` | CPU priority 0–100 (mapped to niceness) |
| `--io <MODE>` | I/O class: `realtime`, `high`, `normal`, `idle` |
| `--max-ram <SIZE>` | Memory ceiling e.g. `4G`, `2.5G`, `512M` |
| `--pid <PID>` | Apply settings to an already-running process |
| `--boost <PID\|CMD>` | Quick-boost a running process to −12 niceness |
| `--reset <PID>` | Restore niceness to 0 |
| `--auto` | Dynamic auto-throttling via temp + load monitoring |
| `--time <DURATION>` | Time-bound boost: `30s`, `10m`, `2h` |
| `--list` | Show top 15 processes by priority |
| `-v, --verbose` | Verbose diagnostic output |

---

## Examples

### Basic priority boost

```
prio -n -10 cargo build --release

Setting priority...

 Command : cargo build --release
 Niceness : -10
 CPU : High
 PID : 18742

✓ Priority boosted successfully
```

### Full power mode

```
prio --cpu 95 --io realtime --max-ram 6G --time 30m ./video_renderer

Setting enhanced priority...

 Command : ./video_renderer
 Niceness : -15
 CPU : 95%
 I/O : Realtime
 Max RAM : 6G
 Duration : 30 minutes

✓ Supercharged successfully (will auto-revert after 30 minutes)
 PID : 20481
```

### Quick boost of a running process

```
prio --boost 17345

Boosting process...

 PID : 17345
 Process : firefox
 Old Nice : 0
 New Nice : -12

✓ Process boosted for better responsiveness
```

### Process list

```
prio --list

Top Processes by Priority:

 PID Process Nice CPU% Status
 18492 cargo build -15 87% High
 17345 firefox 0 23% Normal
 19231 compile-heavy +10 45% Low
 20145 system-monitor 5 12% Normal

✓ Showing top 15 processes
```

### Auto-throttling mode

```
prio --auto ./heavy-compile.sh

Smart Mode Activated

 Command : ./heavy-compile.sh
 Mode : Auto-Throttling
 Monitoring : CPU Temp + Load

✓ Running with dynamic priority management
```

### Error output

```
prio -n -20 code

Setting priority...

 Command : code
 Niceness : -20

✖ Failed to set priority
 Reason : permission denied
 Fix : Re-run with sudo, or grant CAP_SYS_NICE to the binary.
```

---

## Configuration

`prio` reads `~/.config/prio/config.toml` on startup. The file is optional;
all fields fall back to sensible built-in defaults.

```toml
[defaults]
nice = 0 # applied when no -n flag is given
boost_nice = -12 # used by --boost

[auto]
temp_threshold = 80.0 # °C above which throttling kicks in
temp_hysteresis = 10.0 # degrees below threshold before unthrottling
check_interval_secs = 5 # polling interval
load_multiplier = 1.5 # throttle if load > cpu_count × multiplier

[list]
max_processes = 15
```

---

## Privileges

| Operation | Requirement |
|-----------|-------------|
| Negative niceness | `CAP_SYS_NICE` or `root` |
| I/O realtime class | `CAP_SYS_ADMIN` or `root` |
| cgroup memory limits | Write access to `/sys/fs/cgroup` (typically `root`) |
| Positive niceness, `--list`, `--boost` to 0+ | None |

---

## Requirements

- Linux kernel ≥ 4.5 (cgroup v2 for memory limits)
- Rust ≥ 1.75 (build only)
- No runtime dependencies beyond glibc

---

## License

MIT — see [LICENSE](LICENSE).
