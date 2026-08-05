use std::sync::{
    atomic::{AtomicBool, AtomicI32, Ordering},
    Arc,
};
use std::time::Duration;

use crate::config::AutoConfig;

// -- System Sensors ------------------------------------------------------------

/// Read the first readable CPU thermal zone temperature in degrees Celsius.
/// Returns `None` if no thermal zone could be read (e.g. inside a container).
fn read_cpu_temp() -> Option<f32> {
    // Prefer coretemp or package temperature sensors, fall back to zone0.
    let candidates = [
        "/sys/class/thermal/thermal_zone0/temp",
        "/sys/class/thermal/thermal_zone1/temp",
        "/sys/class/thermal/thermal_zone2/temp",
        "/sys/class/hwmon/hwmon0/temp1_input",
        "/sys/class/hwmon/hwmon1/temp1_input",
    ];

    for path in candidates {
        if let Ok(raw) = std::fs::read_to_string(path) {
            if let Ok(millideg) = raw.trim().parse::<i64>() {
                return Some(millideg as f32 / 1000.0);
            }
        }
    }
    None
}

/// Read the 1-minute load average from `/proc/loadavg`.
fn read_load_avg() -> Option<f32> {
    let raw = std::fs::read_to_string("/proc/loadavg").ok()?;
    raw.split_ascii_whitespace().next()?.parse::<f32>().ok()
}

/// Number of logical CPUs reported by `/proc/cpuinfo`.
fn cpu_count() -> f32 {
    std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count()
        .max(1) as f32
}

// -- AutoMonitor ---------------------------------------------------------------

/// Background monitor that dynamically adjusts a process's nice level based on
/// CPU temperature and system load average.
///
/// Throttling policy:
/// - If `temp > threshold`: increase nice by +2 (throttle)
/// - If `temp < threshold − hysteresis`: decrease nice by −1 (recover),
///   but never lower than the user's requested `base_nice`.
/// - If `load > cpu_count × multiplier`: increase nice by +1 (throttle)
///
/// The monitor thread is a daemon thread; it will be killed automatically when
/// the main process exits.
pub struct AutoMonitor {
    stop: Arc<AtomicBool>,
    #[allow(dead_code)]
    current_nice: Arc<AtomicI32>,
}

impl AutoMonitor {
    /// Spawn the background monitor for `pid`, starting at `base_nice`.
    pub fn start(pid: u32, base_nice: i32, cfg: &AutoConfig) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let current_nice = Arc::new(AtomicI32::new(base_nice));

        let stop_clone = Arc::clone(&stop);
        let nice_clone = Arc::clone(&current_nice);

        let temp_threshold = cfg.temp_threshold;
        let temp_hysteresis = cfg.temp_hysteresis;
        let load_multiplier = cfg.load_multiplier;
        let interval = Duration::from_secs(cfg.check_interval_secs);
        let cpus = cpu_count();

        std::thread::Builder::new()
            .name("prio-monitor".into())
            .spawn(move || {
                while !stop_clone.load(Ordering::Relaxed) {
                    std::thread::sleep(interval);

                    let cur = nice_clone.load(Ordering::Relaxed);
                    let mut new_nice = cur;

                    // -- Temperature check ----------------------------------
                    if let Some(temp) = read_cpu_temp() {
                        if temp > temp_threshold {
                            new_nice = (cur + 2).min(19);
                        } else if temp < temp_threshold - temp_hysteresis {
                            new_nice = (cur - 1).max(base_nice);
                        }
                    }

                    // -- Load-average check --------------------------------
                    if let Some(load) = read_load_avg() {
                        if load > cpus * load_multiplier {
                            new_nice = (new_nice + 1).min(19);
                        }
                    }

                    // Apply if changed
                    if new_nice != cur {
                        // SAFETY: `setpriority(2)` only takes plain integers
                        // (no pointers/buffers to validate); `pid` is a
                        // `u32` widened to `id_t` (also `u32` on Linux), and
                        // `new_nice` is clamped to `[base_nice, 19]` above,
                        // well within the kernel's accepted `[-20, 19]`
                        // range. If `pid` no longer exists the call simply
                        // returns -1/ESRCH, which is handled by the `rc ==
                        // 0` check below — no UB in either case.
                        let rc = unsafe {
                            libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, new_nice)
                        };
                        if rc == 0 {
                            nice_clone.store(new_nice, Ordering::Relaxed);
                        }
                    }
                }
            })
            .expect("failed to spawn prio-monitor thread");

        Self { stop, current_nice }
    }

    /// Signal the background thread to stop at the next check interval.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Return the current dynamically-adjusted nice value.
    #[allow(dead_code)]
    pub fn current_nice(&self) -> i32 {
        self.current_nice.load(Ordering::Relaxed)
    }
}

impl Drop for AutoMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

// -- Public sensor accessors (used for verbose display) -----------------------

#[allow(dead_code)]
pub fn current_temp() -> Option<f32> {
    read_cpu_temp()
}
#[allow(dead_code)]
pub fn current_load() -> Option<f32> {
    read_load_avg()
}
#[allow(dead_code)]
pub fn logical_cpus() -> u32 {
    cpu_count() as u32
}
