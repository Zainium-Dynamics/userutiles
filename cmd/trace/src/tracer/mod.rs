pub mod syscalls;
pub mod process;
pub mod memory;
pub mod cpu;
pub mod network;

pub use syscalls::{SyscallTracer, SyscallStats, SyscallInfo};
pub use process::ProcessInfo;
pub use memory::MemoryStats;
pub use cpu::CpuStats;
pub use network::NetworkStats;

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
