use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{PrioError, Result};

/// User-overridable defaults read from `~/.config/prio/config.toml`.
/// Every field has a sensible built-in default so the file is optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub defaults: DefaultsConfig,
    pub auto: AutoConfig,
    pub list: ListConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultsConfig {
    /// Nice level applied when no -n flag is given but a command is present.
    pub nice: i32,
    /// Nice level used by --boost.
    pub boost_nice: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoConfig {
    /// CPU temperature (°C) above which the process will be throttled.
    pub temp_threshold: f32,
    /// How many degrees below threshold before throttling is lifted.
    pub temp_hysteresis: f32,
    /// Seconds between temperature/load checks.
    pub check_interval_secs: u64,
    /// System load-average multiplier above which throttling kicks in.
    /// e.g. 1.5 means "throttle if load > 1.5 × CPU count".
    pub load_multiplier: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ListConfig {
    /// Number of processes shown by --list.
    pub max_processes: usize,
}

// -- Defaults ----------------------------------------------------------------

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            nice: 0,
            boost_nice: -12,
        }
    }
}

impl Default for AutoConfig {
    fn default() -> Self {
        Self {
            temp_threshold: 80.0,
            temp_hysteresis: 10.0,
            check_interval_secs: 5,
            load_multiplier: 1.5,
        }
    }
}

impl Default for ListConfig {
    fn default() -> Self {
        Self { max_processes: 15 }
    }
}

// -- Loading ------------------------------------------------------------------

impl Config {
    /// Load configuration from `~/.config/prio/config.toml`.
    /// Returns [`Config::default`] if the file does not exist.
    pub fn load() -> Self {
        Self::load_from_disk().unwrap_or_default()
    }

    fn load_from_disk() -> Result<Self> {
        let path = config_path()
            .ok_or_else(|| PrioError::SystemError("could not determine home directory".into()))?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(&path)?;
        toml::from_str::<Self>(&raw)
            .map_err(|e| PrioError::SystemError(format!("config parse error: {}", e)))
    }

    /// Write the current config (or the default) to disk so the user has a
    /// template to edit.
    #[allow(dead_code)]
    pub fn write_default() -> Result<()> {
        let path = config_path()
            .ok_or_else(|| PrioError::SystemError("could not determine home directory".into()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let default = Self::default();
        let raw = toml::to_string_pretty(&default)
            .map_err(|e| PrioError::SystemError(format!("config serialise error: {}", e)))?;

        std::fs::write(&path, raw)?;
        Ok(())
    }
}

fn config_path() -> Option<PathBuf> {
    dirs_next().map(|home| home.join(".config").join("prio").join("config.toml"))
}

/// Minimal home-directory lookup without pulling in `dirs` crate.
fn dirs_next() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).or_else(|| {
        // Fallback: parse /etc/passwd for the current UID.
        // SAFETY: `getuid(2)` takes no arguments, performs no pointer
        // dereferencing, and cannot fail — it is a pure syscall wrapper
        // that always succeeds, regardless of process state.
        let uid = unsafe { libc::getuid() };
        std::fs::read_to_string("/etc/passwd")
            .ok()
            .and_then(|passwd| {
                passwd.lines().find_map(|line| {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 6 && parts[2].parse::<u32>().ok() == Some(uid) {
                        return Some(PathBuf::from(parts[5]));
                    }
                    None
                })
            })
    })
}
