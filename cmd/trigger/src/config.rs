use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::discovery::Discoverer;
use crate::error::Result;
use crate::platform::Platform;
use log::debug;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub name: String,
    pub description: String,
    /// Resolved binary path (set when auto-discovered at runtime)
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileHandlerConfig {
    pub extension: String,
    pub handler: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub known_apps: HashMap<String, AppConfig>,
    pub file_handlers: HashMap<String, FileHandlerConfig>,
    #[serde(default = "default_levenshtein_threshold")]
    pub levenshtein_threshold: usize,
    #[serde(default)]
    pub ui_strings: HashMap<String, String>,
    #[serde(default)]
    pub feature_flags: HashMap<String, bool>,
}

fn default_levenshtein_threshold() -> usize {
    3
}

impl Config {
    /// Resolve the expected config file path for the current platform.
    pub fn config_path() -> Result<PathBuf> {
        let platform = Platform::get();
        platform
            .config_dirs
            .first()
            .map(|dir| dir.join("config.toml"))
            .ok_or_else(|| crate::error::TriggerError::ConfigError {
                reason: "No configuration directory available for current platform".to_string(),
            })
    }

    /// Return default configuration with dynamic discovery
    pub fn load() -> Result<Self> {
        Ok(Self::default())
    }

    /// Dynamically discover apps and handlers at runtime
    /// This removes all hardcoding - everything is discovered from the system
    fn discover_dynamic() -> Self {
        let platform = Platform::get();
        let discoverer = Discoverer::new(platform);

        debug!("Dynamically discovering applications and handlers...");

        let discovered_apps = discoverer.discover_apps();
        let discovered_handlers = discoverer.discover_handlers();

        let mut known_apps = HashMap::new();
        for (name, path) in discovered_apps {
            known_apps.insert(
                name.clone(),
                AppConfig {
                    name,
                    description: String::new(),
                    path: Some(path),
                },
            );
        }

        let mut file_handlers = HashMap::new();
        for (ext, handler) in discovered_handlers {
            file_handlers.insert(
                ext.clone(),
                FileHandlerConfig {
                    extension: ext,
                    handler,
                    description: String::new(),
                },
            );
        }

        Config {
            known_apps,
            file_handlers,
            levenshtein_threshold: 3,
            ui_strings: HashMap::new(),
            feature_flags: HashMap::new(),
        }
    }

    /// Get a specific application configuration
    pub fn get_app(&self, app_name: &str) -> Option<&AppConfig> {
        self.known_apps.get(app_name)
    }

    /// Get a specific file handler configuration
    pub fn get_handler(&self, extension: &str) -> Option<&FileHandlerConfig> {
        self.file_handlers.get(extension)
    }

    /// Get a UI string from configuration
    pub fn get_string(&self, _key: &str) -> Option<String> {
        None // No config file, so no custom strings
    }

    /// Get a feature flag from configuration
    #[allow(dead_code)]
    pub fn get_feature(&self, _key: &str) -> bool {
        false // No config file, so no features
    }

    /// Get list of all known applications
    pub fn get_apps_list(&self) -> Vec<String> {
        self.known_apps.keys().cloned().collect()
    }

    /// Get list of all known file handlers
    pub fn get_handlers_list(&self) -> Vec<String> {
        self.file_handlers.keys().cloned().collect()
    }

    /// Check if a specific feature is enabled
    #[allow(dead_code)]
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        self.get_feature(feature)
    }
}

impl Default for Config {
    fn default() -> Self {
        // Default implementation uses dynamic discovery
        // No hardcoded values!
        Self::discover_dynamic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        // Should discover at least some apps
        assert!(
            !config.known_apps.is_empty(),
            "Should discover at least some apps"
        );
        // Should discover at least some handlers
        assert!(
            !config.file_handlers.is_empty(),
            "Should discover at least some handlers"
        );
    }

    #[test]
    fn test_dynamic_discovery() {
        let config = Config::discover_dynamic();
        // Should discover applications
        assert!(!config.known_apps.is_empty());
        // Should discover handlers
        assert!(!config.file_handlers.is_empty());
    }

    #[test]
    fn test_config_path() {
        let result = Config::config_path();
        assert!(result.is_ok(), "Config path should be determinable");
    }

    #[test]
    fn test_levenshtein_threshold_default() {
        let config = Config::default();
        assert_eq!(config.levenshtein_threshold, 3);
    }

    #[test]
    fn test_get_apps_list() {
        let config = Config::default();
        let apps = config.get_apps_list();
        assert!(!apps.is_empty());
    }

    #[test]
    fn test_get_handlers_list() {
        let config = Config::default();
        let handlers = config.get_handlers_list();
        assert!(!handlers.is_empty());
    }
}
