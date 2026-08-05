# trace - Architecture Documentation

## System Overview

`trace` is a **universal system call tracing tool** that provides distro-agnostic process monitoring and syscall profiling for Linux systems.

### Design Principles

1. **Modularity** - Each component is independent and testable
2. **Extensibility** - Easy to add new syscall types or output formats
3. **Safety** - Memory-safe Rust implementation with minimal unsafe code
4. **Performance** - Optimized for low-overhead monitoring
5. **Clarity** - Well-documented, self-explanatory code

## Module Architecture

```
┌─────────────────────────────────────────────────────┐
│                    main.rs (CLI Entry)              │
│           Command routing & error handling           │
└──────────────────┬──────────────────────────────────┘
                   │
        ┌──────────┼──────────┐
        │          │          │
        ▼          ▼          ▼
   ┌────────┐ ┌────────┐ ┌──────────┐
   │ tracer │ │detector│ │  output  │
   └────────┘ └────────┘ └──────────┘
        │          │          │
   [Syscall]   [OSInfo]   [Table]
   [Process]   [Kernel]   [JSON]
   [Memory]           [YAML]
   [CPU]
   [Network]
```

## Component Details

### 1. CLI Module (`cli.rs`)

**Responsibility**: Command-line argument parsing and validation

```rust
pub struct Cli {
    pub process: Option<String>,      // Process name
    pub pid: Option<u32>,             // Process ID
    pub format: Option<String>,       // Output format
    pub output: Option<String>,       // Output directory
    pub verbose: bool,                // Debug logging
    pub live: bool,                   // Live tracing
}
```

**Features**:
- Argument validation
- Format selection
- Permission checks

---

### 2. Detector Module (`detector/`)

**Responsibility**: System environment detection

#### Components:
- **`os.rs`** - OS and distro detection
  - Reads `/etc/os-release`
  - Detects Linux distro and version
  - Falls back gracefully

- **`kernel.rs`** - Kernel version and arch detection
  - Parses `/proc/version`
  - Detects CPU architecture
  - Supports x86_64, ARM64, ARM

---

### 3. Tracer Module (`tracer/`)

**Responsibility**: Core process monitoring and syscall collection

#### Submodules:

##### `syscalls.rs` - System Call Tracking
```rust
pub struct SyscallTracer {
    stats: HashMap<String, SyscallStats>,
    syscall_names: HashMap<u64, String>,
}
```
- Maintains syscall statistics
- Maps syscall IDs to names
- Tracks counts and bytes
- Top N syscall queries

**Syscall Database**: 300+ x86_64 syscalls including:
- File operations (read, write, open, close, etc.)
- Process control (fork, exec, exit)
- Memory management (mmap, munmap, brk)
- Networking (socket, connect, send, recv)
- IPC mechanisms (pipe, semget, msgget)

##### `process.rs` - Process Information
```rust
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub status: String,
    pub memory_mb: u64,
    pub cpu_percent: f64,
    pub uptime: String,
}
```
- Query process by PID or name
- Extract process stats from `/proc/[pid]/stat`
- Calculate uptime from boot time
- Handles process death gracefully

##### `memory.rs` - Memory Stats
- Reads RSS (resident set size)
- Virtual memory size (VMS)
- Proportional set size (PSS)
- Uses procfs crate integration

##### `cpu.rs` - CPU Usage
- User and system time
- CPU percentage calculation
- Processes from `/proc/[pid]/stat`

##### `network.rs` - Network Activity
- Active connection count
- Per-process network data
- Reads `/proc/[pid]/net/tcp`
- Supports IPv4 and IPv6

---

### 4. Output Module (`output/`)

**Responsibility**: Result formatting and presentation

#### Formatters:

##### `table.rs` - Human-Readable Tables
- **Primary Output Format**
- Uses `prettytable-rs` crate
- Color-coded with `colored` crate
- Sections:
  - Process Information
  - Syscall Summary
  - Network Activity
- Responsive layout

**Example Output**:
```
Trace started on process: firefox (PID: 3245)

Process Information
  PID          : 3245
  Name         : firefox
  Status       : Running
  Memory       : 842 MB (RSS)
  
Syscall Summary
  Total syscalls   : 1,847
  Unique syscalls  : 42
  Top 3 syscalls   : read (412), write (289), poll (156)
```

##### `json.rs` - JSON Output
- Machine-readable format
- Standard JSON structure
- RFC3339 timestamps
- Suitable for automation

**Example Structure**:
```json
{
  "process": { /* process info */ },
  "syscalls": { "total": N, "unique": N, "top": [...] },
  "network": { /* network stats */ },
  "timestamp": "2025-04-05T12:45:33Z"
}
```

##### `yaml.rs` - YAML Output
- Human-friendly structured data
- Configuration-style output
- Alternative to JSON

---

### 5. Sandbox Module (`sandbox/`)

**Responsibility**: Security and privilege management

#### Features:
- **Privilege Dropping**: Safely downgrade from root
- **Permission Verification**: Check required capabilities
- **Safety Checks**: Prevent unsafe operations

