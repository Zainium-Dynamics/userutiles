use std::path::{Path, PathBuf};

use crate::error::{PrioError, Result};

// -- Cgroup Version Detection --------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgroupVersion {
    V2,
    V1,
    Unavailable,
}

/// Detect which cgroup version the running kernel exposes.
pub fn detect_version() -> CgroupVersion {
    if Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        CgroupVersion::V2
    } else if Path::new("/sys/fs/cgroup/memory").exists() {
        CgroupVersion::V1
    } else {
        CgroupVersion::Unavailable
    }
}

// -- CgroupManager ------------------------------------------------------------

/// Manages a single child cgroup scoped to one prio-launched process.
///
/// Creates `/sys/fs/cgroup/prio/<name>/` (v2) or
/// `/sys/fs/cgroup/memory/prio/<name>/` (v1) on construction, and removes
/// it on drop once the cgroup is empty.
pub struct CgroupManager {
    path: PathBuf,
    version: CgroupVersion,
}

impl CgroupManager {
    /// Create a new cgroup named `name` under the `prio` slice.
    ///
    /// Errors if cgroup support is unavailable or if the caller lacks the
    /// necessary root permissions.
    pub fn new(name: &str) -> Result<Self> {
        let version = detect_version();

        if version == CgroupVersion::Unavailable {
            return Err(PrioError::CgroupError(
                "cgroup filesystem not found at /sys/fs/cgroup".into(),
            ));
        }

        let root = match version {
            CgroupVersion::V2 => PathBuf::from("/sys/fs/cgroup/prio"),
            CgroupVersion::V1 => PathBuf::from("/sys/fs/cgroup/memory/prio"),
            CgroupVersion::Unavailable => unreachable!(),
        };

        // Ensure the prio parent slice exists.
        if !root.exists() {
            std::fs::create_dir_all(&root).map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    PrioError::PermissionDenied
                } else {
                    PrioError::CgroupError(format!("cannot create {}: {}", root.display(), e))
                }
            })?;
        }

        // For cgroup v2, enable the memory controller on the parent.
        if version == CgroupVersion::V2 {
            let ctrl = root.join("cgroup.subtree_control");
            if ctrl.exists() {
                let _ = std::fs::write(&ctrl, "+memory");
            }
        }

        let path = root.join(name);
        std::fs::create_dir_all(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                PrioError::PermissionDenied
            } else {
                PrioError::CgroupError(format!("cannot create cgroup {}: {}", path.display(), e))
            }
        })?;

        Ok(Self { path, version })
    }

    /// Assign `pid` to this cgroup so its resources are tracked.
    pub fn add_process(&self, pid: u32) -> Result<()> {
        let procs_file = match self.version {
            CgroupVersion::V2 => self.path.join("cgroup.procs"),
            CgroupVersion::V1 => self.path.join("tasks"),
            CgroupVersion::Unavailable => return Ok(()),
        };
        std::fs::write(&procs_file, pid.to_string()).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                PrioError::PermissionDenied
            } else {
                PrioError::CgroupError(format!("cannot add PID to cgroup: {}", e))
            }
        })
    }

    /// Set the maximum memory the cgroup may use, in bytes.
    /// Write `u64::MAX` to remove a previously set limit.
    pub fn set_memory_limit(&self, bytes: u64) -> Result<()> {
        let (file, value) = match self.version {
            CgroupVersion::V2 => {
                let value = if bytes == u64::MAX {
                    "max".to_string()
                } else {
                    bytes.to_string()
                };
                (self.path.join("memory.max"), value)
            }
            CgroupVersion::V1 => {
                let value = if bytes == u64::MAX {
                    "-1".to_string()
                } else {
                    bytes.to_string()
                };
                (self.path.join("memory.limit_in_bytes"), value)
            }
            CgroupVersion::Unavailable => return Ok(()),
        };

        std::fs::write(&file, value)
            .map_err(|e| PrioError::CgroupError(format!("cannot set memory limit: {}", e)))
    }

    /// Set the I/O weight for this cgroup (v2 only, 1–10000).
    #[allow(dead_code)]
    pub fn set_io_weight(&self, weight: u32) -> Result<()> {
        if self.version != CgroupVersion::V2 {
            return Ok(()); // No equivalent in v1 without blkio controller
        }
        let weight = weight.clamp(1, 10_000);
        let file = self.path.join("io.weight");
        if file.exists() {
            std::fs::write(&file, format!("default {}", weight))
                .map_err(|e| PrioError::CgroupError(format!("cannot set io.weight: {}", e)))?;
        }
        Ok(())
    }

    /// The filesystem path of this cgroup.
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for CgroupManager {
    /// Best-effort removal of the cgroup directory once the process exits.
    /// The kernel refuses to remove a cgroup that still has processes, so
    /// this will silently fail if the child is still alive — which is fine.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.path);
    }
}
