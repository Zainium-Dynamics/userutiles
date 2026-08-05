use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// Default mountpoint base directory
    pub mount_base: PathBuf,
    /// Temperature warning threshold in Celsius
    pub temp_warn_celsius: u32,
    /// Temperature critical threshold in Celsius
    pub temp_crit_celsius: u32,
    /// Snapshot directory relative to subvolume
    pub snapshot_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mount_base: PathBuf::from("/mnt"),
            temp_warn_celsius: 45,
            temp_crit_celsius: 60,
            snapshot_dir: ".snapshots".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = dirs_next::config_dir()
            .unwrap_or_else(|| PathBuf::from("/etc"))
            .join("drive")
            .join("config.toml");

        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str(&content) {
                    return cfg;
                }
            }
        }
        Self::default()
    }
}
