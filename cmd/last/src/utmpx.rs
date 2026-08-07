//! Raw wtmp/utmpx record reading via the standard glibc utmpx API,
//! redirected to an arbitrary file (default `/var/log/wtmp`) via
//! `utmpxname(3)` — the same mechanism `last(1)`/`who(1)` use, just pointed
//! at the historical log instead of the live `/var/run/utmp` database.
use std::ffi::CString;

pub(crate) const BOOT_TIME: i16 = libc::BOOT_TIME;
pub(crate) const RUN_LVL: i16 = libc::RUN_LVL;
pub(crate) const USER_PROCESS: i16 = libc::USER_PROCESS;
pub(crate) const DEAD_PROCESS: i16 = libc::DEAD_PROCESS;

#[derive(Clone)]
pub(crate) struct Record {
    pub(crate) rec_type: i16,
    pub(crate) pid: i32,
    pub(crate) line: String,
    pub(crate) user: String,
    pub(crate) host: String,
    pub(crate) time: i64,
}

/// Reads every record from `path` in file order (oldest first).
pub(crate) fn read_all(path: &str) -> Result<Vec<Record>, String> {
    let path_c = CString::new(path).map_err(|_| format!("invalid path: {path}"))?;

    // SAFETY: `utmpxname` just stores the path for subsequent
    // `setutxent`/`getutxent`/`endutxent` calls to use instead of the
    // default `/var/run/utmp`; `path_c` stays alive for the whole call.
    if unsafe { libc::utmpxname(path_c.as_ptr()) } != 0 {
        return Err(format!(
            "cannot open {path}: {}",
            std::io::Error::last_os_error()
        ));
    }

    let mut out = Vec::new();
    // SAFETY: `setutxent`/`getutxent`/`endutxent` form the standard utmpx
    // iteration protocol: `setutxent` (re)starts the cursor, each
    // `getutxent` call returns either NULL (checked before deref) or a
    // pointer to a libc-owned record valid until the next utmpx call — we
    // only dereference `u` within this loop iteration and copy out owned
    // `String`s/plain integers, never retaining the pointer. `endutxent`
    // closes the database. Not called concurrently with other utmpx users
    // in this single-threaded CLI.
    unsafe {
        libc::setutxent();
        loop {
            let u = libc::getutxent();
            if u.is_null() {
                break;
            }
            out.push(Record {
                rec_type: (*u).ut_type,
                pid: (*u).ut_pid,
                line: cstr(&(*u).ut_line),
                user: cstr(&(*u).ut_user),
                host: cstr(&(*u).ut_host),
                time: (*u).ut_tv.tv_sec as i64,
            });
        }
        libc::endutxent();
    }
    Ok(out)
}

/// Decodes a fixed-size utmpx `c_char` field, stopping at the first NUL or
/// the end of the array (a value exactly filling the array need not be
/// NUL-terminated — see `man utmp`); bounds-checked, no unsafe needed here.
fn cstr(buf: &[libc::c_char]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}
