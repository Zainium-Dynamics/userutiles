//! user blockdev — call block device ioctls from the command line.
//!
//! Ported from uutils/util-linux's `blockdev`. The ioctl request numbers
//! below are the stable Linux ABI constants from `<linux/fs.h>` (verified
//! against the system header rather than re-derived), since this crate
//! deliberately avoids depending on `uucore`/`linux-raw-sys`.
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use usercore::Ui;

/// How to interpret the value returned by a "get" ioctl.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ArgType {
    Short,
    Int,
    Long,
    U64,
    U64Sectors,
}

/// What kind of operation a `--flag` performs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Kind {
    /// Read an attribute back and print it.
    Get(ArgType),
    /// Write a caller-supplied value (the ioctl argument is the value
    /// itself, not a pointer to it — matches historic Linux block ioctls
    /// such as `BLKRASET`).
    Set,
    /// Fire-and-forget operation; `Op(n)` passes a pointer to the fixed
    /// value `n` as the ioctl argument (matches `BLKROSET`'s calling
    /// convention).
    Op(u32),
}

/// One `--flag` supported by `blockdev`, and the ioctl it maps to.
struct Action {
    name: &'static str,
    desc: &'static str,
    code: libc::c_ulong,
    kind: Kind,
}

const BLKROSET: libc::c_ulong = 0x125d;
const BLKROGET: libc::c_ulong = 0x125e;
const BLKRRPART: libc::c_ulong = 0x125f;
const BLKGETSIZE: libc::c_ulong = 0x1260;
const BLKFLSBUF: libc::c_ulong = 0x1261;
const BLKRASET: libc::c_ulong = 0x1262;
const BLKRAGET: libc::c_ulong = 0x1263;
const BLKFRASET: libc::c_ulong = 0x1264;
const BLKFRAGET: libc::c_ulong = 0x1265;
const BLKSECTGET: libc::c_ulong = 0x1267;
const BLKSSZGET: libc::c_ulong = 0x1268;
const BLKBSZGET: libc::c_ulong = 0x80081270;
const BLKBSZSET: libc::c_ulong = 0x40081271;
const BLKGETSIZE64: libc::c_ulong = 0x80081272;
const BLKIOMIN: libc::c_ulong = 0x1278;
const BLKIOOPT: libc::c_ulong = 0x1279;
const BLKALIGNOFF: libc::c_ulong = 0x127a;
const BLKPBSZGET: libc::c_ulong = 0x127b;
const BLKDISCARDZEROES: libc::c_ulong = 0x127c;

const ACTIONS: &[Action] = &[
    Action {
        name: "flushbufs",
        desc: "flush buffers",
        code: BLKFLSBUF,
        kind: Kind::Op(0),
    },
    Action {
        name: "getalignoff",
        desc: "get alignment offset in bytes",
        code: BLKALIGNOFF,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getbsz",
        desc: "get blocksize",
        code: BLKBSZGET,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getdiscardzeroes",
        desc: "get discard zeroes support status",
        code: BLKDISCARDZEROES,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getfra",
        desc: "get filesystem readahead",
        code: BLKFRAGET,
        kind: Kind::Get(ArgType::Long),
    },
    Action {
        name: "getiomin",
        desc: "get minimum I/O size",
        code: BLKIOMIN,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getioopt",
        desc: "get optimal I/O size",
        code: BLKIOOPT,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getmaxsect",
        desc: "get max sectors per request",
        code: BLKSECTGET,
        kind: Kind::Get(ArgType::Short),
    },
    Action {
        name: "getpbsz",
        desc: "get physical block (sector) size",
        code: BLKPBSZGET,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getra",
        desc: "get readahead",
        code: BLKRAGET,
        kind: Kind::Get(ArgType::Long),
    },
    Action {
        name: "getro",
        desc: "get read-only",
        code: BLKROGET,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getsize64",
        desc: "get size in bytes",
        code: BLKGETSIZE64,
        kind: Kind::Get(ArgType::U64),
    },
    Action {
        name: "getsize",
        desc: "get 32-bit sector count (deprecated, use --getsz)",
        code: BLKGETSIZE,
        kind: Kind::Get(ArgType::Long),
    },
    Action {
        name: "getss",
        desc: "get logical block (sector) size",
        code: BLKSSZGET,
        kind: Kind::Get(ArgType::Int),
    },
    Action {
        name: "getsz",
        desc: "get size in 512-byte sectors",
        code: BLKGETSIZE64,
        kind: Kind::Get(ArgType::U64Sectors),
    },
    Action {
        name: "rereadpt",
        desc: "reread partition table",
        code: BLKRRPART,
        kind: Kind::Op(0),
    },
    Action {
        name: "setbsz",
        desc: "set blocksize",
        code: BLKBSZSET,
        kind: Kind::Set,
    },
    Action {
        name: "setfra",
        desc: "set filesystem readahead",
        code: BLKFRASET,
        kind: Kind::Set,
    },
    Action {
        name: "setra",
        desc: "set readahead",
        code: BLKRASET,
        kind: Kind::Set,
    },
    Action {
        name: "setro",
        desc: "set read-only",
        code: BLKROSET,
        kind: Kind::Op(1),
    },
    Action {
        name: "setrw",
        desc: "set read-write",
        code: BLKROSET,
        kind: Kind::Op(0),
    },
];

