//! user login — begin a session on a terminal.
//!
//! Reads `usercore::zainium::passwd_path()`/`shadow_path()` (Zainium has
//! no top-level `/etc` — these resolve to `/overlayer/syshub/etc/{passwd,
//! shadow}`, the same files `elevate-umbra` reads/writes in place of a
//! real `/etc/shadow`, falling back to plain `/etc/{passwd,shadow}` on a
//! host with no `/overlayer` tree) and verifies the password via the
//! system's own `crypt(3)` (so every hash format the platform's
//! libc/libcrypt supports — MD5 `$1$`, SHA-256/512 `$5$`/`$6$`, yescrypt
//! `$y$`, … — just works, without reimplementing any of them). Meant to
//! be invoked as root (by `agetty`, or directly) — like real `login(1)`,
//! it can only read the shadow file and change identity when it already
//! has the privilege to do so.
//!
//! Scope: no PAM, no utmp/wtmp session accounting (so `last` won't show
//! logins made through this binary — a real gap, not silently skipped;
//! see `checklist/`), no account-expiry (`chage`) enforcement beyond a
//! locked (`!`/`*`) password hash. Just enough to authenticate against
//! the real shadow file and exec the user's shell as a login shell.
use std::ffi::{CStr, CString};
use std::fs;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::process::Command;

use usercore::Ui;

const MAX_ATTEMPTS: u32 = 3;

struct PasswdEntry {
    uid: u32,
    gid: u32,
    home: String,
    shell: String,
}

fn find_passwd_entry(text: &str, username: &str) -> Option<PasswdEntry> {
    for line in text.lines() {
        let mut f = line.split(':');
        let name = f.next()?;
        if name != username {
            continue;
        }
        let _passwd = f.next()?;
        let uid = f.next()?.parse().ok()?;
        let gid = f.next()?.parse().ok()?;
        let _gecos = f.next()?;
        let home = f.next()?.to_string();
        let shell = f.next().unwrap_or("/bin/sh").to_string();
        return Some(PasswdEntry {
            uid,
            gid,
            home,
            shell,
        });
    }
    None
}

/// `None` means "no such user" (distinct from an empty-hash entry).
fn find_shadow_hash(text: &str, username: &str) -> Option<String> {
    for line in text.lines() {
        let mut f = line.split(':');
        let name = f.next()?;
        if name != username {
            continue;
        }
        return Some(f.next().unwrap_or("").to_string());
    }
    None
}

/// `!`/`*`-prefixed (or empty) hashes mean "no password login" — the
/// same convention every real `/etc/shadow` reader honors.
fn account_allows_password_login(hash: &str) -> bool {
    !hash.is_empty() && !hash.starts_with('!') && !hash.starts_with('*')
}

// crypt(3) lives in libc itself on musl, but needs -lcrypt on glibc
// (split out to libxcrypt decades ago) — link it only where it's
// actually a separate library.
#[cfg_attr(target_env = "gnu", link(name = "crypt"))]
extern "C" {
    fn crypt(key: *const libc::c_char, salt: *const libc::c_char) -> *mut libc::c_char;
}

