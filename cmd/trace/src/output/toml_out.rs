//! Machine-readable TOML output (user_utils uses TOML only — never JSON/YAML).

use crate::tracer::TraceData;
use serde::Serialize;

#[derive(Serialize)]
struct Out<'a> {
    process: ProcessOut<'a>,
    syscalls: SyscallsOut,
    network: NetworkOut,
    timestamp: String,
}

#[derive(Serialize)]
struct ProcessOut<'a> {
    pid: u32,
    name: &'a str,
    status: &'a str,
    memory_mb: u64,
    cpu_percent: f64,
}

#[derive(Serialize)]
struct SyscallsOut {
    total: u64,
    unique: usize,
    top: Vec<String>,
    detailed: Vec<SyscallRow>,
}

#[derive(Serialize)]
struct SyscallRow {
    name: String,
    count: u64,
    bytes: u64,
}

#[derive(Serialize)]
struct NetworkOut {
    active_connections: u32,
    bytes_sent: u64,
    bytes_received: u64,
}

pub fn format_toml(data: &TraceData) -> crate::utils::TraceResult<String> {
    let stats = data.syscalls.get_stats();
    let top_syscalls: Vec<String> = stats.iter().take(3).map(|s| s.name.clone()).collect();
    let detailed: Vec<SyscallRow> = stats
        .iter()
        .map(|s| SyscallRow {
            name: s.name.clone(),
            count: s.count,
            bytes: s.bytes,
        })
        .collect();

    let out = Out {
        process: ProcessOut {
            pid: data.process.pid,
            name: &data.process.name,
            status: &data.process.status,
            memory_mb: data.memory.rss_mb,
            cpu_percent: data.process.cpu_percent,
        },
        syscalls: SyscallsOut {
            total: data.syscalls.total_syscalls(),
            unique: data.syscalls.unique_syscalls(),
            top: top_syscalls,
            detailed,
        },
        network: NetworkOut {
            active_connections: data.network.active_connections,
            bytes_sent: data.network.bytes_sent,
            bytes_received: data.network.bytes_received,
        },
        timestamp: chrono::Local::now().to_rfc3339(),
    };

    toml::to_string_pretty(&out).map_err(|e| {
        crate::utils::TraceError::SerializationError(format!("TOML: {e}"))
    })
}
