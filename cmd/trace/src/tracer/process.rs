use crate::utils::{TraceError, TraceResult};
use procfs::process::Process;
use std::time::SystemTime;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub status: String,
    pub memory_mb: u64,
    pub cpu_percent: f64,
    pub uptime: String,
}

impl ProcessInfo {
    pub fn from_pid(pid: u32) -> TraceResult<Self> {
        let proc = Process::new(pid as i32)
            .map_err(|_| TraceError::ProcessNotFound(format!("PID {}", pid)))?;

        let stat = proc
            .stat()
            .map_err(|_| TraceError::ProcessNotFound(format!("Cannot read stats for PID {}", pid)))?;

        let status = proc
            .status()
            .map_err(|_| TraceError::ProcessNotFound(format!("Cannot read status for PID {}", pid)))?;

        let name = stat.comm.clone();
        let memory_mb = status.vmrss.unwrap_or(0) / 1024; // Convert to MB
        let cpu_percent = 0.0; // Simplified - would need more data for accurate calculation

        Ok(ProcessInfo {
            pid,
            name,
            status: "Running".to_string(),
            memory_mb,
            cpu_percent,
            uptime: Self::calculate_uptime(&stat),
        })
    }

    fn calculate_uptime(_stat: &procfs::process::Stat) -> String {
        // Get boot time from /proc/stat
        if let Ok(boot_time) = Self::get_boot_time() {
            if let Ok(elapsed) = SystemTime::now().duration_since(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(boot_time as u64),
            ) {
                let secs = elapsed.as_secs();
                let hours = secs / 3600;
                let minutes = (secs % 3600) / 60;
                return format!("{}h {}m", hours, minutes);
            }
        }
        "Unknown".to_string()
    }

    fn get_boot_time() -> TraceResult<u64> {
        use std::fs;
        let content = fs::read_to_string("/proc/stat")?;
        for line in content.lines() {
            if line.starts_with("btime") {
                if let Some(time_str) = line.split_whitespace().nth(1) {
                    return Ok(time_str.parse().unwrap_or(0));
                }
            }
        }
        Ok(0)
    }

    pub fn from_name(name: &str) -> TraceResult<Self> {
        // Find process by name using procfs
        let all_procs = procfs::process::all_processes()
            .map_err(|_| TraceError::ProcessNotFound(name.to_string()))?;

        for proc in all_procs {
            if let Ok(proc) = proc {
                if let Ok(stat) = proc.stat() {
                    if stat.comm.contains(name) {
                        return Self::from_pid(proc.pid() as u32);
                    }
                }
            }
        }

        Err(TraceError::ProcessNotFound(name.to_string()))
    }

    pub fn exists(pid: u32) -> bool {
        Process::new(pid as i32).is_ok()
    }
}
