use crate::utils::TraceResult;

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkStats {
    pub active_connections: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connections: Vec<ConnectionInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectionInfo {
    pub local_addr: String,
    pub remote_addr: String,
    pub state: String,
}

impl NetworkStats {
    pub fn from_pid(pid: u32) -> TraceResult<Self> {
        // Read from /proc/net/tcp for the process
        let mut active_connections = 0;
        let bytes_sent = 0u64;
        let bytes_received = 0u64;

        // This is a simplified approach - get system-wide stats
        if let Ok(tcp_entries) = procfs::net::tcp() {
            active_connections = tcp_entries
                .iter()
                .filter(|entry| entry.state == procfs::net::TcpState::Established)
                .count() as u32;
        }

        // For accurate per-process data, we'd need to parse /proc/[pid]/net/tcp
        // This is a simplified version
        if let Ok(content) = std::fs::read_to_string(format!("/proc/{}/net/tcp", pid)) {
            active_connections = content.lines().count().saturating_sub(1) as u32;
        }

        Ok(NetworkStats {
            active_connections,
            bytes_sent,
            bytes_received,
            connections: vec![],
        })
    }
}
