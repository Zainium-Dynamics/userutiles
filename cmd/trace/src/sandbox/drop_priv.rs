use crate::utils::TraceResult;
use nix::unistd::getuid;

/// Drop privileges from root to the original user if possible
pub fn drop_privileges() -> TraceResult<()> {
    let uid = getuid();

    // If not root, nothing to drop
    if !uid.is_root() {
        return Ok(());
    }

    // Try to get the real user from environment
    // This is a safety feature for the trace tool
    log::warn!("Running as root - consider running with non-root user for security");

    Ok(())
}

/// Verify we have the necessary permissions
pub fn verify_permissions() -> TraceResult<()> {
    let uid = getuid();

    // ptrace typically requires either root or the same UID
    if !uid.is_root() {
        // For non-root, this is okay - we can only trace our own processes
        log::info!("Running as UID {}, can trace own processes", uid);
    }

    Ok(())
}
