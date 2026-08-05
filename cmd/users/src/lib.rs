//! user users — print login names currently logged in.
use usercore::Ui;

/// Entry point for the `users` utility. Parses `std::env::args()` and
/// prints the sorted, space-separated login names of every `USER_PROCESS`
/// entry in the utmpx database.
///
/// Returns 0 on success, 1 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("users");
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: users\nPrint the user names of users currently logged in.\n");
                return 0;
            }
            "--version" => {
                println!("users (user_utils) 0.1.0");
                return 0;
            }
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            _ => {}
        }
    }
    let mut names = logged_in_users();
    names.sort();
    if !names.is_empty() {
        println!("{}", names.join(" "));
    }
    0
}

/// Return the login names of every `USER_PROCESS` entry currently in the
/// utmpx database, in database order (unsorted, possibly with duplicates —
/// callers that want the classic `users(1)` output sort it).
fn logged_in_users() -> Vec<String> {
    let mut names = Vec::new();
    // SAFETY: `setutxent`/`getutxent`/`endutxent` form the standard utmpx
    // iteration protocol: `setutxent` (re)starts the cursor, each
    // `getutxent` call returns either NULL (checked before deref) or a
    // pointer to a libc-owned record valid until the next utmpx call — we
    // only dereference `u` within this same loop iteration and copy the
    // `ut_user` field out into an owned `String` via the bounds-checked
    // `utmp_field_to_string` helper below, never retaining the pointer
    // itself. `endutxent` closes the database. Not called concurrently with
    // other utmpx users in this single-threaded CLI.
    unsafe {
        libc::setutxent();
        loop {
            let u = libc::getutxent();
            if u.is_null() {
                break;
            }
            if (*u).ut_type != libc::USER_PROCESS {
                continue;
            }
            let n = utmp_field_to_string(&(*u).ut_user);
            if !n.is_empty() {
                names.push(n);
            }
        }
        libc::endutxent();
    }
    names
}

/// Decode a fixed-size utmpx `c_char` field (e.g. `ut_user`) into a
/// `String`, stopping at the first NUL byte or the end of the array —
/// whichever comes first.
///
/// Per `man utmp`, these fixed-size fields are only NUL-terminated when the
/// value is *shorter* than the field; a value that exactly fills the array
/// is not guaranteed to have a trailing NUL. Scanning with `CStr::from_ptr`
/// directly on such a field would risk reading past the end of the array.
/// Taking a slice keeps the length known and bounds-checked by the
/// language, so this needs no `unsafe` at all.
fn utmp_field_to_string(buf: &[libc::c_char]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utmp_field_to_string_stops_at_nul() {
        let mut buf = [0 as libc::c_char; 8];
        for (i, b) in b"ali\0xxxx".iter().enumerate() {
            buf[i] = *b as libc::c_char;
        }
        assert_eq!(utmp_field_to_string(&buf), "ali");
    }

    #[test]
    fn utmp_field_to_string_full_field_no_nul() {
        let buf = [b'a' as libc::c_char; 4];
        assert_eq!(utmp_field_to_string(&buf), "aaaa");
    }

    #[test]
    fn utmp_field_to_string_empty_field() {
        let buf = [0 as libc::c_char; 4];
        assert_eq!(utmp_field_to_string(&buf), "");
    }

    #[test]
    fn logged_in_users_does_not_panic() {
        // Just exercise the unsafe utmpx walk end-to-end; contents are
        // environment-dependent so we only assert it returns cleanly.
        let _ = logged_in_users();
    }
}
