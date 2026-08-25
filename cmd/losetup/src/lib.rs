//! user losetup — set up and control loop devices.
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;

use usercore::Ui;

// linux/loop.h — stable kernel ABI, not exposed by the `libc` crate.
const LOOP_SET_FD: libc::c_ulong = 0x4C00;
const LOOP_CLR_FD: libc::c_ulong = 0x4C01;
const LOOP_SET_STATUS64: libc::c_ulong = 0x4C04;
const LOOP_GET_STATUS64: libc::c_ulong = 0x4C05;
const LOOP_CTL_GET_FREE: libc::c_ulong = 0x4C82;
const LO_FLAGS_READ_ONLY: u32 = 1;
const LO_NAME_SIZE: usize = 64;
const LO_KEY_SIZE: usize = 32;

/// Mirrors the kernel's `struct loop_info64` (linux/loop.h).
#[repr(C)]
#[derive(Clone, Copy)]
struct LoopInfo64 {
    lo_device: u64,
    lo_inode: u64,
    lo_rdevice: u64,
    lo_offset: u64,
    lo_sizelimit: u64,
    lo_number: u32,
    lo_encrypt_type: u32,
    lo_encrypt_key_size: u32,
    lo_flags: u32,
    lo_file_name: [u8; LO_NAME_SIZE],
    lo_crypt_name: [u8; LO_NAME_SIZE],
    lo_encrypt_key: [u8; LO_KEY_SIZE],
    lo_init: [u64; 2],
}

impl Default for LoopInfo64 {
    fn default() -> Self {
        // SAFETY: an all-zero `loop_info64` is a valid value — the kernel
        // itself zero-initializes it before filling in queried fields.
        unsafe { std::mem::zeroed() }
    }
}

fn set_name(buf: &mut [u8; LO_NAME_SIZE], name: &str) {
    let bytes = name.as_bytes();
    let n = bytes.len().min(LO_NAME_SIZE - 1);
    buf[..n].copy_from_slice(&bytes[..n]);
}

/// Find a free loop device index via `/dev/loop-control`, without
/// attaching anything.
fn find_free() -> io::Result<i64> {
    let ctl = File::open("/dev/loop-control")?;
    // SAFETY: `LOOP_CTL_GET_FREE` takes no third argument; the ioctl's
    // return value (not an out-pointer) is the free index.
    let r = unsafe { libc::ioctl(ctl.as_raw_fd(), LOOP_CTL_GET_FREE as _) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r as i64)
    }
}

/// Attach `file` to `device` (e.g. `/dev/loop0`), with an optional byte
/// `offset` and `read_only` flag.
fn attach(device: &str, file: &str, offset: u64, read_only: bool) -> io::Result<()> {
    let mut file_opts = OpenOptions::new();
    file_opts.read(true).write(!read_only);
    let backing = file_opts.open(file)?;
    let loop_dev = OpenOptions::new()
        .read(true)
        .write(!read_only)
        .open(device)?;

    // SAFETY: `loop_dev` and `backing` are both valid, open file
    // descriptors kept alive for the call; `LOOP_SET_FD` takes the
    // backing fd directly as its third argument (not a pointer).
    let r = unsafe { libc::ioctl(loop_dev.as_raw_fd(), LOOP_SET_FD, backing.as_raw_fd()) };
    if r == -1 {
        return Err(io::Error::last_os_error());
    }

    let mut info = LoopInfo64 {
        lo_offset: offset,
        lo_flags: if read_only { LO_FLAGS_READ_ONLY } else { 0 },
        ..Default::default()
    };
    set_name(&mut info.lo_file_name, file);

    // SAFETY: `info` is a correctly-sized, fully-initialized
    // `loop_info64`; `loop_dev` stays open for the call.
    let r = unsafe { libc::ioctl(loop_dev.as_raw_fd(), LOOP_SET_STATUS64, &info) };
    if r == -1 {
        let save = io::Error::last_os_error();
        // SAFETY: undoing the SET_FD above on the same fd; takes no
        // pointer argument.
        unsafe { libc::ioctl(loop_dev.as_raw_fd(), LOOP_CLR_FD, 0) };
        return Err(save);
    }
    Ok(())
}

fn detach(device: &str) -> io::Result<()> {
    let loop_dev = OpenOptions::new().read(true).open(device)?;
    // SAFETY: `loop_dev` is open for the call; `LOOP_CLR_FD` takes no
    // pointer argument.
    let r = unsafe { libc::ioctl(loop_dev.as_raw_fd(), LOOP_CLR_FD, 0) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Query the backing file (if attached) of `device`, without needing
/// write access to it.
fn status(device: &str) -> io::Result<LoopInfo64> {
    let loop_dev = OpenOptions::new().read(true).open(device)?;
    let mut info = LoopInfo64::default();
    // SAFETY: `info` is a correctly-sized `loop_info64` out-param;
    // `loop_dev` stays open for the call.
    let r = unsafe { libc::ioctl(loop_dev.as_raw_fd(), LOOP_GET_STATUS64, &mut info) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(info)
    }
}

fn backing_file_name(info: &LoopInfo64) -> String {
    let end = info
        .lo_file_name
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(LO_NAME_SIZE);
    String::from_utf8_lossy(&info.lo_file_name[..end]).into_owned()
}

/// List every currently-attached loop device, by scanning `/sys/block/`
/// for `loop*` entries with a non-empty `loop/backing_file`.
fn list_active() -> io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir("/sys/block")?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("loop") {
            continue;
        }
        let backing_path = entry.path().join("loop/backing_file");
        if let Ok(backing) = fs::read_to_string(&backing_path) {
            out.push((format!("/dev/{name}"), backing.trim().to_string()));
        }
    }
    out.sort();
    Ok(out)
}

