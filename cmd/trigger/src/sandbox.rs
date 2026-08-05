//! Process sandboxing — restricts privilege escalation
//!
//! Currently applies `PR_SET_NO_NEW_PRIVS` which prevents any process
//! executed by trigger from gaining new privileges (setuid, capabilities).
//!
//! Seccomp-BPF syscall filtering is the next hardening step — see TODO below.

/// Apply all available sandboxing for the current process.
/// Call this at binary startup before processing any user input.
pub fn apply() {
    apply_no_new_privs();
    log::debug!("sandbox: hardening applied");
}

// -- Linux implementation --------------------------------------------------

#[cfg(target_os = "linux")]
fn apply_no_new_privs() {
    // Safety: prctl(2) with PR_SET_NO_NEW_PRIVS is always safe to call.
    // It only restricts privileges — it never grants them.
    let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };

    if ret != 0 {
        log::warn!(
            "sandbox: PR_SET_NO_NEW_PRIVS failed: {}",
            std::io::Error::last_os_error()
        );
    } else {
        log::debug!("sandbox: PR_SET_NO_NEW_PRIVS set — child processes cannot gain privileges");
    }

    // TODO (Phase 2 hardening): apply seccomp-bpf filter
    // Use `seccompiler` crate to block dangerous syscalls:
    // kexec_load, init_module, delete_module, pivot_root,
    // ptrace(PTRACE_POKEDATA/TEXT), process_vm_writev
    // Keep filter loose enough to allow: execve, fork, read, write,
    // open/openat, stat, mmap, exit_group.
}

// -- Non-Linux stub --------------------------------------------------------

#[cfg(not(target_os = "linux"))]
fn apply_no_new_privs() {
    // PR_SET_NO_NEW_PRIVS is Linux-specific.
    // Redox OS handles privilege isolation at the kernel microkernel level.
}
