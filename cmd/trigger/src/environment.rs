//! Environment variable wrapper for real-time resolution
//! Handles XDG Base Directory Specification and safe environment access

use std::env;
use std::path::PathBuf;

/// Safe environment variable access wrapper
pub struct Environment;

impl Environment {
    /// Get configuration directories respecting XDG Base Directory spec (Linux)
    /// or native Redox paths
    pub fn config_dir() -> Result<PathBuf, String> {
        // Try XDG_CONFIG_HOME first
        if let Ok(xdg_config) = env::var("XDG_CONFIG_HOME") {
            if !xdg_config.is_empty() {
                return Ok(PathBuf::from(xdg_config));
            }
        }

        // Fall back to dirs crate
        if let Some(config_home) = dirs::config_dir() {
            return Ok(config_home);
        }

        // Fall back to HOME/.config
        if let Ok(home) = env::var("HOME") {
            return Ok(PathBuf::from(home).join(".config"));
        }

        Err("Cannot determine config directory".to_string())
    }

    /// Get cache directories respecting XDG spec
    #[allow(dead_code)]
    pub fn cache_dir() -> Result<PathBuf, String> {
        // Try XDG_CACHE_HOME first
        if let Ok(xdg_cache) = env::var("XDG_CACHE_HOME") {
            if !xdg_cache.is_empty() {
                return Ok(PathBuf::from(xdg_cache));
            }
        }

        // Fall back to dirs crate
        if let Some(cache_home) = dirs::cache_dir() {
            return Ok(cache_home);
        }

        // Fall back to HOME/.cache
        if let Ok(home) = env::var("HOME") {
            return Ok(PathBuf::from(home).join(".cache"));
        }

        Err("Cannot determine cache directory".to_string())
    }

    /// Get data directories respecting XDG spec
    #[allow(dead_code)]
    pub fn data_dir() -> Result<PathBuf, String> {
        // Try XDG_DATA_HOME first
        if let Ok(xdg_data) = env::var("XDG_DATA_HOME") {
            if !xdg_data.is_empty() {
                return Ok(PathBuf::from(xdg_data));
            }
        }

        // Fall back to dirs crate
        if let Some(data_home) = dirs::data_dir() {
            return Ok(data_home);
        }

        // Fall back to HOME/.local/share
        if let Ok(home) = env::var("HOME") {
            return Ok(PathBuf::from(home).join(".local/share"));
        }

        Err("Cannot determine data directory".to_string())
    }

    /// Get all PATH directories as a vector
    pub fn path_dirs() -> Vec<PathBuf> {
        env::var("PATH")
            .unwrap_or_else(|_| "/overlayer/syshub/bin:/overlayer/syshub/sbin".to_string())
            .split(':')
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    /// Get shell from environment, with fallback
    #[allow(dead_code)]
    pub fn shell() -> String {
        env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }

    /// Get username from environment
    pub fn username() -> String {
        env::var("USER")
            .or_else(|_| env::var("LOGNAME"))
            .unwrap_or_else(|_| "unknown".to_string())
    }

    /// Get home directory safely with fallback
    #[allow(dead_code)]
    pub fn home() -> Result<PathBuf, String> {
        // Try HOME environment variable first
        if let Ok(home_path) = env::var("HOME") {
            if !home_path.is_empty() {
                return Ok(PathBuf::from(home_path));
            }
        }

        // Fall back to dirs crate
        if let Some(home_path) = dirs::home_dir() {
            return Ok(home_path);
        }

        Err("Cannot determine home directory".to_string())
    }

    /// Get a specific environment variable safely
    #[allow(dead_code)]
    pub fn get(key: &str) -> Option<String> {
        env::var(key).ok().filter(|v| !v.is_empty())
    }

    /// Check if running as root (UID 0)
    #[allow(dead_code)]
    pub fn is_root() -> bool {
        #[cfg(unix)]
        // SAFETY: `libc::geteuid` takes no arguments, reads no
        // caller-supplied pointers, and cannot fail — it is always safe to
        // call regardless of process state.
        unsafe {
            libc::geteuid() == 0
        }
        #[cfg(not(unix))]
        false
    }

    /// Get runtime directory (XDG_RUNTIME_DIR) if available
    #[allow(dead_code)]
    pub fn runtime_dir() -> Option<PathBuf> {
        env::var("XDG_RUNTIME_DIR")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    /// Resolve a path with environment variable expansion
    /// Replaces $HOME, $USER, $SHELL at the beginning
    #[allow(dead_code)]
    pub fn expand_path(path: &str) -> PathBuf {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Ok(home) = Self::home() {
                return home.join(stripped);
            }
        }

        let expanded = path
            .replace(
                "$HOME",
                &Self::home()
                    .map(|h| h.display().to_string())
                    .unwrap_or_default(),
            )
            .replace("$USER", &Self::username())
            .replace("$SHELL", &Self::shell());

        PathBuf::from(expanded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir() {
        let result = Environment::config_dir();
        assert!(result.is_ok(), "Config dir should be determinable");
    }

    #[test]
    fn test_cache_dir() {
        let result = Environment::cache_dir();
        assert!(result.is_ok(), "Cache dir should be determinable");
    }

    #[test]
    fn test_data_dir() {
        let result = Environment::data_dir();
        assert!(result.is_ok(), "Data dir should be determinable");
    }

    #[test]
    fn test_path_dirs() {
        let dirs = Environment::path_dirs();
        assert!(
            !dirs.is_empty(),
            "PATH should contain at least one directory"
        );
    }

    #[test]
    fn test_shell() {
        let shell = Environment::shell();
        assert!(!shell.is_empty(), "Shell should be determinable");
    }

    #[test]
    fn test_username() {
        let user = Environment::username();
        assert!(!user.is_empty(), "Username should be determinable");
    }

    #[test]
    fn test_home() {
        let result = Environment::home();
        assert!(result.is_ok(), "Home directory should be determinable");
    }

    #[test]
    fn test_path_expansion() {
        let expanded = Environment::expand_path("~/test");
        let home_str = Environment::home()
            .map(|h| h.display().to_string())
            .unwrap_or_default();
        assert!(expanded.display().to_string().contains(&home_str));
    }
}