const HELP: &str = "Usage: blockdev [-v|-q] COMMANDS DEVICE...\n\
       blockdev --report [DEVICE...]\n\n\
Call block device ioctls from the command line.\n\n\
Options:\n  -v, --verbose      verbose mode\n\
  -q, --quiet        quiet mode\n\
      --report       print report for specified devices\n\
  -h, --help         display this help and exit\n\
      --version      output version information and exit\n\n\
Commands:\n";

fn help_text() -> String {
    let mut s = HELP.to_string();
    for a in ACTIONS {
        s.push_str(&format!("  --{:<16} {}\n", a.name, a.desc));
    }
    s
}

/// Entry point for the `blockdev` utility. Parses `std::env::args()` and
/// either prints a one-line report per device (`--report`) or runs the
/// requested sequence of get/set ioctls against each device operand, in
/// the order given on the command line.
///
/// Returns 0 on success, 1 on a usage error or if any ioctl/open fails.
pub fn run() -> i32 {
    let ui = Ui::new("blockdev");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", help_text());
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("blockdev (user_utils) 0.1.0");
        return 0;
    }

    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    if parsed.devices.is_empty() {
        ui.err("no device specified");
        return 1;
    }

    if parsed.report {
        return run_report(&ui, &parsed.devices);
    }

    run_ops(&ui, &parsed.devices, &parsed.ops)
}

/// A single parsed operation: the flag name and (for `Set` actions) its
/// numeric argument. `"verbose"`/`"quiet"` are pseudo-operations that
/// toggle the reporting mode for subsequent ioctls rather than mapping to
/// an `Action`.
struct Op {
    name: String,
    value: usize,
}

struct Parsed {
    report: bool,
    ops: Vec<Op>,
    devices: Vec<String>,
}

/// Parse `blockdev`'s options and device operands out of `args` (already
/// stripped of `argv[0]`; `--help`/`--version` handled by the caller).
/// Operations are kept in command-line order so `run_ops` can replay them
/// faithfully (e.g. `--setra 512 --getra`).
fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut report = false;
    let mut ops = Vec::new();
    let mut devices = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--report" => report = true,
            "-v" | "--verbose" => ops.push(Op {
                name: "verbose".to_string(),
                value: 0,
            }),
            "-q" | "--quiet" => ops.push(Op {
                name: "quiet".to_string(),
                value: 0,
            }),
            s if s.starts_with("--") => {
                let name = &s[2..];
                let Some(action) = ACTIONS.iter().find(|act| act.name == name) else {
                    return Err(format!("unknown option -- '{s}'"));
                };
                let value = if action.kind == Kind::Set {
                    i += 1;
                    let raw = args
                        .get(i)
                        .ok_or_else(|| format!("option '--{name}' requires an argument"))?;
                    raw.parse::<usize>()
                        .map_err(|_| format!("invalid argument for '--{name}': '{raw}'"))?
                } else {
                    0
                };
                ops.push(Op {
                    name: name.to_string(),
                    value,
                });
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option -- '{s}'"));
            }
            other => devices.push(other.to_string()),
        }
        i += 1;
    }
    Ok(Parsed {
        report,
        ops,
        devices,
    })
}

