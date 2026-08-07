//! user uname
use std::ffi::CStr;
use std::io::{self, Write};
use std::mem::MaybeUninit;

#[derive(Default)]
struct Flags {
    all: bool,
    sys: bool,
    node: bool,
    release: bool,
    version: bool,
    machine: bool,
}

pub fn run() -> i32 {
    let mut f = Flags::default();
    let mut any = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: uname [OPTION]...\nPrint certain system information.\n -a all -s kernel-name -n nodename -r release -v version -m machine\n");
                return 0;
            }
            "--version" => {
                println!("uname (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => {
                f.all = true;
                any = true;
            }
            "-s" | "--kernel-name" => {
                f.sys = true;
                any = true;
            }
            "-n" | "--nodename" => {
                f.node = true;
                any = true;
            }
            "-r" | "--kernel-release" => {
                f.release = true;
                any = true;
            }
            "-v" | "--kernel-version" => {
                f.version = true;
                any = true;
            }
            "-m" | "--machine" => {
                f.machine = true;
                any = true;
            }
            other if other.starts_with('-') && other.len() > 1 && !other.starts_with("--") => {
                for c in other.chars().skip(1) {
                    any = true;
                    match c {
                        'a' => f.all = true,
                        's' => f.sys = true,
                        'n' => f.node = true,
                        'r' => f.release = true,
                        'v' => f.version = true,
                        'm' => f.machine = true,
                        _ => {
                            eprintln!("uname: invalid option -- '{c}'");
                            return 1;
                        }
                    }
                }
            }
            other => {
                eprintln!("uname: extra operand '{other}'");
                return 1;
            }
        }
    }
    if !any {
        f.sys = true;
    }
    if f.all {
        f.sys = true;
        f.node = true;
        f.release = true;
        f.version = true;
        f.machine = true;
    }
    let mut uts = MaybeUninit::<libc::utsname>::uninit();
    // SAFETY: `uts.as_mut_ptr()` gives `libc::uname` a valid, properly
    // aligned pointer to enough space for a `libc::utsname` (the type the
    // `MaybeUninit` was declared with); the kernel fully populates every
    // field of the struct on success (return 0), which we check before
    // treating the memory as initialized.
    if unsafe { libc::uname(uts.as_mut_ptr()) } != 0 {
        eprintln!("uname: {}", io::Error::last_os_error());
        return 1;
    }
    // SAFETY: reached only after `uname` returned 0 above, so the kernel has
    // written every field of `uts`, making it fully initialized.
    let uts = unsafe { uts.assume_init() };
    // SAFETY: `buf` is one of the fixed-size `[c_char; N]` fields of
    // `libc::utsname` (sysname/nodename/release/version/machine), which the
    // `uname(2)` syscall always NUL-terminates within the field's bounds —
    // the kernel enforces this by construction (e.g. hostnames/domainnames
    // are length-limited at `sethostname`/`setdomainname` time to leave room
    // for the terminator), so scanning from `buf.as_ptr()` for a NUL byte
    // never reads past the end of the array.
    let field = |buf: &[libc::c_char]| unsafe {
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    };
    let mut parts = Vec::new();
    if f.sys {
        parts.push(field(&uts.sysname));
    }
    if f.node {
        parts.push(field(&uts.nodename));
    }
    if f.release {
        parts.push(field(&uts.release));
    }
    if f.version {
        parts.push(field(&uts.version));
    }
    if f.machine {
        parts.push(field(&uts.machine));
    }
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{}", parts.join(" "));
    0
}
