use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

use crate::core::cgroup::CgroupManager;
use crate::error::{PrioError, Result};
use crate::utils::priority::IoMode;

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

    // SAFETY: `pre_exec` requires the closure to behave like a
    unsafe {
        cmd.pre_exec(move || {
            libc::setpriority(libc::PRIO_PROCESS, 0, nice_val);

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

    if let Some(ram_bytes) = config.max_ram {
        match CgroupManager::new(&format!("prio_{}", pid)) {
            Ok(cg) => {
                let _ = cg.set_memory_limit(ram_bytes);
                let _ = cg.add_process(pid);
                std::mem::forget(cg);
            }
            Err(e) => {
                eprintln!("prio: cgroup warning: {}", e);
            }
        }
    }

    Ok(child)
}

/// Apply priority settings to an already-running process by PID.
/// Silently skips I/O and cgroup adjustments on permission failure,
/// returning the first hard error encountered.
pub fn apply_to_pid(pid: u32, nice: i32, io_mode: Option<&IoMode>) -> Result<()> {
    crate::utils::process::set_nice(pid, nice)?;

    if let Some(mode) = io_mode {
        let ioprio = mode.ioprio_value();
        // SAFETY: raw `syscall()` with only integer arguments — no
        let rc = unsafe { libc::syscall(251, 1i64, pid as i64, ioprio as i64) };
        if rc < 0 {
            // SAFETY: `__errno_location()` returns a pointer to the
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

/// Quick-boost: set the given PID's niceness to `boost_nice` (typically -12).
/// Returns the old niceness value so the caller can display it.
pub fn boost_pid(pid: u32, boost_nice: i32) -> Result<i32> {
    let old_nice = crate::utils::process::get_nice(pid);
    crate::utils::process::set_nice(pid, boost_nice)?;
    Ok(old_nice)
}

/// Reset a process's niceness to 0 (the kernel default).
pub fn reset_pid(pid: u32) -> Result<()> {
    crate::utils::process::set_nice(pid, 0)
}
