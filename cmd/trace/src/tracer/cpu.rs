use crate::utils::TraceResult;
use procfs::process::Process;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CpuStats {
    pub user_time: u64,
    pub system_time: u64,
    pub cpu_percent: f64,
}

impl CpuStats {
    pub fn from_pid(pid: u32) -> TraceResult<Self> {
        let proc = Process::new(pid as i32)?;
        let stat = proc.stat()?;

        Ok(CpuStats {
            user_time: stat.utime,
            system_time: stat.stime,
            cpu_percent: 0.0, // Simplified
        })
    }
}