/// `crypt(3)` (as opposed to the reentrant `crypt_r`) writes its result
/// into a single buffer shared process-wide and isn't documented as
/// thread-safe — two concurrent calls can clobber each other's output.
/// `login` itself only ever calls this once per process, so this never
/// mattered there; it does matter for this crate's own test suite,
/// which runs test functions concurrently by default. Serialize every
/// call through this lock instead of switching to `crypt_r` (whose
/// `struct crypt_data` layout isn't guaranteed identical across
/// glibc/musl, unlike this Mutex, which needs no ABI assumptions).
static CRYPT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Call `crypt(3)` under [`CRYPT_LOCK`] and copy its result out of
/// libc's shared buffer before releasing the lock. `None` on a bad
/// (NUL-containing) input or a `crypt(3)` failure.
fn crypt_locked(key: &str, salt: &str) -> Option<String> {
    let c_key = CString::new(key).ok()?;
    let c_salt = CString::new(salt).ok()?;
    let _guard = CRYPT_LOCK.lock().unwrap();
    // SAFETY: both C strings are valid, NUL-terminated, and kept alive
    // for the call. `crypt` returns a pointer into a buffer owned by
    // libc (never freed by the caller) or NULL on error; we only read
    // through it before the next call, never write — and `CRYPT_LOCK`
    // ensures no other thread in this process calls `crypt` between
    // this call and that read.
    let result = unsafe { crypt(c_key.as_ptr(), c_salt.as_ptr()) };
    if result.is_null() {
        return None;
    }
    // SAFETY: `result` was just checked non-NULL and points at libc's
    // crypt buffer, still valid because `CRYPT_LOCK` is still held.
    Some(
        unsafe { CStr::from_ptr(result) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// Verify `password` against `stored_hash` via the system's own
/// `crypt(3)` — `stored_hash` itself is used as the salt argument, since
/// every `$id$salt$...` hash format encodes its own salt in that prefix.
fn verify_password(password: &str, stored_hash: &str) -> bool {
    // Constant-ish comparison isn't attempted here (crypt(3)'s own
    // buffer already isn't a secret-independent-time API); this matches
    // what real login(1) does too — the timing signal is dominated by
    // crypt()'s own KDF cost either way.
    crypt_locked(password, stored_hash).as_deref() == Some(stored_hash)
}

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Read a line from stdin with terminal echo turned off, restoring the
/// original mode afterward regardless of how reading went.
fn prompt_no_echo(label: &str) -> io::Result<String> {
    let fd = 0; // stdin
                // SAFETY: an all-zero `termios` is a valid initial value —
                // `tcgetattr` below fully populates it before any field is read.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: `term` is a valid out-param; `fd` is the standard,
    // always-valid stdin descriptor.
    let have_tty = unsafe { libc::tcgetattr(fd, &mut term) } == 0;
    if have_tty {
        let mut noecho = term;
        noecho.c_lflag &= !libc::ECHO;
        // SAFETY: `noecho` was read from a real `tcgetattr` call above
        // with only `ECHO` cleared; `fd` is stdin.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &noecho) };
    }

    print!("{label}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let result = io::stdin().read_line(&mut line);

    if have_tty {
        // SAFETY: restoring the exact settings `tcgetattr` reported
        // before this function touched anything.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &term) };
    }
    println!();
    result?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Drop privileges to `entry`'s uid/gid — group first, matching every
/// correct setuid-dropping order (dropping uid first would leave the
/// process unable to change its gid afterward).
fn switch_identity(entry: &PasswdEntry) -> io::Result<()> {
    // SAFETY: plain integer argument, no pointers; failure is reported
    // through the return value, which is checked.
    let gid_ok = unsafe { libc::setgid(entry.gid) } == 0;
    // SAFETY: plain integer argument, no pointers; only called after
    // `setgid` succeeded, and its own failure is checked the same way.
    let uid_ok = gid_ok && unsafe { libc::setuid(entry.uid) } == 0;
    if uid_ok {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn print_help() {
    print!(
        "Usage: login [-f] [USERNAME]\n\
 Begin a session on this terminal: prompt for a username (if not given)\n\
 and password, verify against the system's passwd/shadow database, then\n\
 exec the user's shell as a login shell.\n\
 -f skip password verification -- USERNAME is already authenticated\n\
    by the (trusted, real-root) caller, e.g. agetty --autologin.\n\
    Refused unless this process is already running as real uid 0,\n\
    matching real login(1)'s -f trust boundary.\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `login` utility. Prompts for a username (or uses
/// the one given on `std::env::args()`) and password, verifies it, then
/// `exec`s the user's login shell — this function only returns on
/// failure (after [`MAX_ATTEMPTS`] tries) or a fatal setup error.
///
/// Returns 1 after repeated failed attempts, or if the passwd database
/// (see [`usercore::zainium::passwd_path`]) couldn't be read at all.
pub fn run() -> i32 {
    let ui = Ui::new("login");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("login (user_utils) 0.1.0");
        return 0;
    }

    // `-f USERNAME`: caller (agetty --autologin, or another trusted,
    // already-root program) asserts USERNAME is already authenticated —
    // matches real login(1)'s -f. Only honored when this process's real
    // uid is already 0: -f is a trust-boundary bypass, not a general
    // "skip the password" switch, so an unprivileged invocation just
    // falls through to the normal prompt-and-verify path below instead
    // of silently granting a free login.
    let preauth_user = if let Some(pos) = args.iter().position(|a| a == "-f") {
        let user = args.get(pos + 1).cloned();
        // SAFETY: getuid(2) takes no arguments and cannot fail.
        let real_root = unsafe { libc::getuid() } == 0;
        match (user, real_root) {
            (Some(u), true) => Some(u),
            (Some(_), false) => {
                ui.err("-f requires real uid 0 -- ignoring, falling back to password login");
                None
            }
            (None, _) => {
                ui.err("option requires an argument -- 'f'");
                return 1;
            }
        }
    } else {
        None
    };

    let passwd_path = usercore::zainium::passwd_path();
    let passwd_text = match fs::read_to_string(&passwd_path) {
        Ok(t) => t,
        Err(e) => {
            ui.err(&format!("{}: {e}", passwd_path.display()));
            return 1;
        }
    };
    let shadow_text = fs::read_to_string(usercore::zainium::shadow_path()).unwrap_or_default();

    // Positional args with a recognized `-f USERNAME` pair stripped out,
    // so the normal prompt-and-verify path below never sees `-f` itself
    // as a literal username (matters both when `-f` was honored above,
    // and when it was refused for lacking real root and we fall through
    // to a real password prompt for that same invocation).
    let mut positional = args.clone();
    if let Some(pos) = positional.iter().position(|a| a == "-f") {
        positional.drain(pos..(pos + 2).min(positional.len()));
    }

    if let Some(username) = preauth_user {
        let entry = find_passwd_entry(&passwd_text, &username);
        let Some(entry) = entry else {
            ui.err(&format!("-f: no such user: {username}"));
            return 1;
        };
        return exec_login_shell(&ui, &username, &entry);
    }

    for _ in 0..MAX_ATTEMPTS {
        let username = match positional.first() {
            Some(u) => u.clone(),
            None => match prompt("login: ") {
                Ok(u) => u,
                Err(_) => return 1,
            },
        };
        let password = match prompt_no_echo("Password: ") {
            Ok(p) => p,
            Err(_) => return 1,
        };

        let entry = find_passwd_entry(&passwd_text, &username);
        let hash = find_shadow_hash(&shadow_text, &username);

        let authenticated = match (&entry, &hash) {
            (Some(_), Some(h)) if account_allows_password_login(h) => verify_password(&password, h),
            _ => false,
        };

        if authenticated {
            return exec_login_shell(&ui, &username, &entry.unwrap());
        }

        println!("Login incorrect");
    }
    1
}

/// Finish a successful login: chdir home, drop to the user's identity,
/// populate the standard session env vars, then exec the login shell.
/// Only returns (with a nonzero code) if a step failed — a real success
/// replaces this process entirely.
fn exec_login_shell(ui: &Ui, username: &str, entry: &PasswdEntry) -> i32 {
    if let Err(e) = std::env::set_current_dir(&entry.home) {
        ui.err(&format!("cannot chdir to {}: {e}", entry.home));
        return 1;
    }
    if let Err(e) = switch_identity(entry) {
        ui.err(&format!("cannot switch to user {username}: {e}"));
        return 1;
    }
    std::env::set_var("HOME", &entry.home);
    std::env::set_var("SHELL", &entry.shell);
    std::env::set_var("USER", username);
    std::env::set_var("LOGNAME", username);
    // A new session's PATH is established fresh, never inherited — the
    // same rule real login(1) follows (ENV_PATH/ENV_SUPATH in
    // login.defs). This matters concretely on Zainium: agetty/login are
    // spawned by a systemd unit with no `Environment=PATH=...` override,
    // so the process inherits systemd's own compiled-in default
    // (`/usr/bin:/bin`-style FHS paths that don't exist here) — and
    // effective_path() (which prefers an already-non-empty env PATH)
    // would silently keep that broken value instead of falling back.
    // Bypass it and always start the session with Zainium's real path.
    std::env::set_var("PATH", usercore::zainium::DEFAULT_PATH);
    std::env::set_var("LD_LIBRARY_PATH", usercore::zainium::DEFAULT_LD_LIBRARY_PATH);
    // sys_resolver's LD_PRELOAD is what makes any bare FHS-shaped path
    // (/etc/greetd/config.toml, /etc/profile, ...) resolve at all on
    // Zainium. system.conf.d's DefaultEnvironment= only reaches units
    // systemd itself spawns -- a manually-typed command from this very
    // shell never sees it, no matter how fresh the ISO is. Set it here
    // directly, same guaranteed-not-inherited reasoning as PATH above,
    // so testing anything by hand from an interactive login session
    // actually has the same env real systemd units are supposed to.
    std::env::set_var("LD_PRELOAD", "/overlayer/syshub/lib/libsys_resolver.so");

    // Real login(1)'s login.defs (ENV_PATH/ENV_SUPATH) is exactly this
    // pattern: a fresh login session gets its own known-good env, not
    // whatever it happened to inherit. PS1 belongs in that same set --
    // shipping it through /overlayer/syshub/etc/profile instead (relying
    // on bash's own login-shell startup, remapped via sys_resolver's
    // LD_PRELOAD) was tried first and left unverified whether it's
    // actually reached by the time bash starts (a getty->login->bash
    // exec chain, LD_PRELOAD propagation through systemd's
    // DefaultEnvironment -- too many links to trust without a live
    // test). Setting it directly here has none of that: an inherited
    // PS1 env var is just a plain shell variable to bash, no file lookup
    // involved, guaranteed to show correctly the first time.
    std::env::set_var("PS1", "\\u@\\h:\\w\\$ ");

    // Login shell convention: argv[0] starts with '-'.
    let shell_name = entry.shell.rsplit('/').next().unwrap_or(&entry.shell);
    let err = Command::new(&entry.shell)
        .arg0(format!("-{shell_name}"))
        .exec();
    ui.err(&format!("failed to execute {}: {err}", entry.shell));
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_passwd_entry_parses_known_fields() {
        let text =
            "root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/zsh\n";
        let e = find_passwd_entry(text, "alice").unwrap();
        assert_eq!(e.uid, 1000);
        assert_eq!(e.gid, 1000);
        assert_eq!(e.home, "/home/alice");
        assert_eq!(e.shell, "/bin/zsh");
        assert!(find_passwd_entry(text, "nobody-here").is_none());
    }

    #[test]
    fn find_shadow_hash_reads_the_hash_field() {
        let text = "root:$6$abc$hash:19000:0:99999:7:::\nlocked:!:19000:0:99999:7:::\n";
        assert_eq!(
            find_shadow_hash(text, "root"),
            Some("$6$abc$hash".to_string())
        );
        assert_eq!(find_shadow_hash(text, "locked"), Some("!".to_string()));
        assert_eq!(find_shadow_hash(text, "nobody"), None);
    }

    #[test]
    fn account_allows_password_login_rejects_locked_forms() {
        assert!(!account_allows_password_login(""));
        assert!(!account_allows_password_login("!"));
        assert!(!account_allows_password_login("!$6$abc$hash"));
        assert!(!account_allows_password_login("*"));
        assert!(account_allows_password_login("$6$abc$hash"));
    }

    #[test]
    fn verify_password_matches_an_independently_generated_python_crypt_hash() {
        let hash = "$6$KNAGnQW.bNsSwCoC$J9v/GkUUAn.3Y/qtDGIj1tAtqvV/vqVZwX4U0W0sjezfQCpp3wlGOsKkiHtyt9Q9Ory6rJClP9BPqD6dt8rJn.";
        assert!(verify_password("testpass123", hash));
        assert!(!verify_password("wrongpass", hash));
    }

    #[test]
    fn verify_password_round_trips_via_the_real_system_crypt() {
        // Hash our own password with the same crypt() we verify with
        // (through the same locked helper — see CRYPT_LOCK's docs for
        // why a direct, unlocked crypt() call here would race against
        // other tests in this file), so this test is meaningful on
        // whatever libc/libcrypt the build machine actually has, rather
        // than hardcoding a vector tied to one specific algorithm.
        let hashed = crypt_locked("correct horse battery staple", "$6$usertestsalt$").unwrap();

        assert!(
            verify_password("correct horse battery staple", &hashed),
            "verify_password failed against a hash crypt() itself just produced: {hashed:?}"
        );
        assert!(!verify_password("wrong password", &hashed));
    }
}
