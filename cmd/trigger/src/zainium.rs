//! ZainiumOS service awareness
//!
//! Checks `/etc/zainium/services/` and `/etc/zainium/enabled/` for
//! registered service definitions. Allows `trigger` to launch Zainium
//! services by name, adding a third resolution tier:
//!
//! ```text
//! Application (PATH) → Zainium Service → Script file → Error
//! ```

use log::debug;
use std::path::PathBuf;

/// Directories searched for Zainium service definitions, in priority order
const SERVICE_DIRS: &[&str] = &["/etc/zainium/services", "/etc/zainium/enabled"];

/// A discovered Zainium service
#[derive(Debug, Clone)]
pub struct ZainiumService {
    /// Service name (without `.toml` extension)
    pub name: String,
    /// Path to the service TOML definition
    pub toml_path: PathBuf,
}

/// Find a service by name in the Zainium service directories.
/// Returns `None` if the name does not correspond to any known service.
pub fn find_service(name: &str) -> Option<ZainiumService> {
    for dir in SERVICE_DIRS {
        // Check <name>.toml (services/ uses this convention)
        let with_ext = PathBuf::from(dir).join(format!("{}.toml", name));
        if with_ext.exists() {
            debug!("zainium: found service '{}' at {:?}", name, with_ext);
            return Some(ZainiumService {
                name: name.to_string(),
                toml_path: with_ext,
            });
        }

        // Check bare <name> (enabled/ uses symlinks without extension)
        let bare = PathBuf::from(dir).join(name);
        if bare.exists() {
            debug!("zainium: found service symlink '{}' at {:?}", name, bare);
            return Some(ZainiumService {
                name: name.to_string(),
                toml_path: bare,
            });
        }
    }
    None
}

/// List all services across all Zainium service directories.
/// Deduplicates by name (services/ takes priority over enabled/).
pub fn list_services() -> Vec<ZainiumService> {
    let mut services: Vec<ZainiumService> = Vec::new();

    for dir in SERVICE_DIRS {
        let dir_path = PathBuf::from(dir);
        if !dir_path.is_dir() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Extract display name without extension
                let name = path
                    .file_stem()
                    .or_else(|| path.file_name())
                    .and_then(|s| s.to_str())
                    .map(str::to_string);

                if let Some(name) = name {
                    if !services.iter().any(|s| s.name == name) {
                        services.push(ZainiumService {
                            name,
                            toml_path: path,
                        });
                    }
                }
            }
        }
    }

    services.sort_by(|a, b| a.name.cmp(&b.name));
    services
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_nonexistent_service() {
        // Should never find a service that doesn't exist
        assert!(find_service("zzz_no_such_zainium_service_xyz").is_none());
    }

    #[test]
    fn test_list_services_does_not_panic() {
        // On a non-Zainium system this returns empty; on ZainiumOS it returns services
        let services = list_services();
        // Just ensure it doesn't panic — content is system-dependent
        let _ = services;
    }
}
