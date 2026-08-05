use sysinfo::{System, SystemExt};

#[allow(dead_code)]
pub struct MemoryInfo {
    pub total_gib: f64,
    pub used_gib: f64,
    pub available_gib: f64,
    pub percent: f64,
    pub total_swap_gib: f64,
    pub used_swap_gib: f64,
    pub swap_percent: f64,
}

pub fn collect() -> MemoryInfo {
    let mut sys = System::new_all();
    sys.refresh_memory();
    let gib = 1024.0 * 1024.0 * 1024.0;
    let total = sys.total_memory() as f64;
    let used = sys.used_memory() as f64;
    let avail = sys.available_memory() as f64;
    let ts = sys.total_swap() as f64;
    let us = sys.used_swap() as f64;
    MemoryInfo {
        total_gib: total / gib,
        used_gib: used / gib,
        available_gib: avail / gib,
        percent: if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        },
        total_swap_gib: ts / gib,
        used_swap_gib: us / gib,
        swap_percent: if ts > 0.0 { (us / ts) * 100.0 } else { 0.0 },
    }
}
