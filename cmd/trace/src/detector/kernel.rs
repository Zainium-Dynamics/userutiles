use crate::utils::TraceResult;
use std::fs;

#[derive(Debug, Clone)]
pub struct KernelInfo {
    pub version: String,
    pub arch: String,
}

impl KernelInfo {
    pub fn detect() -> TraceResult<Self> {
        let version = Self::detect_version()?;
        let arch = Self::detect_arch()?;

        Ok(KernelInfo { version, arch })
    }

    fn detect_version() -> TraceResult<String> {
        if let Ok(content) = fs::read_to_string("/proc/version") {
            if let Some(version_str) = content.split_whitespace().nth(2) {
                return Ok(version_str.to_string());
            }
        }
        Ok("Unknown".to_string())
    }

    fn detect_arch() -> TraceResult<String> {
        #[cfg(target_arch = "x86_64")]
        return Ok("x86_64".to_string());

        #[cfg(target_arch = "aarch64")]
        return Ok("aarch64".to_string());

        #[cfg(target_arch = "arm")]
        return Ok("arm".to_string());

        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "arm"
        )))]
        return Ok("unknown".to_string());
    }
}
