//! user id — print user identity.
use usercore::Ui;

/// Entry point for the `id` utility. Parses `std::env::args()` and prints
/// the (real or effective) user/group identity for the named USER, or for
/// the current process if none is given.
///
/// Returns 0 on success, 1 on a usage error or unknown user.
pub fn run() -> i32 {
    let ui = Ui::new("id");
    let mut user_only = false;
    let mut group_only = false;
    let mut groups_only = false;
    let mut name = false;
    let mut real = false;
    let mut user_arg: Option<String> = None;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: id [OPTION]... [USER]\n\
 Print user and group information for the specified USER,\n\
 or (when USER omitted) for the current user.\n\n\
 -g, --group print only the effective group ID\n\
 -G, --groups print all group IDs\n\
 -n, --name print a name instead of a number\n\
 -r, --real print the real ID instead of the effective ID\n\
 -u, --user print only the effective user ID\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("id (user_utils) 0.1.0");
                return 0;
            }
            "-u" | "--user" => user_only = true,
            "-g" | "--group" => group_only = true,
            "-G" | "--groups" => groups_only = true,
            "-n" | "--name" => name = true,
            "-r" | "--real" => real = true,
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'u' => user_only = true,
                        'g' => group_only = true,
                        'G' => groups_only = true,
                        'n' => name = true,
                        'r' => real = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            other => user_arg = Some(other.to_string()),
        }
    }

    let (uid, gid, groups) = if let Some(ref u) = user_arg {
        match lookup_user(u) {
            Ok(v) => v,
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        }
    } else {
        // SAFETY: `getuid`/`geteuid`/`getgid`/`getegid` (below) take no
        // arguments and simply read process credential state; none of
        // them can fail or cause undefined behavior regardless of
        // process state.
        let uid = if real {
            // SAFETY: see comment above.
            unsafe { libc::getuid() }
        } else {
            // SAFETY: see comment above.
            unsafe { libc::geteuid() }
        };
        let gid = if real {
            // SAFETY: see comment above.
            unsafe { libc::getgid() }
        } else {
            // SAFETY: see comment above.
            unsafe { libc::getegid() }
        };
        let groups = get_groups();
        (uid, gid, groups)
    };

    if user_only {
        if name {
            println!("{}", uid_name(uid));
        } else {
            println!("{uid}");
        }
        return 0;
    }
    if group_only {
        if name {
            println!("{}", gid_name(gid));
        } else {
            println!("{gid}");
        }
        return 0;
    }
    if groups_only {
        let mut parts = Vec::new();
        for g in &groups {
            if name {
                parts.push(gid_name(*g));
            } else {
                parts.push(g.to_string());
            }
        }
        println!("{}", parts.join(" "));
        return 0;
    }

    // default full format
    print!("uid={uid}({}) gid={gid}({})", uid_name(uid), gid_name(gid));
    if !groups.is_empty() {
        print!(" groups=");
        for (i, g) in groups.iter().enumerate() {
            if i > 0 {
                print!(",");
            }
            print!("{g}({})", gid_name(*g));
        }
    }
    println!();
    0
}

/// Look up `name` directly against Zainium's real passwd/group files
/// (`usercore::nss`) and return its `(uid, gid, groups)`, including
/// secondary group membership. Bypasses `getpwnam`/`getgrouplist`
/// entirely: those go through libc's own internal, non-interposable
/// `/etc/passwd`+`/etc/group` access, which only resolves to Zainium's
/// real path if the deployed `libc.so` happens to be built from an
/// already-patched musl source.
fn lookup_user(name: &str) -> Result<(u32, u32, Vec<u32>), String> {
    let pw = usercore::nss::user_by_name(name).ok_or_else(|| format!("'{name}': no such user"))?;
    let groups = usercore::nss::groups_for_user(&pw.name, pw.gid);
    Ok((pw.uid, pw.gid, groups))
}

/// Return the calling process's supplementary group IDs via
/// `getgroups(2)`, falling back to `[getegid()]` if the query fails.
fn get_groups() -> Vec<u32> {
    // SAFETY: `getgroups(0, null)` is the documented POSIX idiom for
    // querying the supplementary group count without writing anything, so
    // a null list pointer is valid in that mode. `getegid` takes no
    // arguments and cannot fail. `buf` is then allocated with exactly `n`
    // elements (from that same query), and `n` is passed again as the
    // `gidsetsize` bound, so the second `getgroups` call cannot write
    // past `buf`; if the group count grew in the meantime it fails with
    // `EINVAL` (handled via the `n < 0` check) rather than overflowing
    // the buffer. `buf.truncate(n as usize)` is bounded by `buf.len()` by
    // construction (`n` can only shrink from the original count on
    // success), so it cannot panic.
    unsafe {
        let mut n = libc::getgroups(0, std::ptr::null_mut());
        if n < 0 {
            return vec![libc::getegid()];
        }
        let mut buf = vec![0 as libc::gid_t; n as usize];
        n = libc::getgroups(n, buf.as_mut_ptr());
        if n < 0 {
            return vec![libc::getegid()];
        }
        buf.truncate(n as usize);
        buf.into_iter().collect()
    }
}

/// Resolve `uid` to a user name via `usercore::nss` (direct passwd-file
/// lookup, not `getpwuid`), falling back to the numeric id (stringified)
/// if no matching passwd entry exists.
fn uid_name(uid: u32) -> String {
    usercore::nss::user_by_uid(uid)
        .map(|pw| pw.name)
        .unwrap_or_else(|| uid.to_string())
}

/// Resolve `gid` to a group name via `usercore::nss` (direct group-file
/// lookup, not `getgrgid`), falling back to the numeric id (stringified)
/// if no matching group entry exists.
fn gid_name(gid: u32) -> String {
    usercore::nss::group_by_gid(gid)
        .map(|gr| gr.name)
        .unwrap_or_else(|| gid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_user_unknown_name_errors() {
        let err = lookup_user("no_such_user_user_id_test").unwrap_err();
        assert!(err.contains("no such user"));
    }

    #[test]
    fn lookup_user_root_resolves_to_uid_zero() {
        // "root" (uid 0) exists on every Linux system this crate targets.
        let (uid, _gid, groups) = lookup_user("root").expect("root should resolve");
        assert_eq!(uid, 0);
        assert!(!groups.is_empty());
    }

    #[test]
    fn get_groups_is_nonempty() {
        assert!(!get_groups().is_empty());
    }

    #[test]
    fn uid_name_root_is_root() {
        assert_eq!(uid_name(0), "root");
    }

    #[test]
    fn uid_name_unknown_uid_falls_back_to_number() {
        // A UID this unlikely to exist on any real system.
        assert_eq!(uid_name(u32::MAX - 1), (u32::MAX - 1).to_string());
    }

    #[test]
    fn gid_name_unknown_gid_falls_back_to_number() {
        assert_eq!(gid_name(u32::MAX - 1), (u32::MAX - 1).to_string());
    }
}
