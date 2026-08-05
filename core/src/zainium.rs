//! Zainium OS filesystem layout — overlayer, not FHS `/usr`.
//!
//! Reference (read-only): zairoot `overlayer/syshub/etc/profile`
//!
//! ```text
//! PATH="/overlayer/syshub/bin:/overlayer/syshub/sbin"
//! LD_LIBRARY_PATH="/overlayer/syshub/lib"
//! ```
//!
//! Do **not** assume `/usr/bin`, `/bin`, or `/usr`.

/// Primary executable directory.
pub const SYSHUB_BIN: &str = "/overlayer/syshub/bin";

/// Privileged / system executable directory.
pub const SYSHUB_SBIN: &str = "/overlayer/syshub/sbin";

/// Shared libraries.
pub const SYSHUB_LIB: &str = "/overlayer/syshub/lib";

/// Shared data.
pub const SYSHUB_SHARE: &str = "/overlayer/syshub/share";

/// Drivers.
pub const SYSHUB_DRIVERS: &str = "/overlayer/syshub/drivers";

/// Engine / services.
pub const SYSHUB_ENGINE: &str = "/overlayer/syshub/engine";

/// Default `PATH` when the environment variable is unset or empty.
/// Matches ZainiumOS profile — never falls back to `/usr/bin`.
pub const DEFAULT_PATH: &str = "/overlayer/syshub/bin:/overlayer/syshub/sbin";

/// Default `LD_LIBRARY_PATH` fallback.
pub const DEFAULT_LD_LIBRARY_PATH: &str = "/overlayer/syshub/lib";

/// Default secure_path for elevated tools (elevate.toml).
pub const DEFAULT_SECURE_PATH: &str = "/overlayer/syshub/sbin:/overlayer/syshub/bin:/overlayer/syshub/lib:/overlayer/syshub/share:/overlayer/syshub/drivers:/overlayer/syshub/engine";

/// Install prefix for packaging user_utils (relative to rootfs).
/// Overridable at runtime via `ZEX_PREFIX` (no other layout hardcoding needed).
pub const INSTALL_PREFIX: &str = "/overlayer/syshub";

/// Resolve install prefix: `ZEX_PREFIX` env, else [`INSTALL_PREFIX`].
pub fn effective_prefix() -> std::path::PathBuf {
    std::env::var_os("ZEX_PREFIX")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::PathBuf::from(INSTALL_PREFIX))
}

/// Default locate database path (GNU-compatible env, then Zainium layout).
/// Order: `LOCATE_PATH` → `ZEX_LOCATEDB` → `$ZEX_PREFIX/var/lib/misc/locatedb`.
pub fn default_locate_db() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("LOCATE_PATH") {
        // LOCATE_PATH may be colon-separated; first entry is primary db
        let s = p.to_string_lossy();
        if let Some(first) = s.split(':').next() {
            if !first.is_empty() {
                return std::path::PathBuf::from(first);
            }
        }
    }
    if let Some(p) = std::env::var_os("ZEX_LOCATEDB") {
        if !p.is_empty() {
            return std::path::PathBuf::from(p);
        }
    }
    effective_prefix().join("var/lib/misc/locatedb")
}

/// Resolve PATH: env `PATH` if non-empty, else [`DEFAULT_PATH`].
pub fn effective_path() -> String {
    match std::env::var("PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => DEFAULT_PATH.to_string(),
    }
}

/// Split effective PATH into directory components.
pub fn path_dirs() -> Vec<std::path::PathBuf> {
    std::env::split_paths(&effective_path())
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

/// Basename of invocation path (for multicall argv0 → util name). No hardcoding.
pub fn util_name_from_argv0(argv0: &std::ffi::OsStr) -> String {
    let path = std::path::Path::new(argv0);
    path.file_name()
        .unwrap_or(argv0)
        .to_string_lossy()
        .into_owned()
}