**Functions**:
- `drop_privileges()` - Reduce privilege level if running as root
- `verify_permissions()` - Ensure ptrace capability

---

### 6. Utils Module (`utils/`)

**Responsibility**: Common utilities and error handling

#### Components:

##### `error.rs` - Error Types
```rust
pub enum TraceError {
    PermissionDenied(String),
    ProcessNotFound(String),
    PtraceError(String),
    IoError(String),
    ConfigError(String),
    OutputError(String),
}
```
- Implements `Display` and `Error`
- Converts from libc/nix errors
- Clear, actionable error messages

##### `logger.rs` - Logging Setup
- Initialize env_logger
- Conditional loglevel (Info/Debug)
- Formatted output

##### `fs.rs` - File Operations
- `ensure_output_dir()` - Create directories safely
- `generate_filename()` - Timestamped output names
- `write_output_file()` - Save results to disk

---

## Data Flow

```
User Input (CLI)
    │
    ▼
┌─────────────────────┐
│  CLI Validation     │
│  & Arg Parsing      │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Process Lookup     │  ← detector/os.rs
│  (name or PID)      │  ← tracer/process.rs
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Data Collection    │
│  (All Tracer Mods)  │
└────────┬────────────┘
         │
    ┌────┼────┬────────┐
    │    │    │        │
    ▼    ▼    ▼        ▼
[Process][Memory][CPU][Network]
         │    │    │        │
         ▼    ▼    ▼        ▼
         TraceData struct
         │
         ▼
┌─────────────────────┐
│  Format Selection   │
│  (output module)    │
└────────┬────────────┘
         │
    ┌────┼─────────┐
    │    │         │
    ▼    ▼         ▼
[Table][JSON]   [YAML]
    │    │         │
    └────┼─────────┘
         │
         ▼
    (Display/Save)
```

## Error Handling Strategy

```rust
// Result type alias
pub type TraceResult<T> = Result<T, TraceError>;

// Cascading conversions
io::Error → TraceError::IoError
nix::Error → TraceError::PtraceError
```

**Error Recovery**:
- Process not found → Suggest alternatives
- Permission denied → Explain privilege requirements
- IO errors → Detailed context
- Invalid config → Clear remediation steps

---

## Dependencies

### Core Dependencies
| Crate | Purpose |
|-------|---------|
| `libc` | Low-level Linux API |
| `nix` | Safe syscall wrappers |
| `procfs` | `/proc` filesystem parsing |
| `clap` | CLI argument parsing |
| `serde` | Data serialization |
| `anyhow` / `thiserror` | Error handling |

### Output Dependencies
| Crate | Purpose |
|-------|---------|
| `prettytable-rs` | Table formatting |
| `colored` | Terminal colors |
| `serde_json` | JSON output |
| `serde_yaml` | YAML output |

### Utility Dependencies
| Crate | Purpose |
|-------|---------|
| `chrono` | Timestamp handling |
| `regex` | Pattern matching |
| `log` | Logging framework |
| `env_logger` | Log initialization |

---

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    // Syscall name mapping tests
    // Process lookup tests
    // Output formatting tests
}
```

### Integration Tests
```rust
// Full CLI execution tests
// End-to-end process tracing
// Output format validation
```

---

## Build System

### `Cargo.toml` Configuration
- Edition: 2021 (MSRV: 1.70.0)
- Profile: Release with LTO
- Codegen units: 1 (optimize for speed)

### `build.rs`
- Pre-build validation
- Version info injection (optional)
- Platform-specific checks

---

## Security Considerations

1. **Privilege Escalation Prevention**
   - Verify before ptrace
   - Log privilege operations
   - No file execution

2. **Data Integrity**
   - No modification of traced processes
   - Read-only monitoring
   - Atomic operations

3. **Error Handling**
   - No hidden errors
   - Clear failure states
   - User-actionable messages

4. **Resource Management**
   - Bounded memory usage
   - Proper cleanup on exit
   - No file descriptor leaks

---

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Process lookup | O(n) | Linear scan of `/proc` if by name |
| Syscall tracking | O(1) | HashMap insert/update |
| Output formatting | O(n log n) | Sorting syscalls, n = unique syscalls |
| Memory footprint | O(m) | m = number of unique syscalls |

**Optimization Tips**:
- Use PID when available (O(1) vs O(n))
- JSON output faster than table (no formatting)
- Large syscall counts may slow table layout

---

## Future Enhancements

1. **Advanced Filtering**
   - Filter by syscall type
   - Time-based filtering
   - Regex matching

2. **Real-time Streaming**
   - Live syscall events
   - Progress bars
   - Network throughput visualization

3. **Statistical Analysis**
   - Histogramming
   - Percentile calculations
   - Anomaly detection

4. **Integration**
   - Log aggregation support
   - Prometheus metrics export
   - Distributed tracing

---

## References

- Linux man pages: ptrace(2), proc(5)
- Kernel syscall table: https://syscalls.kernelgrok.com/
- Arch support: x86_64, ARM64, ARM (32-bit)

