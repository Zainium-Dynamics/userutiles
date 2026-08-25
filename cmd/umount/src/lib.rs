//! user umount — unmount a filesystem.
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::Path;

use usercore::Ui;

fn to_cstring(s: &str) -> io::Result<CString> {
    CString::new(s).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"))
}

/// Read `/proc/self/mounts` as `(device, mountpoint)` pairs, in the
/// kernel's own (mount-order) sequence.
fn read_mounts() -> io::Result<Vec<(String, String)>> {
    let text = fs::read_to_string("/proc/self/mounts")?;
    Ok(text
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some((f.next()?.to_string(), f.next()?.to_string()))
        })
        .collect())
}

/// Resolve `target` to a mountpoint: if it already names one, use it
/// as-is; if it names a mounted device instead, use that device's
/// mountpoint. `umount2(2)` only accepts a mountpoint path, never a
/// device, so this mirrors `umount`'s own device-argument convenience.
fn resolve_mountpoint(mounts: &[(String, String)], target: &str) -> Option<String> {
    if mounts.iter().any(|(_, mp)| mp == target) {
        return Some(target.to_string());
    }
    mounts
        .iter()
        .rev()
        .find(|(dev, _)| dev == target)
        .map(|(_, mp)| mp.clone())
}

fn do_umount(target: &str, force: bool, lazy: bool) -> io::Result<()> {
    let c_target = to_cstring(target)?;
    let mut flags = 0;
    if force {
        flags |= libc::MNT_FORCE;
    }
    if lazy {
        flags |= libc::MNT_DETACH;
    }
    // SAFETY: `c_target` is a valid, NUL-terminated `CString` kept alive
    // for the call; `umount2(2)` takes no other pointer arguments.
    let r = unsafe { libc::umount2(c_target.as_ptr(), flags) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn print_help() {
    print!(
        "Usage: umount [-fln] {{dir|device}}...\n\
 umount -a [-fln]\n\
 -f force unmount (in case of an unreachable NFS system)\n\
 -l lazy unmount (detach now, clean up later)\n\
 -R recursively unmount every mount under each target\n\
 -a unmount all mounted filesystems\n\
 -n don't touch mtab (no-op — no mtab is maintained)\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `umount` utility. Parses `std::env::args()` and
/// unmounts each operand (accepting either its mountpoint or its mounted
/// device path), recursing into sub-mounts first with `-R`, or (`-a`)
/// unmounts everything in `/proc/self/mounts`, deepest mounts first.
///
/// Returns 0 on success, 1 if any requested unmount failed.
pub fn run() -> i32 {
    let ui = Ui::new("umount");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut force = false;
    let mut lazy = false;
    let mut recursive = false;
    let mut all = false;
    let mut targets: Vec<String> = Vec::new();

    for arg in &args {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("umount (user_utils) 0.1.0");
                return 0;
            }
            "-f" | "--force" => force = true,
            "-l" | "--lazy" => lazy = true,
            "-R" | "--recursive" => recursive = true,
            "-a" | "--all" => all = true,
            "-n" | "--no-mtab" => {}
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => targets.push(other.to_string()),
        }
    }

    let mounts = match read_mounts() {
        Ok(m) => m,
        Err(e) => {
            ui.err(&format!("/proc/self/mounts: {e}"));
            return 1;
        }
    };

    // Deepest-path-first order, so a child mount is always unmounted
    // before its parent — required for -a and for -R on a subtree.
    let mut plan: Vec<String> = if all {
        let mut mps: Vec<String> = mounts.iter().map(|(_, mp)| mp.clone()).collect();
        mps.sort_by_key(|b| std::cmp::Reverse(b.len()));
        mps
    } else {
        let mut plan = Vec::new();
        for t in &targets {
            let Some(mp) = resolve_mountpoint(&mounts, t) else {
                ui.err(&format!("{t}: not mounted"));
                return 1;
            };
            if recursive {
                let mut under: Vec<String> = mounts
                    .iter()
                    .map(|(_, m)| m.clone())
                    .filter(|m| *m == mp || m.starts_with(&format!("{mp}/")))
                    .collect();
                under.sort_by_key(|b| std::cmp::Reverse(b.len()));
                plan.extend(under);
            } else {
                plan.push(mp);
            }
        }
        plan
    };
    plan.dedup();

    if plan.is_empty() && !all {
        ui.err("no targets specified");
        return 1;
    }

    let mut status = 0;
    for mp in &plan {
        if let Some(reason) = usercore::protect::modification_denied(Path::new(mp)) {
            if !all {
                ui.err(&format!("{mp}: {}", reason.message()));
                status = 1;
            }
            continue;
        }
        if let Err(e) = do_umount(mp, force, lazy) {
            if !all {
                ui.err(&format!("{mp}: {e}"));
            }
            status = 1;
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mountpoint_matches_mountpoint_or_device() {
        let mounts = vec![
            ("/dev/sda1".to_string(), "/".to_string()),
            ("/dev/sda2".to_string(), "/home".to_string()),
        ];
        assert_eq!(
            resolve_mountpoint(&mounts, "/home"),
            Some("/home".to_string())
        );
        assert_eq!(
            resolve_mountpoint(&mounts, "/dev/sda2"),
            Some("/home".to_string())
        );
        assert_eq!(resolve_mountpoint(&mounts, "/nope"), None);
    }

    #[test]
    fn resolve_mountpoint_prefers_the_most_recent_mount_of_a_device() {
        // A device re-mounted at a new location shows up twice in
        // /proc/self/mounts; the last entry is the current mountpoint.
        let mounts = vec![
            ("/dev/sda1".to_string(), "/mnt/old".to_string()),
            ("/dev/sda1".to_string(), "/mnt/new".to_string()),
        ];
        assert_eq!(
            resolve_mountpoint(&mounts, "/dev/sda1"),
            Some("/mnt/new".to_string())
        );
    }

    #[test]
    fn do_umount_unprivileged_fails_cleanly() {
        // SAFETY: `libc::geteuid` takes no arguments and cannot fail or
        // cause UB.
        if unsafe { libc::geteuid() } != 0 {
            assert!(do_umount("/", false, false).is_err());
        }
    }
}