fn print_help() {
    print!(
        "Usage: losetup -f [--show] [FILE]\n\
 losetup [-r] [-o OFFSET] DEVICE FILE\n\
 losetup -d DEVICE\n\
 losetup -a\n\
 losetup DEVICE\n\
 -f, --find find the first unused device\n\
 -d, --detach DEVICE detach the device\n\
 -a, --all list all attached devices\n\
 -o, --offset NUM start at NUM bytes into the file\n\
 -r, --read-only attach read-only\n\
 --show print the resulting device after -f\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `losetup` utility. Parses `std::env::args()` and
/// attaches/detaches/queries/lists Linux loop devices via `/dev/loopN`
/// and `/dev/loop-control` ioctls.
///
/// Returns 0 on success, 1 on any usage or ioctl error.
pub fn run() -> i32 {
    let ui = Ui::new("losetup");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut find = false;
    let mut show = false;
    let mut all = false;
    let mut detach_dev: Option<String> = None;
    let mut offset: u64 = 0;
    let mut read_only = false;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("losetup (user_utils) 0.1.0");
                return 0;
            }
            "-f" | "--find" => find = true,
            "--show" => show = true,
            "-a" | "--all" => all = true,
            "-r" | "--read-only" => read_only = true,
            "-d" | "--detach" => {
                i += 1;
                match args.get(i) {
                    Some(v) => detach_dev = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 'd'");
                        return 1;
                    }
                }
            }
            "-o" | "--offset" => {
                i += 1;
                match args.get(i).and_then(|v| v.parse().ok()) {
                    Some(v) => offset = v,
                    None => {
                        ui.err("invalid or missing offset");
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    if let Some(dev) = detach_dev {
        return match detach(&dev) {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&format!("{dev}: {e}"));
                1
            }
        };
    }

    if all {
        return match list_active() {
            Ok(devices) => {
                for (dev, file) in devices {
                    println!("{dev}: []: ({file})");
                }
                0
            }
            Err(e) => {
                ui.err(&format!("{e}"));
                1
            }
        };
    }

    if find {
        let index = match find_free() {
            Ok(i) => i,
            Err(e) => {
                ui.err(&format!("{e}"));
                return 1;
            }
        };
        let dev = format!("/dev/loop{index}");
        match positional.first() {
            Some(file) => match attach(&dev, file, offset, read_only) {
                Ok(()) => {
                    if show {
                        println!("{dev}");
                    }
                    0
                }
                Err(e) => {
                    ui.err(&format!("{file}: {e}"));
                    1
                }
            },
            None => {
                println!("{dev}");
                0
            }
        }
    } else {
        match positional.len() {
            1 => match status(&positional[0]) {
                Ok(info) => {
                    println!(
                        "{}: [{}]: ({})",
                        positional[0],
                        info.lo_device,
                        backing_file_name(&info)
                    );
                    0
                }
                Err(e) => {
                    ui.err(&format!("{}: {e}", positional[0]));
                    1
                }
            },
            2 => match attach(&positional[0], &positional[1], offset, read_only) {
                Ok(()) => 0,
                Err(e) => {
                    ui.err(&format!("{}: {e}", positional[1]));
                    1
                }
            },
            _ => {
                print_help();
                1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_name_truncates_and_nul_terminates() {
        let mut buf = [0u8; LO_NAME_SIZE];
        set_name(&mut buf, "abc");
        assert_eq!(&buf[..3], b"abc");
        assert_eq!(buf[3], 0);

        let long = "x".repeat(100);
        let mut buf2 = [0u8; LO_NAME_SIZE];
        set_name(&mut buf2, &long);
        assert_eq!(buf2.len(), LO_NAME_SIZE);
        assert_eq!(buf2[LO_NAME_SIZE - 1], 0);
    }

    #[test]
    fn backing_file_name_reads_up_to_first_nul() {
        let mut info = LoopInfo64::default();
        set_name(&mut info.lo_file_name, "/tmp/disk.img");
        assert_eq!(backing_file_name(&info), "/tmp/disk.img");
    }

    #[test]
    fn find_free_returns_a_nonnegative_index() {
        // /dev/loop-control needs group `disk` (or root); unprivileged
        // CI/sandbox users legitimately get EACCES here, so only assert
        // the happy path when it actually opens.
        if let Ok(idx) = find_free() {
            assert!(idx >= 0);
        }
    }

    #[test]
    fn attach_missing_file_fails_cleanly() {
        let result = attach("/dev/loop0", "/nonexistent/user-losetup-missing", 0, false);
        assert!(result.is_err());
    }

    #[test]
    fn list_active_does_not_panic() {
        assert!(list_active().is_ok());
    }
}
