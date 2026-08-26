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

/// Config tree — Zainium has no top-level `/etc`; `elevate-umbra` (the
/// `/etc/passwd`+`/etc/shadow` replacement) reads and writes here.
pub const SYSHUB_ETC: &str = "/overlayer/syshub/etc";

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

/// Resolve a config file: `$ZEX_PREFIX/etc/<name>` (real Zainium, and
/// this is the same directory `elevate-umbra` reads/writes) if it
/// exists, else `/etc/<name>` — so tools built and tested on an
/// ordinary Linux host (no `/overlayer` tree) still find a real
/// `/etc/passwd`. Existence-checked rather than unconditional, since
/// unlike `PATH`/install-prefix this has no environment-variable
/// override of its own.
pub fn etc_path(name: &str) -> std::path::PathBuf {
    let syshub = effective_prefix().join("etc").join(name);
    if syshub.exists() {
        syshub
    } else {
        std::path::PathBuf::from("/etc").join(name)
    }
}

/// `passwd`-format user database — `etc_path("passwd")`.
pub fn passwd_path() -> std::path::PathBuf {
    etc_path("passwd")
}

/// `shadow`-format password database — `etc_path("shadow")` (on
/// Zainium, this is what `elevate-umbra` reads and writes instead of a
/// real `/etc/shadow`, but the on-disk format is unchanged).
pub fn shadow_path() -> std::path::PathBuf {
    etc_path("shadow")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ZEX_PREFIX is a process-wide env var; serialize the two tests
    // that mutate it so they can't race each other.
    static PREFIX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn etc_path_prefers_the_syshub_tree_when_present() {
        let _guard = PREFIX_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "usercore_zainium_test_present_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("etc")).unwrap();
        std::fs::write(dir.join("etc/passwd"), b"root:x:0:0::/root:/bin/sh\n").unwrap();
        // SAFETY: test-only; this test process does not spawn threads
        // that read the environment concurrently with this mutation.
        unsafe {
            std::env::set_var("ZEX_PREFIX", &dir);
        }

        assert_eq!(passwd_path(), dir.join("etc/passwd"));

        // SAFETY: test-only; undoing the mutation above.
        unsafe {
            std::env::remove_var("ZEX_PREFIX");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn etc_path_falls_back_to_etc_when_syshub_tree_is_absent() {
        let _guard = PREFIX_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "usercore_zainium_test_absent_{}",
            std::process::id()
        ));
        // SAFETY: test-only; this test process does not spawn threads
        // that read the environment concurrently with this mutation.
        unsafe {
            std::env::set_var("ZEX_PREFIX", &dir);
        }

        assert_eq!(passwd_path(), std::path::PathBuf::from("/etc/passwd"));
        assert_eq!(shadow_path(), std::path::PathBuf::from("/etc/shadow"));

        // SAFETY: test-only; undoing the mutation above.
        unsafe {
            std::env::remove_var("ZEX_PREFIX");
        }
    }
}
