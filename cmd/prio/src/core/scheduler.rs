use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

use crate::core::cgroup::CgroupManager;
use crate::error::{PrioError, Result};
use crate::utils::priority::IoMode;

// -- Spawn Configuration -------------------------------------------------------

/// All scheduling parameters collected from the CLI and config for a single
/// process launch.
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    /// The command tokens to execute (argv).
    pub command: Vec<String>,
    /// Resolved niceness level (-20..+19).
    pub nice: i32,
    /// Optional I/O scheduling class.
    pub io_mode: Option<IoMode>,
    /// Optional memory ceiling in bytes (requires cgroup support + root).
    pub max_ram: Option<u64>,
    /// Whether auto-throttling is requested.
    #[allow(dead_code)]
    pub auto: bool,
    /// Verbose output flag (forwarded to caller for display decisions).
    #[allow(dead_code)]
    pub verbose: bool,
}

// -- Spawning ------------------------------------------------------------------

/// Spawn `config.command` with the configured scheduling parameters.
///
/// Niceness and I/O priority are applied in the child process **before** the
/// `exec` syscall via [`CommandExt::pre_exec`], eliminating any scheduling
/// race. The cgroup (if requested) is assigned in the parent immediately
/// after the fork returns.
///
/// Returns the [`Child`] handle so the caller can wait on it and later clean
/// up any associated resources.
pub fn spawn(config: &SpawnConfig) -> Result<Child> {
    if config.command.is_empty() {
        return Err(PrioError::SpawnError("no command provided".into()));
    }

    let nice_val = config.nice;
    let io_ioprio = config.io_mode.as_ref().map(|m| m.ioprio_value());

    let mut cmd = Command::new(&config.command[0]);
    if config.command.len() > 1 {
        cmd.args(&config.command[1..]);
    }

    // -- Child-side pre-exec setup -----------------------------------------
    // SAFETY: `pre_exec` requires the closure to behave like a
    // signal handler: it runs in the forked child (which is
    // single-threaded — a fresh copy of only the calling thread) between
    // `fork` and `exec`, so it must not allocate heap memory or touch
    // any lock that another thread might have held at fork time, or the
    // child can deadlock. The closure below only calls `libc::setpriority`
    // and the raw `ioprio_set` syscall with plain integer arguments
    // (`nice_val`/`io_ioprio` are `Copy` values captured by the closure,
    // not pointers) — no allocation, no locking, no pointer dereferencing.
    // `who=0` means "the calling process/thread", i.e. the child itself,
    // and per `ioprio_set(2)` argument order is
    // `(which=IOPRIO_WHO_PROCESS, who, ioprio)`, matched here as
    // `(1, 0, ioprio)`. Both calls' failures are deliberately ignored (the
    // process still runs at default priority), which cannot cause UB.
    unsafe {
        cmd.pre_exec(move || {
            // Set niceness. Failure is silent: the process still runs.
            libc::setpriority(libc::PRIO_PROCESS, 0, nice_val);

            // Set I/O priority.
            // SYS_ioprio_set = 251 (x86-64); IOPRIO_WHO_PROCESS = 1; pid 0 = self.
            if let Some(ioprio) = io_ioprio {
                libc::syscall(251, 1i64, 0i64, ioprio as i64);
            }

            Ok(())
        });
    }

    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            PrioError::PermissionDenied
        } else {
            PrioError::SpawnError(e.to_string())
        }
    })?;

    let pid = child.id();

    // -- Parent-side cgroup setup ------------------------------------------
    if let Some(ram_bytes) = config.max_ram {
        match CgroupManager::new(&format!("prio_{}", pid)) {
            Ok(cg) => {
                // Errors here are non-fatal — log and continue without cgroup.
                let _ = cg.set_memory_limit(ram_bytes);
                let _ = cg.add_process(pid);
                // Leak the manager so the cgroup lives until drop at program exit.
                std::mem::forget(cg);
            }
            Err(e) => {
                // Print a warning but do not abort — the process is already running.
                eprintln!("prio: cgroup warning: {}", e);
            }
        }
    }

    Ok(child)
}

// -- Apply to Existing PID -----------------------------------------------------

/// Apply priority settings to an already-running process by PID.
/// Silently skips I/O and cgroup adjustments on permission failure,
/// returning the first hard error encountered.
pub fn apply_to_pid(pid: u32, nice: i32, io_mode: Option<&IoMode>) -> Result<()> {
    crate::utils::process::set_nice(pid, nice)?;

    if let Some(mode) = io_mode {
        let ioprio = mode.ioprio_value();
        // SAFETY: raw `syscall()` with only integer arguments — no
        // pointers are passed, so there is nothing to dereference and no
        // buffer/lifetime invariant to uphold. Per `ioprio_set(2)` the
        // argument order is `(which, who, ioprio)`; `1i64` is
        // `IOPRIO_WHO_PROCESS`, `pid as i64` is the target PID (always
        // non-negative since `pid: u32`), and `ioprio` is the
        // class/data-encoded value built by `IoMode::ioprio_value`. On
        // x86-64, syscall number 251 is `SYS_ioprio_set`. If the syscall
        // number or arguments were wrong the kernel would simply return an
        // error (e.g. ENOSYS/EINVAL) via `rc`, not corrupt memory.
        let rc = unsafe { libc::syscall(251, 1i64, pid as i64, ioprio as i64) };
        if rc < 0 {
            // SAFETY: `__errno_location()` returns a pointer to the
            // calling thread's `errno` TLS cell, which is always valid to
            // dereference; it was just set by the failed `syscall` call
            // immediately above.
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::EPERM || errno == libc::EACCES {
                return Err(PrioError::PermissionDenied);
            }
            return Err(PrioError::IoPriorityError(format!(
                "ioprio_set errno={}",
                errno
            )));
        }
    }

    Ok(())
}

// -- Boost ---------------------------------------------------------------------

/// Quick-boost: set the given PID's niceness to `boost_nice` (typically -12).
/// Returns the old niceness value so the caller can display it.
pub fn boost_pid(pid: u32, boost_nice: i32) -> Result<i32> {
    let old_nice = crate::utils::process::get_nice(pid);
    crate::utils::process::set_nice(pid, boost_nice)?;
    Ok(old_nice)
}

// -- Reset ---------------------------------------------------------------------

/// Reset a process's niceness to 0 (the kernel default).
pub fn reset_pid(pid: u32) -> Result<()> {
    crate::utils::process::set_nice(pid, 0)
}
