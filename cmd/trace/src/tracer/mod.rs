pub mod cpu;
pub mod memory;
pub mod network;
pub mod process;
pub mod syscalls;

pub use cpu::CpuStats;
pub use memory::MemoryStats;
pub use network::NetworkStats;
pub use process::ProcessInfo;
pub use syscalls::{SyscallInfo, SyscallStats, SyscallTracer};

use crate::utils::TraceResult;

#[derive(Debug, Clone)]
pub struct TraceData {
    pub process: ProcessInfo,
    pub syscalls: SyscallTracer,
    pub memory: MemoryStats,
    pub cpu: CpuStats,
    pub network: NetworkStats,
}

impl TraceData {
    pub fn collect(pid: u32) -> TraceResult<Self> {
        Ok(TraceData {
            process: ProcessInfo::from_pid(pid)?,
            syscalls: SyscallTracer::new(),
            memory: MemoryStats::from_pid(pid)?,
            cpu: CpuStats::from_pid(pid)?,
            network: NetworkStats::from_pid(pid)?,
        })
    }
}
