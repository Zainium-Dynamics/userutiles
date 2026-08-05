//! user logname — print the name of the current user.
use std::ffi::CStr;

use usercore::Ui;

/// Entry point for the `logname` utility. Takes no operands (only
/// `-h`/`--help`/`--version`); prints the invoking user's login name,
/// resolved in order from `$LOGNAME`, `getlogin(3)`, then a `getpwuid(3)`
/// lookup of the real uid.
///
/// Returns 0 on success, 1 if no login name could be determined or on a
/// usage error.
pub fn run() -> i32 {
    let ui = Ui::new("logname");
    // Only the first argument is meaningful: `logname` takes at most one
    // flag. (Using `if let` instead of a `for` loop avoids a
    // clippy::never_loop lint, since every match arm below returns
    // unconditionally anyway.)
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: logname\nPrint the name of the current user.\n");
                return 0;
            }
            "--version" => {
                println!("logname (user_utils) 0.1.0");
                return 0;
            }
            other => {
                ui.err(&format!("invalid option -- '{other}'"));
                return 1;
            }
        }
    }
    if let Some(n) = nonempty_env("LOGNAME") {
        println!("{n}");
        return 0;
    }
    // SAFETY: `getlogin` takes no arguments and returns either NULL
    // (handled below) or a pointer to an internal static buffer holding a
    // NUL-terminated string that remains valid until the next
    // `getlogin`-family call on this thread; we build a `CStr` from it
    // and print it immediately, then return, so no later call can
    // invalidate it while it's in use. `getuid` takes no arguments and
    // cannot fail. `getpwuid` returns either NULL (handled below) or a
    // pointer to an internal static `passwd` buffer whose `pw_name` field
    // is a NUL-terminated string valid until the next `getpwnam`/
    // `getpwuid`/`getpwent`-family call on this thread; we read and print
    // it immediately, before any such call, so the dereference and
    // `CStr::from_ptr` are sound.
    unsafe {
        let p = libc::getlogin();
        if !p.is_null() {
            println!("{}", CStr::from_ptr(p).to_string_lossy());
            return 0;
        }
        let uid = libc::getuid();
        let pw = libc::getpwuid(uid);
        if !pw.is_null() {
            println!("{}", CStr::from_ptr((*pw).pw_name).to_string_lossy());
            return 0;
        }
    }
    ui.err("no login name");
    1
}

/// Read env var `key`, treating an unset or empty value the same way
/// (both mean "not usable as a login name").
fn nonempty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty_env_missing_var_is_none() {
        assert_eq!(nonempty_env("ZEX_LOGNAME_TEST_UNSET_VAR"), None);
    }

    #[test]
    fn nonempty_env_empty_value_is_none() {
        // SAFETY: single-threaded test; no concurrent env access here.
        unsafe {
            std::env::set_var("ZEX_LOGNAME_TEST_EMPTY", "");
        }
        assert_eq!(nonempty_env("ZEX_LOGNAME_TEST_EMPTY"), None);
        // SAFETY: see above.
        unsafe {
            std::env::remove_var("ZEX_LOGNAME_TEST_EMPTY");
        }
    }

    #[test]
    fn nonempty_env_present_value_is_returned() {
        // SAFETY: single-threaded test; no concurrent env access here.
        unsafe {
            std::env::set_var("ZEX_LOGNAME_TEST_SET", "alice");
        }
        assert_eq!(
            nonempty_env("ZEX_LOGNAME_TEST_SET"),
            Some("alice".to_string())
        );
        // SAFETY: see above.
        unsafe {
            std::env::remove_var("ZEX_LOGNAME_TEST_SET");
        }
    }
}
