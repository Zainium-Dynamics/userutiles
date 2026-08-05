use std::time::Duration;
use sysinfo::{CpuExt, System, SystemExt};

#[allow(dead_code)]
pub struct CpuInfo {
    pub name: String,
    pub cores: usize,
    pub threads: usize,
    pub usage: f32,
    pub per_core: Vec<f32>,
    pub frequency_mhz: u64,
}

pub fn collect() -> CpuInfo {
    let mut sys = System::new_all();
    sys.refresh_cpu();
    std::thread::sleep(Duration::from_millis(400));
    sys.refresh_cpu();

    let name = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_else(|| "Unknown".into());
    let cores = sys.physical_core_count().unwrap_or(1);
    let threads = sys.cpus().len();
    let usage = sys.global_cpu_info().cpu_usage();
    let per_core: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
    let frequency_mhz = sys.cpus().first().map(|c| c.frequency()).unwrap_or(0);

    CpuInfo {
        name,
        cores,
        threads,
        usage,
        per_core,
        frequency_mhz,
    }
}
