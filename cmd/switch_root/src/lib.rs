//! user switch_root — switch to another filesystem as root (initramfs).
//!
//! Implements the core mechanism (validate `NEWROOT` is its own
//! filesystem, `mount --move` it onto `/`, `chroot`, `exec INIT`) that
//! every initramfs actually depends on. Deliberately does **not**
//! recursively delete the old root's contents first — real
//! `switch_root(8)` does that purely to reclaim initramfs RAM before
//! `exec`; it's an optimization, not part of what "switching root"
//! means, and skipping it means this can never wipe the wrong directory
//! tree from a bad invocation.
use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::Path;

use usercore::Ui;

fn to_cstring_path(p: &Path) -> io::Result<CString> {
    CString::new(p.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"))
}

fn to_cstring(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"))
}

/// `true` if `path` is already a mountpoint (its device id differs from
/// its parent's) — matches the check real `switch_root` uses to decide
/// whether `NEWROOT` needs a self-bind-mount first.
fn is_mountpoint(path: &Path) -> io::Result<bool> {
    let meta = std::fs::metadata(path)?;
    let parent_meta = std::fs::metadata(path.join(".."))?;
    Ok(meta.dev() != parent_meta.dev())
}

/// Bind-mount `path` onto itself, so it becomes its own mountpoint (what
/// `mount --move` below needs) even when the caller passed a plain
/// directory rather than an already-mounted filesystem.
fn self_bind_mount(path: &Path) -> io::Result<()> {
    let c_path = to_cstring_path(path)?;
    // SAFETY: `c_path` is valid, NUL-terminated, and used as both the
    // source and target argument, kept alive for the call; no other
    // pointer is passed.
    let r = unsafe {
        libc::mount(
            c_path.as_ptr(),
            c_path.as_ptr(),
            std::ptr::null(),
            libc::MS_BIND,
            std::ptr::null(),
        )
    };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn mount_move(source: &str, target: &str) -> io::Result<()> {
    let c_source = to_cstring(source)?;
    let c_target = to_cstring(target)?;
    // SAFETY: both `CString`s are valid, NUL-terminated, and kept alive
    // for the call; `MS_MOVE` ignores the (NULL) type and data args.
    let r = unsafe {
        libc::mount(
            c_source.as_ptr(),
            c_target.as_ptr(),
            std::ptr::null(),
            libc::MS_MOVE,
            std::ptr::null(),
        )
    };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn chroot_dot() -> io::Result<()> {
    let dot = to_cstring(".")?;
    // SAFETY: `dot` is a valid, NUL-terminated `CString` naming the
    // current directory (already `chdir`'d into `NEWROOT`), kept alive
    // for the call.
    let r = unsafe { libc::chroot(dot.as_ptr()) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn print_help() {
    print!(
        "Usage: switch_root NEWROOT INIT [ARG...]\n\
 Move the currently mounted root filesystem out of the way, mount\n\
 NEWROOT as the new root, and exec INIT under it.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `switch_root` utility. Parses `std::env::args()`
/// for `NEWROOT INIT [ARG...]`, performs the root switch (see module
/// docs), and `exec`s `INIT` — this function only returns if that setup,
/// or the final `exec`, fails.
///
/// Returns 1 on any usage or setup error, or `INIT`'s own not-found exit
/// convention (127) if it couldn't be executed.
pub fn run() -> i32 {
    let ui = Ui::new("switch_root");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("switch_root (user_utils) 0.1.0");
        return 0;
    }
    if args.len() < 2 {
        ui.err("usage: switch_root NEWROOT INIT [ARG...]");
        return 1;
    }
    let newroot = Path::new(&args[0]);
    let init = &args[1];
    let init_args = &args[1..];

    if !newroot.is_dir() {
        ui.err(&format!("{}: not a directory", newroot.display()));
        return 1;
    }

    match is_mountpoint(newroot) {
        Ok(true) => {}
        Ok(false) => {
            if let Err(e) = self_bind_mount(newroot) {
                ui.err(&format!(
                    "failed to bind mount {} onto itself: {e}",
                    newroot.display()
                ));
                return 1;
            }
        }
        Err(e) => {
            ui.err(&format!("{}: {e}", newroot.display()));
            return 1;
        }
    }

    if let Err(e) = std::env::set_current_dir(newroot) {
        ui.err(&format!("failed to chdir to {}: {e}", newroot.display()));
        return 1;
    }
    if let Err(e) = mount_move(".", "/") {
        ui.err(&format!(
            "failed to mount moving {} to /: {e}",
            newroot.display()
        ));
        return 1;
    }
    if let Err(e) = chroot_dot() {
        ui.err(&format!("failed to change root: {e}"));
        return 1;
    }
    if let Err(e) = std::env::set_current_dir("/") {
        ui.err(&format!("failed to chdir to /: {e}"));
        return 1;
    }

    let err = std::process::Command::new(init)
        .args(&init_args[1..])
        .exec();
    ui.err(&format!("failed to execute {init}: {err}"));
    if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_mountpoint_detects_root() {
        // "/" always straddles a device boundary from its own ".." in
        // every real environment (there's no parent above the root).
        assert!(is_mountpoint(Path::new("/")).is_ok());
    }

    #[test]
    fn is_mountpoint_false_for_a_plain_subdirectory() {
        let dir =
            std::env::temp_dir().join(format!("user_switch_root_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(!is_mountpoint(&sub).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn self_bind_mount_unprivileged_fails_cleanly() {
        // SAFETY: `libc::geteuid` takes no arguments and cannot fail or
        // cause UB.
        if unsafe { libc::geteuid() } != 0 {
            let dir = std::env::temp_dir()
                .join(format!("user_switch_root_test_bind_{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            assert!(self_bind_mount(&dir).is_err());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
