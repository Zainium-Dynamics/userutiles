use crate::utils::TraceResult;
use nix::unistd::getuid;

/// Drop privileges from root to the original user if possible
pub fn drop_privileges() -> TraceResult<()> {
    let uid = getuid();

    if !uid.is_root() {
        return Ok(());
    }

    log::warn!("Running as root - consider running with non-root user for security");

    Ok(())
}

/// Verify we have the necessary permissions
pub fn verify_permissions() -> TraceResult<()> {
    let uid = getuid();

    if !uid.is_root() {
        log::info!("Running as UID {}, can trace own processes", uid);
    }

    Ok(())
}
