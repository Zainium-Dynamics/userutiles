use crate::utils::TraceResult;
use std::fs;

#[derive(Debug, Clone)]
pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub distro: String,
}

impl OsInfo {
    pub fn detect() -> TraceResult<Self> {
        let distro = Self::detect_distro()?;
        let version = Self::detect_version()?;

        Ok(OsInfo {
            name: "Zainium OS".to_string(),
            version,
            distro,
        })
    }

    fn detect_distro() -> TraceResult<String> {
        // Try /etc/os-release
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("NAME=") {
                    return Ok(line
                        .strip_prefix("NAME=")
                        .unwrap_or("Unknown")
                        .trim_matches('"')
                        .to_string());
                }
            }
        }

        // Fallback
        Ok("Zainium OS".to_string())
    }

    fn detect_version() -> TraceResult<String> {
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("VERSION_ID=") {
                    return Ok(line
                        .strip_prefix("VERSION_ID=")
                        .unwrap_or("Unknown")
                        .trim_matches('"')
                        .to_string());
                }
            }
        }

        Ok("Unknown".to_string())
    }
}
