//! `BLKPG` ioctl — tell the running kernel about a partition without
//! touching on-disk data. Shared by `partx`/`addpart`/`delpart`/
//! `resizepart`. Structs mirror the kernel's `linux/blkpg.h`, which
//! `libc` doesn't expose.
use std::fs::File;
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;

const BLKPG: libc::c_ulong = 0x1269;
pub const BLKPG_ADD_PARTITION: libc::c_int = 1;
pub const BLKPG_DEL_PARTITION: libc::c_int = 2;
pub const BLKPG_RESIZE_PARTITION: libc::c_int = 3;

/// Mirrors the kernel's `struct blkpg_partition`.
#[repr(C)]
struct BlkpgPartition {
    start: i64,
    length: i64,
    pno: i32,
    devname: [u8; 64],
    volname: [u8; 64],
}

/// Mirrors the kernel's `struct blkpg_ioctl_arg`.
#[repr(C)]
struct BlkpgIoctlArg {
    op: libc::c_int,
    flags: libc::c_int,
    datalen: libc::c_int,
    data: *mut BlkpgPartition,
}

/// Tell the kernel about partition `pno` on `disk` (the whole-disk
/// device node): add, delete, or resize it, per `op`. `start_bytes`/
/// `length_bytes` are ignored for `BLKPG_DEL_PARTITION`.
pub fn notify(
    disk: &Path,
    op: libc::c_int,
    pno: u32,
    start_bytes: u64,
    length_bytes: u64,
) -> io::Result<()> {
    let f = File::open(disk)?;
    let mut bp = BlkpgPartition {
        start: start_bytes as i64,
        length: length_bytes as i64,
        pno: pno as i32,
        devname: [0u8; 64],
        volname: [0u8; 64],
    };
    let name = format!("{}{pno}", disk.display());
    let name_bytes = name.as_bytes();
    let n = name_bytes.len().min(63);
    bp.devname[..n].copy_from_slice(&name_bytes[..n]);

    let mut arg = BlkpgIoctlArg {
        op,
        flags: 0,
        datalen: std::mem::size_of::<BlkpgPartition>() as libc::c_int,
        data: &mut bp,
    };
    // SAFETY: `arg.data` points at `bp`, a live `BlkpgPartition` of
    // exactly `arg.datalen` bytes, for the duration of this call; `arg`
    // itself is a correctly-sized `blkpg_ioctl_arg` kept alive here.
    // `f` is the caller's open block-device descriptor.
    let r = unsafe { libc::ioctl(f.as_raw_fd(), BLKPG as _, &mut arg) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes_match_kernel_abi() {
        // linux/blkpg.h: blkpg_partition is 152 bytes on a 64-bit
        // target (natural C alignment); blkpg_ioctl_arg is 24 bytes. A
        // mismatch here means the ioctl reads/writes past what the
        // kernel expects.
        assert_eq!(std::mem::size_of::<BlkpgPartition>(), 152);
        assert_eq!(std::mem::size_of::<BlkpgIoctlArg>(), 24);
    }

    #[test]
    fn notify_on_a_non_block_device_fails_cleanly() {
        assert!(notify(Path::new("/dev/null"), BLKPG_ADD_PARTITION, 1, 0, 0).is_err());
    }
}
