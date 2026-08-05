//! Runtime discovery of applications and handlers
//! Dynamically scans the system for installed applications and available interpreters

use log::debug;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::platform::Platform;

/// Discovers applications and handlers at runtime
pub struct Discoverer {
    platform: &'static Platform,
}

impl Discoverer {
    /// Create a new discoverer using the cached platform singleton
    pub fn new(platform: &'static Platform) -> Self {
        Discoverer { platform }
    }

    /// Discover all available applications on the system
    /// Returns a HashMap of application names to their paths
    pub fn discover_apps(&self) -> HashMap<String, String> {
        let mut apps = HashMap::new();

        // Scan all application search paths
        for path in &self.platform.app_search_paths {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            // Check if file is executable
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let mode = metadata.permissions().mode();
                                if mode & 0o111 != 0 {
                                    if let Some(name) = entry.file_name().to_str() {
                                        let app_path = entry.path();
                                        debug!("Discovered app: {} at {:?}", name, app_path);
                                        apps.insert(
                                            name.to_string(),
                                            app_path.display().to_string(),
                                        );
                                    }
                                }
                            }
                            #[cfg(not(unix))]
                            {
                                if let Some(name) = entry.file_name().to_str() {
                                    let app_path = entry.path();
                                    debug!("Discovered app: {} at {:?}", name, app_path);
                                    apps.insert(name.to_string(), app_path.display().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        debug!("Total apps discovered: {}", apps.len());
        apps
    }

    /// Discover available script handlers by probing the system
    /// Returns a HashMap of file extensions to handler commands
    pub fn discover_handlers(&self) -> HashMap<String, String> {
        let mut handlers = HashMap::new();
        let discovered_apps = self.discover_apps();

        // List of handler candidates - (extension, handler_binary)
        let handler_candidates = vec![
            ("rs", "rustc"),
            ("py", "python3"),
            ("py", "python"),
            ("sh", "bash"),
            ("sh", "sh"),
            ("js", "node"),
            ("ts", "ts-node"),
            ("java", "java"),
            ("go", "go"),
            ("cpp", "g++"),
            ("cpp", "clang++"),
            ("c", "gcc"),
            ("c", "clang"),
            ("rb", "ruby"),
            ("lua", "lua"),
            ("pl", "perl"),
            ("swift", "swift"),
            ("kt", "kotlinc"),
            ("scala", "scala"),
            ("r", "Rscript"),
            ("php", "php"),
            ("clj", "clojure"),
            ("hs", "runhaskell"),
            ("ex", "elixir"),
            ("m", "swift"),
            ("groovy", "groovy"),
            ("erl", "erl"),
            ("jl", "julia"),
            ("ml", "ocamlc"),
        ];

        // Check which handlers are available on the system
        for (ext, handler) in handler_candidates {
            if discovered_apps.contains_key(handler) {
                debug!("Discovered handler: .{} -> {}", ext, handler);
                handlers.insert(ext.to_string(), handler.to_string());
            }
        }

        debug!("Total handlers discovered: {}", handlers.len());
        handlers
    }

    /// Discover desktop application files (Linux only, Redox has no .desktop files)
    /// Returns a HashMap of app names to .desktop file paths
    #[allow(dead_code)]
    pub fn discover_desktop_apps(&self) -> HashMap<String, PathBuf> {
        let mut desktop_apps = HashMap::new();

        for desktop_path in &self.platform.desktop_paths {
            if !desktop_path.exists() {
                continue;
            }

            if let Ok(entries) = fs::read_dir(desktop_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                        if let Some(filename) = path.file_stem() {
                            if let Some(name) = filename.to_str() {
                                debug!("Discovered desktop app: {} at {:?}", name, path);
                                desktop_apps.insert(name.to_string(), path);
                            }
                        }
                    }
                }
            }
        }

        debug!("Total desktop apps discovered: {}", desktop_apps.len());
        desktop_apps
    }

    /// Get the list of all discovered application names (without paths)
    #[allow(dead_code)]
    pub fn get_app_names(&self) -> Vec<String> {
        self.discover_apps().keys().cloned().collect()
    }

    /// Get the list of all discovered file extensions (handlers)
    #[allow(dead_code)]
    pub fn get_handler_extensions(&self) -> Vec<String> {
        self.discover_handlers().keys().cloned().collect()
    }
}

impl Default for Discoverer {
    fn default() -> Self {
        Self::new(Platform::get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discoverer_creation() {
        let _discoverer = Discoverer::default();
    }

    #[test]
    fn test_discover_apps_not_empty() {
        let discoverer = Discoverer::default();
        let apps = discoverer.discover_apps();
        // Should at least find /bin/sh, /bin/bash, or similar
        assert!(!apps.is_empty(), "No applications discovered");
    }

    #[test]
    fn test_discover_handlers_not_empty() {
        let discoverer = Discoverer::default();
        let handlers = discoverer.discover_handlers();
        // Should at least find sh handler
        assert!(!handlers.is_empty(), "No handlers discovered");
    }

    #[test]
    fn test_app_names_list() {
        let discoverer = Discoverer::default();
        let names = discoverer.get_app_names();
        assert!(!names.is_empty());
    }

    #[test]
    fn test_handler_extensions_list() {
        let discoverer = Discoverer::default();
        let exts = discoverer.get_handler_extensions();
        assert!(!exts.is_empty());
    }
}
