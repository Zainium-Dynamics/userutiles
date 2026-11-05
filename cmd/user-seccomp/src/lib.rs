//! user-seccomp: lightweight process hardening for Zainium Dynamics tools.

/// Apply default hardening. Never panics; failures are ignored (best-effort).
pub fn apply() {
    #[cfg(target_os = "linux")]
    {
        const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
        // SAFETY: the `libc` crate declares `prctl` as a bare variadic FFI
        unsafe {
            let _ = libc::prctl(
                PR_SET_NO_NEW_PRIVS,
                1 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            );
        }
    }
}