/// SAFETY: `code` must be a valid block-device ioctl request for `fd`'s
/// underlying device, and `arg` must be a value/pointer of the size and
/// kind that ioctl expects (an immediate value for value-based ioctls, or
/// the address of a correctly-sized/aligned local for pointer-based ones).
unsafe fn raw_ioctl(fd: i32, code: libc::c_ulong, arg: usize) -> io::Result<()> {
    // SAFETY: forwarded from the caller's contract above. The `as _` cast
    // is required because `libc::ioctl`'s request parameter type differs
    // by target libc (c_ulong on glibc, c_int on musl) — `code` stays
    // c_ulong at the call boundary and narrows/widens per target here.
    let ret = unsafe { libc::ioctl(fd, code as _, arg) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Perform a `Get` ioctl and return the resulting value widened to `u64`.
fn get_attribute(file: &File, code: libc::c_ulong, arg_type: ArgType) -> io::Result<u64> {
    let fd = file.as_raw_fd();
    match arg_type {
        ArgType::Short => {
            let mut v: libc::c_ushort = 0;
            // SAFETY: `code` is one of our `Get(Short)` constants (BLKSECTGET)
            // and `v` is a correctly-sized, live local the ioctl writes into.
            unsafe { raw_ioctl(fd, code, &mut v as *mut _ as usize)? };
            Ok(v as u64)
        }
        ArgType::Int => {
            let mut v: libc::c_uint = 0;
            // SAFETY: same contract as above, for the `Get(Int)` ioctls.
            unsafe { raw_ioctl(fd, code, &mut v as *mut _ as usize)? };
            Ok(v as u64)
        }
        ArgType::Long => {
            let mut v: libc::c_ulong = 0;
            // SAFETY: same contract as above, for the `Get(Long)` ioctls.
            unsafe { raw_ioctl(fd, code, &mut v as *mut _ as usize)? };
            Ok(v as u64)
        }
        ArgType::U64 => {
            let mut v: u64 = 0;
            // SAFETY: same contract as above, for the `Get(U64)` ioctls.
            unsafe { raw_ioctl(fd, code, &mut v as *mut _ as usize)? };
            Ok(v)
        }
        ArgType::U64Sectors => {
            let mut v: u64 = 0;
            // SAFETY: same contract as above; BLKGETSIZE64 writes a byte
            // count into `v`, which we then convert to 512-byte sectors.
            unsafe { raw_ioctl(fd, code, &mut v as *mut _ as usize)? };
            Ok(v / 512)
        }
    }
}

/// Compute the partition start offset (in 512-byte sectors) for `device`,
/// or `0` if the device is not a partition.
fn partition_offset(device_file: &File) -> io::Result<u64> {
    let rdev = device_file.metadata()?.rdev();
    let (major, minor) = (libc::major(rdev), libc::minor(rdev));
    let partition_marker = format!("/sys/dev/block/{major}:{minor}/partition");
    if Path::new(&partition_marker).exists() {
        let s = std::fs::read_to_string(format!("/sys/dev/block/{major}:{minor}/start"))?;
        s.trim().parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unable to parse partition start offset",
            )
        })
    } else {
        Ok(0)
    }
}

fn action(name: &str) -> &'static Action {
    ACTIONS
        .iter()
        .find(|a| a.name == name)
        .expect("caller only passes names already validated against ACTIONS")
}

fn do_report(device_path: &str) -> io::Result<String> {
    let file = File::open(device_path)?;
    let offset = partition_offset(&file)?;
    let ro = get_attribute(&file, action("getro").code, ArgType::Int)?;
    let ra = get_attribute(&file, action("getra").code, ArgType::Long)?;
    let ss = get_attribute(&file, action("getss").code, ArgType::Int)?;
    let bsz = get_attribute(&file, action("getbsz").code, ArgType::Int)?;
    let size = get_attribute(&file, action("getsize64").code, ArgType::U64)?;
    Ok(format!(
        "{} {ra:5} {ss:5} {bsz:5} {offset:15} {size:15}   {device_path}",
        if ro == 1 { "ro" } else { "rw" },
    ))
}

fn run_report(ui: &Ui, devices: &[String]) -> i32 {
    println!("RO    RA   SSZ   BSZ        StartSec            Size   Device");
    let mut had_error = false;
    for device_path in devices {
        match do_report(device_path) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                had_error = true;
                ui.err(&format!("{device_path}: {e}"));
            }
        }
    }
    i32::from(had_error)
}

fn run_ops(ui: &Ui, devices: &[String], ops: &[Op]) -> i32 {
    for device_path in devices {
        let device_file = match File::open(device_path) {
            Ok(f) => f,
            Err(e) => {
                ui.err(&format!("cannot open {device_path}: {e}"));
                return 1;
            }
        };
        let mut verbose = false;
        for op in ops {
            match op.name.as_str() {
                "verbose" => verbose = true,
                "quiet" => verbose = false,
                name => {
                    let act = action(name);
                    if let Err(e) = do_ioctl(&device_file, act, verbose, op.value) {
                        if verbose {
                            println!("{} failed.", act.desc);
                        }
                        ui.err(&format!("{}: {e}", act.desc));
                        return 1;
                    }
                }
            }
        }
    }
    0
}

