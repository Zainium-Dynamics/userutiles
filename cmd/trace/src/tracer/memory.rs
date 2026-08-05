use crate::utils::TraceResult;
use procfs::process::Process;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryStats {
    pub rss_mb: u64,
    pub vms_mb: u64,
}

impl MemoryStats {
    pub fn from_pid(pid: u32) -> TraceResult<Self> {
        let proc = Process::new(pid as i32)?;
        let status = proc.status()?;

        let rss_mb = status.vmrss.unwrap_or(0) / 1024;
        let vms_mb = status.vmsize.unwrap_or(0) / 1024;

        Ok(MemoryStats {
            rss_mb,
            vms_mb,
        })
    }
}
