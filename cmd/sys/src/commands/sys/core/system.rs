use std::time::Duration;
use sysinfo::{CpuExt, System, SystemExt};

pub struct SystemInfo {
    pub hostname: String,
    pub os_name: String,
    pub kernel_version: String,
    pub uptime_secs: u64,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub cpu_usage: f32,
    pub total_memory_gib: f64,
    pub used_memory_gib: f64,
    pub memory_percent: f64,
    pub total_swap_gib: f64,
    pub used_swap_gib: f64,
    pub swap_percent: f64,
}

pub fn collect() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();
    // Second refresh for accurate CPU usage
    std::thread::sleep(Duration::from_millis(300));
    sys.refresh_cpu();

    let hostname = sys.host_name().unwrap_or_else(|| "zainium".into());
    let os_name = sys
        .long_os_version()
        .unwrap_or_else(|| "ZainiumOS 2026".into());
    let kernel_version = sys.kernel_version().unwrap_or_else(|| "7.0-pulse".into());
    let uptime_secs = sys.uptime();

    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".into());
    let cpu_cores = sys.physical_core_count().unwrap_or(1);
    let cpu_threads = sys.cpus().len();
    let cpu_usage = sys.global_cpu_info().cpu_usage();

    let total_mem = sys.total_memory() as f64;
    let used_mem = sys.used_memory() as f64;
    let gib = 1024.0 * 1024.0 * 1024.0;

    let total_swap = sys.total_swap() as f64;
    let used_swap = sys.used_swap() as f64;

    SystemInfo {
        hostname,
        os_name,
        kernel_version,
        uptime_secs,
        cpu_name,
        cpu_cores,
        cpu_threads,
        cpu_usage,
        total_memory_gib: total_mem / gib,
        used_memory_gib: used_mem / gib,
        memory_percent: if total_mem > 0.0 {
            (used_mem / total_mem) * 100.0
        } else {
            0.0
        },
        total_swap_gib: total_swap / gib,
        used_swap_gib: used_swap / gib,
        swap_percent: if total_swap > 0.0 {
            (used_swap / total_swap) * 100.0
        } else {
            0.0
        },
    }
}

pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{} days, {} hours", days, hours)
    } else if hours > 0 {
        format!("{} hours, {} mins", hours, mins)
    } else {
        format!("{} mins", mins)
    }
}
