//! user-seccomp — lightweight process hardening for Zainium Dynamics tools.
//!
//! Applies best-effort `PR_SET_NO_NEW_PRIVS`. Full seccomp-bpf filters can be
//! layered later without changing call sites.

/// Apply default hardening. Never panics; failures are ignored (best-effort).
pub fn apply() {
    #[cfg(target_os = "linux")]
    {
        // prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)
        const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
        // SAFETY: the `libc` crate declares `prctl` as a bare variadic FFI
        // function (`fn prctl(option: c_int, ...) -> c_int`), so Rust does
        // not check the variadic arguments against the real prototype for
        // us. Per `man 2 prctl`, the actual C signature is
        // `int prctl(int option, unsigned long arg2, unsigned long arg3,
        // unsigned long arg4, unsigned long arg5)` — all four trailing
        // arguments are `unsigned long`, not `int`. We therefore pass them
        // explicitly typed as `libc::c_ulong` (matching `unsigned long`
        // exactly) rather than relying on Rust's default `i32` inference
        // for bare integer literals, which would silently mismatch the
        // real ABI. `option` is a valid, well-known constant
        // (`PR_SET_NO_NEW_PRIVS`); `arg2 = 1` requests the flag be set;
        // `arg3..arg5` are unused by this option and set to `0` per the
        // documented convention. This call touches no memory (no pointers
        // involved) and cannot fail in a way that corrupts state — on
        // kernels where the option is unsupported it just returns -1,
        // which we intentionally ignore (best-effort hardening).
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
