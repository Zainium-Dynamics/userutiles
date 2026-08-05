use std::fs;

pub struct PowerInfo {
    pub ac_online: bool,
    pub battery_present: bool,
    pub battery_percent: Option<u8>,
    pub battery_status: Option<String>,
    pub cpu_freq_ghz: Option<f64>,
    pub governor: Option<String>,
}

fn read_file(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn collect() -> PowerInfo {
    // AC adapter
    let ac_online = read_file("/sys/class/power_supply/AC/online")
        .or_else(|| read_file("/sys/class/power_supply/AC0/online"))
        .map(|v| v.trim() == "1")
        .unwrap_or(true);

    // Battery
    let battery_present = std::path::Path::new("/sys/class/power_supply/BAT0").exists()
        || std::path::Path::new("/sys/class/power_supply/BAT1").exists();

    let battery_percent = read_file("/sys/class/power_supply/BAT0/capacity")
        .or_else(|| read_file("/sys/class/power_supply/BAT1/capacity"))
        .and_then(|v| v.parse::<u8>().ok());

    let battery_status = read_file("/sys/class/power_supply/BAT0/status")
        .or_else(|| read_file("/sys/class/power_supply/BAT1/status"));

    // CPU frequency (MHz)
    let cpu_freq_ghz = read_file("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
        .and_then(|v| v.parse::<f64>().ok())
        .map(|khz| khz / 1_000_000.0);

    let governor = read_file("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor");

    PowerInfo {
        ac_online,
        battery_present,
        battery_percent,
        battery_status,
        cpu_freq_ghz,
        governor,
    }
}
