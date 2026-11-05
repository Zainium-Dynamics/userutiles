//! Process sandboxing: restricts privilege escalation

/// Apply all available sandboxing for the current process.
/// Call this at binary startup before processing any user input.
pub fn apply() {
    apply_no_new_privs();
    log::debug!("sandbox: hardening applied");
}

#[cfg(target_os = "linux")]
fn apply_no_new_privs() {
    let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };

    if ret != 0 {
        log::warn!(
            "sandbox: PR_SET_NO_NEW_PRIVS failed: {}",
            std::io::Error::last_os_error()
        );
    } else {
        log::debug!("sandbox: PR_SET_NO_NEW_PRIVS set — child processes cannot gain privileges");
    }

}

#[cfg(not(target_os = "linux"))]
fn apply_no_new_privs() {
}
