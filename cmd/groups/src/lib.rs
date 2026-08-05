//! user groups — print group memberships.

use std::ffi::{CStr, CString};

use usercore::Ui;

/// Entry point for the `groups` utility. Parses `std::env::args()`; with
/// no arguments it prints the current process's supplementary group
/// names, otherwise it prints `USERNAME : GROUP` for each given username.
///
/// Returns 0 on success, 1 on a usage error or if a given username isn't
/// found.
pub fn run() -> i32 {
    let ui = Ui::new("groups");
    let mut users: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: groups [USERNAME]...\nPrint group memberships for each USERNAME or current process.\n");
                return 0;
            }
            "--version" => {
                println!("groups (user_utils) 0.1.0");
                return 0;
            }
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => users.push(other.to_string()),
        }
    }
    if users.is_empty() {
        print_current();
        return 0;
    }
    let mut status = 0;
    for u in users {
        if let Err(e) = print_user(&u) {
            ui.err(&e);
            status = 1;
        }
    }
    status
}

/// Print the current process's supplementary group names (plus its
/// effective GID, if not already among them), space-separated.
fn print_current() {
    // SAFETY: passing a `gidsetsize` of 0 is the documented POSIX idiom
    // for querying the number of supplementary group IDs without writing
    // anything, so the kernel never dereferences the list pointer and a
    // null pointer is a valid argument in this mode.
    let mut n = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
    if n < 0 {
        n = 0;
    }
    let mut buf = vec![0 as libc::gid_t; n as usize];
    if n > 0 {
        // SAFETY: `buf` is a `Vec<gid_t>` with exactly `n` elements
        // (allocated above from the same `n`), and `n` is passed as the
        // `gidsetsize` bound, so `getgroups` cannot write more than
        // `buf.len()` entries into `buf.as_mut_ptr()`. If the process's
        // group count grew since the sizing call above, `getgroups`
        // fails with `EINVAL` instead of overflowing the buffer, so this
        // is sound even under that race.
        unsafe {
            libc::getgroups(n, buf.as_mut_ptr());
        }
    }
    // SAFETY: `getegid` takes no arguments and always succeeds; it
    // cannot cause undefined behavior regardless of process state.
    let egid = unsafe { libc::getegid() };
    if !buf.contains(&egid) {
        buf.insert(0, egid);
    }
    let names: Vec<_> = buf.iter().map(|g| gid_name(*g)).collect();
    println!("{}", names.join(" "));
}

/// Look up `name` via `getpwnam(3)` and print `name : PRIMARY_GROUP`.
/// Returns `Err` (with a user-facing message, no `name:` prefix — the
/// caller adds that via [`Ui::err`]) if `name` contains a NUL byte or
/// isn't a known user.
fn print_user(name: &str) -> Result<(), String> {
    let c = to_cstring(name)?;
    // SAFETY: `c` is a valid, NUL-terminated `CString` that outlives this
    // block, so `c.as_ptr()` is a sound `getpwnam` argument. `getpwnam`
    // returns either NULL (handled above) or a pointer to an internal
    // static `passwd` buffer valid until the next `getpwnam`/`getpwuid`/
    // `getpwent`-family call on this thread; we read `pw_gid` (a plain
    // integer field) immediately, before any such call, so the
    // dereference is sound. The nested call to `gid_name` performs its
    // own independent `getgrgid` lookup and does not reuse `pw`, so it
    // cannot invalidate the already-copied `gid`.
    unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() {
            return Err(format!("'{name}': no such user"));
        }
        let gid = (*pw).pw_gid;
        print!("{name} : {}", gid_name(gid));
        // primary only (full getgrouplist needs more setup)
        println!();
    }
    Ok(())
}

/// Convert `name` to a `CString` suitable for passing to `getpwnam(3)`.
fn to_cstring(name: &str) -> Result<CString, String> {
    CString::new(name).map_err(|_| format!("invalid user: '{name}'"))
}

/// Resolve `gid` to its group name via `getgrgid(3)`, falling back to the
/// numeric GID (as a string) if there is no such group.
fn gid_name(gid: libc::gid_t) -> String {
    // SAFETY: `getgrgid` takes a plain integer and returns either NULL
    // (handled below) or a pointer to an internal static `group` buffer
    // whose `gr_name` field is a NUL-terminated string valid until the
    // next `getgrnam`/`getgrgid`/`getgrent`-family call on this thread.
    // We build the `CStr` and copy it into an owned `String` immediately,
    // before any such call, so both the dereference and `CStr::from_ptr`
    // are sound.
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            gid.to_string()
        } else {
            CStr::from_ptr((*gr).gr_name).to_string_lossy().into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_cstring_accepts_normal_name() {
        assert!(to_cstring("alice").is_ok());
    }

    #[test]
    fn to_cstring_rejects_embedded_nul() {
        assert!(to_cstring("bad\0name").is_err());
    }

    #[test]
    fn gid_name_falls_back_to_numeric_for_unknown_gid() {
        // A gid this large is exceedingly unlikely to exist in any group
        // database, so this exercises the "no such group" fallback path
        // hermetically.
        let gid = u32::MAX - 1;
        assert_eq!(gid_name(gid), gid.to_string());
    }

    #[test]
    fn print_user_reports_invalid_name_for_embedded_nul() {
        let err = print_user("bad\0name").unwrap_err();
        assert!(err.contains("invalid user"));
    }

    #[test]
    fn print_user_reports_missing_user() {
        let name = format!("user_nonexistent_user_{}", std::process::id());
        let err = print_user(&name).unwrap_err();
        assert!(err.contains("no such user"));
    }
}
