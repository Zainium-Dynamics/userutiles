//! Platform detection and OS-specific configuration

use once_cell::sync::OnceCell;
use std::fs;
use std::path::PathBuf;

/// Cached singleton: computed once per process
static PLATFORM: OnceCell<Platform> = OnceCell::new();

/// Detected operating system type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OsType {
    /// Linux operating system
    Linux,
    /// Redox operating system
    Redox,
    /// Unknown/unsupported OS
    Unknown,
}

/// Platform configuration for current system
#[allow(dead_code)]
pub struct Platform {
    /// Detected operating system
    pub os: OsType,
    /// Whether running with elevated privileges (UID 0)
    pub is_root: bool,
    /// Paths to search for applications
    pub app_search_paths: Vec<PathBuf>,
    /// Configuration directories
    pub config_dirs: Vec<PathBuf>,
    /// Desktop application file paths (Linux only)
    pub desktop_paths: Vec<PathBuf>,
    /// Paths to search for script handlers/interpreters
    pub handler_search_paths: Vec<PathBuf>,
}

impl Platform {
    /// Get the cached platform singleton: detected once per process
    pub fn get() -> &'static Platform {
        PLATFORM.get_or_init(Self::detect)
    }

    /// Detect the current platform and return platform configuration
    pub fn detect() -> Self {
        let os = Self::detect_os();
        let is_root = Self::is_privileged();

        let (app_search_paths, config_dirs, desktop_paths, handler_search_paths) =
            Self::get_paths_for_os(os);

        Platform {
            os,
            is_root,
            app_search_paths,
            config_dirs,
            desktop_paths,
            handler_search_paths,
        }
    }

    /// Detect which operating system we're running on
    fn detect_os() -> OsType {
        if cfg!(target_os = "linux") {
            if Self::is_running_on_redox() {
                return OsType::Redox;
            }
            OsType::Linux
        } else if cfg!(target_os = "redox") {
            OsType::Redox
        } else {
            OsType::Unknown
        }
    }

    /// Check if running on Redox OS (when compiled for Linux)
    /// Only meaningful on a Linux build: Redox builds never call this path
    #[cfg(target_os = "linux")]
    fn is_running_on_redox() -> bool {
        if std::env::var("REDOX_OS").is_ok() {
            return true;
        }

        if let Ok(version) = fs::read_to_string("/proc/version") {
            if version.contains("Redox") {
                return true;
            }
        }

        std::path::Path::new("/etc/redox").exists() || std::path::Path::new("/scheme").exists()
    }

    /// Stub for non-Linux targets: Redox is always natively detected via cfg!(target_os)
    #[cfg(not(target_os = "linux"))]
    #[inline(always)]
    fn is_running_on_redox() -> bool {
        false
    }

    /// Check if running with root/elevated privileges (UID 0)
    fn is_privileged() -> bool {
        #[cfg(unix)]
        // SAFETY: `libc::geteuid` takes no arguments, reads no
        unsafe {
            libc::geteuid() == 0
        }
        #[cfg(not(unix))]
        false
    }

    /// Get path configuration for the detected OS
    fn get_paths_for_os(os: OsType) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
        match os {
            OsType::Linux => Self::linux_paths(),
            OsType::Redox => Self::redox_paths(),
            OsType::Unknown => Self::default_paths(),
        }
    }

    /// Get Linux-specific paths (FHS + XDG compliance)
    fn linux_paths() -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
        let app_paths = vec![
            PathBuf::from("/overlayer/syshub/bin"),
            PathBuf::from("/overlayer/syshub/sbin"),
            PathBuf::from("/opt/overlayer/syshub/bin"),
            PathBuf::from("/snap/overlayer/syshub/bin"),
            PathBuf::from("/usr/lib/flatpak/exports/overlayer/syshub/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/bin"),
        ];

        let config_dirs = vec![
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    dirs::config_dir().unwrap_or_else(|| {
                        std::env::var("HOME")
                            .map(|h| PathBuf::from(h).join(".config"))
                            .unwrap_or_else(|_| PathBuf::from("/etc/trigger"))
                    })
                }),
            PathBuf::from("/etc/trigger"),
        ];

        let mut desktop_dirs = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
        ];

        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            desktop_dirs.push(PathBuf::from(data_home).join("applications"));
        } else if let Some(home) = dirs::home_dir() {
            desktop_dirs.push(home.join(".local/share/applications"));
        }

        if std::path::Path::new("/snap").exists() {
            if let Ok(snaps) = fs::read_dir("/snap") {
                for entry in snaps.flatten() {
                    let candidate = entry.path().join("current/meta/gui");
                    if candidate.is_dir() {
                        desktop_dirs.push(candidate);
                    }
                }
            }
        }

        let handler_paths = vec![
            PathBuf::from("/overlayer/syshub/bin"),
            PathBuf::from("/overlayer/syshub/sbin"),
            PathBuf::from("/opt/overlayer/syshub/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/bin"),
        ];

        (app_paths, config_dirs, desktop_dirs, handler_paths)
    }

    /// Get Redox OS-specific paths
    fn redox_paths() -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
        let app_paths = vec![
            PathBuf::from("/overlayer/syshub/bin"),
            PathBuf::from("/overlayer/syshub/bin"),
            PathBuf::from("/opt/overlayer/syshub/bin"),
        ];

        let config_dirs = vec![
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    dirs::home_dir()
                        .map(|h| h.join(".config"))
                        .unwrap_or_else(|| PathBuf::from("/etc/trigger"))
                }),
            PathBuf::from("/etc/trigger"),
        ];

        let desktop_dirs = vec![];

        let handler_paths = vec![
            PathBuf::from("/overlayer/syshub/bin"),
            PathBuf::from("/overlayer/syshub/bin"),
            PathBuf::from("/opt/overlayer/syshub/bin"),
        ];

        (app_paths, config_dirs, desktop_dirs, handler_paths)
    }

    /// Get default fallback paths
    fn default_paths() -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
        (
            vec![
                PathBuf::from("/overlayer/syshub/bin"),
                PathBuf::from("/overlayer/syshub/sbin"),
            ],
            vec![PathBuf::from("/etc/trigger")],
            vec![],
            vec![
                PathBuf::from("/overlayer/syshub/bin"),
                PathBuf::from("/overlayer/syshub/sbin"),
            ],
        )
    }
}

impl Default for Platform {
    fn default() -> Self {
        Self::detect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::detect();
        assert_ne!(platform.os, OsType::Unknown);
    }

    #[test]
    fn test_privilege_detection() {
        let platform = Platform::detect();
        let _is_root = platform.is_root;
    }

    #[test]
    fn test_app_search_paths_not_empty() {
        let platform = Platform::detect();
        assert!(!platform.app_search_paths.is_empty());
    }

    #[test]
    fn test_config_dirs_not_empty() {
        let platform = Platform::detect();
        assert!(!platform.config_dirs.is_empty());
    }
}
