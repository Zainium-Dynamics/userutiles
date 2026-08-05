# prio — Architecture Documentation

## Overview

`prio` is a single-binary Linux process scheduler written in Rust. It wraps
five kernel interfaces — `setpriority(2)`, `ioprio_set(2)`, cgroup v2/v1
memory controllers, `SIGTERM`/`SIGINT` signal handling, and the thermal sensor
sysfs tree — behind one ergonomic CLI.

All output uses the ZainiumOS colour palette defined in `src/ui/display.rs`;
no shell escaping is required because the `colored` crate generates ANSI
sequences directly, and coloured output is automatically disabled when stdout
is not a TTY.

---

## Directory Structure

```
prio/
├── build.rs ← Compile-time metadata (build date, target triple)
├── Cargo.toml ← Workspace manifest, feature flags, release profile
├── src/
│ ├── main.rs ← CLI dispatch, orchestration, signal handler
│ ├── cli.rs ← Clap derive-based argument definitions
│ ├── config.rs ← ~/.config/prio/config.toml loader (TOML/serde)
│ ├── error.rs ← thiserror PrioError enum, fix_hint() method
│ ├── core/
│ │ ├── scheduler.rs ← spawn(), apply_to_pid(), boost_pid(), reset_pid()
│ │ ├── cgroup.rs ← CgroupManager: cgroup v2/v1 auto-detect + memory.max
│ │ └── monitor.rs ← AutoMonitor: background temp/load polling thread
│ ├── utils/
│ │ ├── priority.rs ← IoMode, cpu_level_to_nice, parse_memory, format_*
│ │ ├── process.rs ← get_nice/set_nice (libc), get_top_processes (sysinfo)
│ │ └── timebound.rs ← parse_duration, format_duration, schedule_reset
│ └── ui/
│ └── display.rs ← All terminal rendering; ZainiumOS colour scheme
└── tests/
 ├── integration_test.rs ← End-to-end binary tests via assert_cmd
 └── cli_test.rs ← Unit tests for parsers and utility functions
```

---

## Core Data Flow

```
main()
 │
 ├─ Cli::parse() (clap)
 ├─ Config::load() (~/.config/prio/config.toml, or defaults)
 │
 ├─ --list ────────────► get_top_processes() ──► print_process_list()
 │
 ├─ --reset ─────────────► reset_pid() ──► print_reset()
 │
 ├─ --boost ─────────────► find_process()
 │ boost_pid() ──► print_boost()
 │
 ├─ --pid ─────────────► find_by_pid()
 │ apply_to_pid() ──► print_apply()
 │ schedule_reset() (optional)
 │
 └─ <COMMAND> ───────────► resolve_nice()
 parse_memory() (optional)
 parse_duration() (optional)
 scheduler::spawn()
 ├─ Command::pre_exec → setpriority + ioprio_set
 └─ CgroupManager::add_process (optional, post-fork)
 AutoMonitor::start() (optional --auto)
 schedule_reset() (optional --time)
 child.wait()
```

---

## Module Responsibilities

### `src/main.rs`

The entry point performs three tasks only: argument/config loading, routing to
the appropriate sub-operation, and top-level error rendering. No business
logic lives here.

### `src/cli.rs`

A fully self-contained Clap `derive`-based struct. The help template is
embedded as a const string so the ZainiumOS heading always appears, regardless
of terminal width. `Cli::is_empty()` allows `main` to print help when the
user invokes `prio` with no arguments.

### `src/config.rs`

Loads `~/.config/prio/config.toml` using `serde` + `toml`. Every field has a
`Default` implementation, so the file is entirely optional. The home
directory is resolved by reading `$HOME` first, then `/etc/passwd` as a
fallback — no additional crate dependency required.

### `src/error.rs`

`PrioError` is a `thiserror` enum covering every failure mode. The
`fix_hint()` method returns an `Option<String>` so `display::print_prio_error`
can always show a contextual remediation suggestion below the error message.

### `src/core/scheduler.rs`

`spawn()` uses `CommandExt::pre_exec` to call `setpriority(2)` and
`ioprio_set(2)` in the child between `fork` and `exec`. This eliminates the
scheduling gap present in tools that set priority after the process is already
running.

`SpawnConfig` is a plain data struct with no `Arc` or `Mutex` — it is
constructed once in `main`, used for a single call, and then dropped.

