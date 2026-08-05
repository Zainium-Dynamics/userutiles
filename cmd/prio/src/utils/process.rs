use crate::error::{PrioError, Result};

/// Snapshot of a process's key scheduling attributes.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub nice: i32,
    pub cpu: f32, // percentage (per-core, may exceed 100 on multi-core)
    #[allow(dead_code)]
    pub memory: u64, // resident set size in bytes
}

// -- Niceness via libc --------------------------------------------------------

/// Read the current nice value for `pid` using `getpriority(2)`.
/// Returns 0 on any failure (safe default for display).
pub fn get_nice(pid: u32) -> i32 {
    // SAFETY: `__errno_location()` returns a pointer to the calling
    // thread's `errno` TLS cell, which is always valid to write/read from
    // this single thread — no aliasing or lifetime issues. `getpriority`
    // takes only plain integers (`pid` widened to `id_t`, also `u32` on
    // Linux) and dereferences nothing on the caller's side; a PID that no
    // longer exists just yields -1/ESRCH, handled by the errno check
    // below rather than by any unchecked pointer access.
    unsafe {
        // Clear errno before the call, because getpriority can legitimately
        // return -1 as a valid nice value.
        *libc::__errno_location() = 0;
        let prio = libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t);
        if *libc::__errno_location() != 0 {
            0
        } else {
            prio
        }
    }
}

/// Set the nice value for `pid`. Returns [`PrioError::PermissionDenied`] if
/// the caller lacks `CAP_SYS_NICE` to raise priority above 0.
pub fn set_nice(pid: u32, nice: i32) -> Result<()> {
    // SAFETY: `setpriority(2)` takes only plain integers — no pointers
    // or buffers involved. `pid as libc::id_t` is a lossless `u32` -> `u32`
    // widening on Linux; `nice` is validated by callers via
    // `validate_nice`/`cpu_level_to_nice` to lie within the kernel's
    // accepted `[-20, 19]` range before reaching here (and even an
    // out-of-range value would just be clamped/rejected by the kernel,
    // not cause UB).
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
    if rc != 0 {
        // SAFETY: `__errno_location()` returns a pointer to the calling
        // thread's `errno` TLS cell, valid to dereference; it was just set
        // by the failed `setpriority` call immediately above.
        let errno = unsafe { *libc::__errno_location() };
        return Err(if errno == libc::EPERM || errno == libc::EACCES {
            PrioError::PermissionDenied
        } else {
            PrioError::SystemError(format!("setpriority errno={}", errno))
        });
    }
    Ok(())
}

// -- Process List -------------------------------------------------------------

/// Return up to `count` processes sorted by ascending niceness (highest
/// priority first), then descending CPU usage as a tiebreaker.
pub fn get_top_processes(count: usize) -> Vec<ProcessInfo> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let mut list: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p| {
            let pid = p.pid().as_u32();
            ProcessInfo {
                pid,
                name: p.name().to_string(),
                nice: get_nice(pid),
                cpu: p.cpu_usage(),
                memory: p.memory(),
            }
        })
        .collect();

    list.sort_by(|a, b| {
        a.nice.cmp(&b.nice).then_with(|| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    list.truncate(count);
    list
}

// -- Process Lookup -----------------------------------------------------------

/// Find a process by PID or by name (first match).
pub fn find_process(target: &str) -> Result<ProcessInfo> {
    if let Ok(pid) = target.parse::<u32>() {
        find_by_pid(pid)
    } else {
        find_by_name(target)
    }
}

/// Look up a process by exact PID.
pub fn find_by_pid(pid: u32) -> Result<ProcessInfo> {
    use sysinfo::{Pid, System};

    let mut sys = System::new_all();
    sys.refresh_all();

    sys.process(Pid::from_u32(pid))
        .map(|p| ProcessInfo {
            pid,
            name: p.name().to_string(),
            nice: get_nice(pid),
            cpu: p.cpu_usage(),
            memory: p.memory(),
        })
        .ok_or(PrioError::ProcessNotFound(pid))
}

/// Look up the first process whose name matches `name` (case-insensitive).
pub fn find_by_name(name: &str) -> Result<ProcessInfo> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let name_lower = name.to_ascii_lowercase();

    sys.processes()
        .values()
        .find(|p| p.name().to_ascii_lowercase() == name_lower)
        .map(|p| {
            let pid = p.pid().as_u32();
            ProcessInfo {
                pid,
                name: p.name().to_string(),
                nice: get_nice(pid),
                cpu: p.cpu_usage(),
                memory: p.memory(),
            }
        })
        .ok_or_else(|| PrioError::ProcessNameNotFound(name.to_string()))
}

/// Check whether the calling process is running as root (UID 0).
pub fn is_root() -> bool {
    // SAFETY: `getuid(2)` takes no arguments, dereferences no pointers,
    // and cannot fail — it always returns the calling process's real UID.
    unsafe { libc::getuid() == 0 }
}