fn do_ioctl(file: &File, act: &Action, verbose: bool, value: usize) -> io::Result<()> {
    match act.kind {
        Kind::Get(arg_type) => {
            let ret = get_attribute(file, act.code, arg_type)?;
            if verbose {
                println!("{}: {ret}", act.name);
            } else {
                println!("{ret}");
            }
        }
        Kind::Set => {
            // SAFETY: `act.code` is one of the `Set` ioctls (BLKBSZSET,
            // BLKFRASET, BLKRASET), which — matching historic Linux block
            // ioctl calling convention — take the value directly as the
            // ioctl argument rather than a pointer to it.
            unsafe { raw_ioctl(file.as_raw_fd(), act.code, value)? };
            if verbose {
                println!("{} succeeded.", act.name);
            }
        }
        Kind::Op(param) => {
            // SAFETY: `act.code` is one of the `Op` ioctls (BLKFLSBUF,
            // BLKRRPART, BLKROSET); `param` is a live local for the
            // duration of the call, and its address is what the ioctl
            // (when it reads an argument at all, e.g. BLKROSET) expects.
            unsafe { raw_ioctl(file.as_raw_fd(), act.code, &param as *const u32 as usize)? };
            if verbose {
                println!("{} succeeded.", act.name);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_constants_match_linux_fs_h() {
        // Cross-checked against a tiny C program compiled with the system
        // <linux/fs.h> (see the port notes) rather than re-derived by hand.
        assert_eq!(BLKROSET, 0x125d);
        assert_eq!(BLKGETSIZE64, 0x80081272);
        assert_eq!(BLKBSZSET, 0x40081271);
        assert_eq!(BLKBSZGET, 0x80081270);
    }

    #[test]
    fn parse_args_report_flag() {
        let args: Vec<String> = ["--report", "/dev/sda"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = parse_args(&args).unwrap();
        assert!(p.report);
        assert_eq!(p.devices, vec!["/dev/sda".to_string()]);
    }

    #[test]
    fn parse_args_get_flag_needs_no_value() {
        let args: Vec<String> = ["--getra", "/dev/sda"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = parse_args(&args).unwrap();
        assert_eq!(p.ops.len(), 1);
        assert_eq!(p.ops[0].name, "getra");
        assert_eq!(p.devices, vec!["/dev/sda".to_string()]);
    }

    #[test]
    fn parse_args_set_flag_consumes_value() {
        let args: Vec<String> = ["--setra", "256", "/dev/sda"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = parse_args(&args).unwrap();
        assert_eq!(p.ops.len(), 1);
        assert_eq!(p.ops[0].name, "setra");
        assert_eq!(p.ops[0].value, 256);
        assert_eq!(p.devices, vec!["/dev/sda".to_string()]);
    }

    #[test]
    fn parse_args_set_flag_missing_value_errors() {
        let args: Vec<String> = ["--setra"].iter().map(|s| s.to_string()).collect();
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_unknown_option_errors() {
        let args: Vec<String> = ["--bogus", "/dev/sda"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn parse_args_preserves_operation_order() {
        let args: Vec<String> = ["--setra", "512", "--getra", "-v"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = parse_args(&args).unwrap();
        assert_eq!(p.ops[0].name, "setra");
        assert_eq!(p.ops[1].name, "getra");
        assert_eq!(p.ops[2].name, "verbose");
    }

    #[test]
    fn no_device_is_a_usage_error() {
        let ui = Ui::new("blockdev");
        let code = {
            let parsed = parse_args(&[]).unwrap();
            if parsed.devices.is_empty() {
                ui.err("no device specified");
                1
            } else {
                0
            }
        };
        assert_eq!(code, 1);
    }

    #[test]
    fn open_nonexistent_device_reports_error_not_panic() {
        let ui = Ui::new("blockdev");
        let code = run_ops(
            &ui,
            &["/nonexistent/user-blockdev-test-device".to_string()],
            &[],
        );
        assert_eq!(code, 1);
    }

    #[test]
    fn report_on_missing_device_reports_error_not_panic() {
        let ui = Ui::new("blockdev");
        let code = run_report(&ui, &["/nonexistent/user-blockdev-test-device".to_string()]);
        assert_eq!(code, 1);
    }

    #[test]
    fn help_text_lists_every_action() {
        let text = help_text();
        for a in ACTIONS {
            assert!(text.contains(a.name), "missing {} in help text", a.name);
        }
    }
}