### `src/core/cgroup.rs`

`CgroupManager::new()` auto-detects whether the host runs cgroup v2 (presence
of `/sys/fs/cgroup/cgroup.controllers`) or v1 (presence of
`/sys/fs/cgroup/memory`). All writes use absolute sysfs paths; there are no
external cgroup management binaries. The `Drop` implementation removes the
cgroup directory once the process exits, keeping the system clean.

### `src/core/monitor.rs`

`AutoMonitor::start()` spawns a named daemon thread (`prio-monitor`) that
polls `/sys/class/thermal/thermal_zone*/temp` and `/proc/loadavg` at a
configurable interval. Priority adjustments are stored in an `AtomicI32` so
they are visible to the parent thread for verbose display without any mutex
contention. The `stop` flag is an `AtomicBool`; the thread exits cleanly on
the next iteration after `stop()` is called or the struct is dropped.

### `src/utils/priority.rs`

Pure functions with no I/O side-effects. `cpu_level_to_nice` performs a
linear mapping: `nice = 19 − floor(level × 39 / 100)`. `parse_memory`
handles `G`, `GB`, `M`, `MB`, `K`, `KB` suffixes with fractional gigabyte
support.

### `src/utils/timebound.rs`

`schedule_reset` spawns a detached thread that sleeps for the requested
duration then calls `setpriority` to restore the original niceness. If the
target process has already exited, the syscall fails silently — which is the
correct behaviour.

### `src/ui/display.rs`

Every public function in this module corresponds to exactly one named UI mode.
No business logic appears here; the functions accept plain data and render it.
The colour palette is defined as module-level inline functions that delegate
to the `colored` crate.

---

## Scheduling Mechanics

### Niceness

`nice` maps to `setpriority(PRIO_PROCESS, pid, nice)`. Values in the range
`[-20, -1]` require `CAP_SYS_NICE` or root. Values `[0, 19]` are
unprivileged.

### I/O Priority

`ioprio_set(IOPRIO_WHO_PROCESS, pid, ioprio)` uses Linux's inline `syscall`
since glibc does not expose a wrapper. The bitmask encoding is:

```
bits 15-13 class (1 = Realtime, 2 = Best-effort, 3 = Idle)
bits 2- 0 priority within class (0 = highest, 7 = lowest)
```

Realtime class requires root (`CAP_SYS_ADMIN`).

### Memory Limits (cgroup)

On cgroup v2: `echo <bytes> > /sys/fs/cgroup/prio/<name>/memory.max` 
On cgroup v1: `echo <bytes> > /sys/fs/cgroup/memory/prio/<name>/memory.limit_in_bytes`

Both require write access to the cgroup filesystem (root by default, or a
delegated cgroup namespace).

---

## Error Handling Strategy

All fallible operations return `Result<T, PrioError>`. The `?` operator
propagates errors to `main`, where `display::print_prio_error` renders them
with the ZainiumOS colour scheme (red `✖`, yellow reason, cyan fix hint).
`proc::exit(1)` is called after rendering so the shell receives a non-zero
exit code without a Rust panic message polluting the output.

---

## Build Profile

The release profile is configured for maximum performance and minimum binary
size:

| Setting | Value | Effect |
|------------------|----------|----------------------------------------|
| `opt-level` | `3` | Full optimisation |
| `lto` | `true` | Cross-crate link-time optimisation |
| `codegen-units` | `1` | Single LLVM codegen unit (best LTO) |
| `strip` | `true` | Remove debug symbols from final binary |
| `panic` | `abort` | No unwinding machinery (smaller binary)|

---

## Security Considerations

`prio` never stores credentials. Privileged operations (negative niceness,
Realtime I/O, cgroup writes) require the caller to hold `CAP_SYS_NICE`,
`CAP_SYS_ADMIN`, or run as root. The binary does not set the SUID bit.
When permission is denied, `prio` emits a clear remediation message and exits
with code 1.

---

## Testing

Integration tests in `tests/integration_test.rs` spawn the compiled binary via
`assert_cmd`, testing exit codes, stdout content, and error paths. Unit tests
in `tests/cli_test.rs` cover all pure utility functions (parsers,
format helpers, I/O mode enum) without any OS-level side effects, so they run
correctly on non-Linux CI hosts.
