use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crossbeam_channel::Sender;

use crate::{
    cli::{ReflinkMode, SparseMode},
    error::{io_err, CpError, Result},
    progress::ProgressEvent,
};

const COPY_CHUNK: usize = 64 * 1024 * 1024;
const BUF_CHUNK: usize = 1024 * 1024;

/// Strategy knobs that affect how a single file is copied.
#[derive(Debug, Clone, Copy)]
pub struct FileCopyOpts {
    pub reflink: ReflinkMode,
    pub sparse: SparseMode,
}

/// Copy a single regular file from `src` to `dest` atomically.
///
/// Writes to a hidden temporary file in `dest`'s parent directory, fsyncs
/// it, then renames it into place. Returns the number of bytes copied.
pub fn copy_file_atomic(
    src: &Path,
    dest: &Path,
    opts: FileCopyOpts,
    tx: &Sender<ProgressEvent>,
) -> Result<u64> {
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;

    let tmp = temp_path(parent, dest);
    let src_size = fs::metadata(src).map_err(|e| io_err(src, e))?.len();

    let _ = tx.send(ProgressEvent::FileStart {
        path: src.to_owned(),
        total: src_size,
    });

    let copied = match copy_into(src, &tmp, opts, src_size, tx) {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };

    match File::open(&tmp) {
        Ok(f) => {
            let _ = f.sync_all();
        }
        Err(_) => { /* best-effort: still attempt the rename below */ }
    }

    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(dest, e)
    })?;

    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }

    let _ = tx.send(ProgressEvent::FileDone {
        path: src.to_owned(),
        bytes: copied,
    });

    Ok(copied)
}

fn temp_path(parent: &Path, dest: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cp".to_owned());
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{name}.usercp.{}.{unique}.tmp",
        std::process::id()
    ))
}

fn copy_into(
    src: &Path,
    tmp: &Path,
    opts: FileCopyOpts,
    src_size: u64,
    tx: &Sender<ProgressEvent>,
) -> Result<u64> {
    if opts.reflink != ReflinkMode::Never {
        if try_ficlone(src, tmp).is_ok() {
            let _ = tx.send(ProgressEvent::Progress {
                path: src.to_owned(),
                bytes: src_size,
            });
            return Ok(src_size);
        }
        if opts.reflink == ReflinkMode::Always {
            return Err(CpError::ReflinkUnsupported(src.to_owned()));
        }
    }

    let use_sparse = match opts.sparse {
        SparseMode::Never => false,
        SparseMode::Always => true,
        SparseMode::Auto => file_has_holes(src)?,
    };

    if use_sparse {
        return copy_sparse(src, tmp, src_size, tx);
    }

    copy_file_range_loop(src, tmp, src_size, tx)
}

#[cfg(target_os = "linux")]
fn try_ficlone(src: &Path, dest: &Path) -> std::io::Result<()> {
    const FICLONE: libc::c_ulong = 0x4004_9409;

    let src_file = File::open(src)?;
    let dest_file = OpenOptions::new().write(true).create_new(true).open(dest)?;

    // SAFETY: `dest_file`/`src_file` are open files for the duration of this
    let ret = unsafe { libc::ioctl(dest_file.as_raw_fd(), FICLONE as _, src_file.as_raw_fd()) };

    if ret == -1 {
        let err = std::io::Error::last_os_error();
        drop(dest_file);
        let _ = std::fs::remove_file(dest);
        Err(err)
    } else {
        Ok(())
    }
}

#[cfg(target_os = "redox")]
fn try_ficlone(_src: &Path, _dest: &Path) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

#[cfg(target_os = "linux")]
fn file_has_holes(path: &Path) -> Result<bool> {
    let f = File::open(path).map_err(|e| io_err(path, e))?;
    let size = f.metadata().map_err(|e| io_err(path, e))?.len();
    if size == 0 {
        return Ok(false);
    }

    // SAFETY: `f` is an open file kept alive for this call, so its raw fd is
    let data_start = unsafe { libc::lseek(f.as_raw_fd(), 0, libc::SEEK_DATA) };
    if data_start < 0 {
        let err = std::io::Error::last_os_error();
        return Ok(err.raw_os_error() == Some(libc::ENXIO) && size > 0);
    }

    // SAFETY: same reasoning as above — `f`'s fd is valid, `SEEK_HOLE` is a
    let hole_start = unsafe { libc::lseek(f.as_raw_fd(), data_start, libc::SEEK_HOLE) };
    if hole_start < 0 {
        return Ok(false);
    }

    Ok(data_start > 0 || (hole_start as u64) < size)
}

#[cfg(target_os = "redox")]
fn file_has_holes(_path: &Path) -> Result<bool> {
    Ok(false)
}

#[cfg(target_os = "linux")]
fn copy_sparse(src: &Path, dest: &Path, src_size: u64, tx: &Sender<ProgressEvent>) -> Result<u64> {
    use libc::{SEEK_DATA, SEEK_HOLE};

    let mut src_file = File::open(src).map_err(|e| io_err(src, e))?;
    let mut dest_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(|e| io_err(dest, e))?;

    if src_size > 0 {
        dest_file
            .seek(SeekFrom::Start(src_size - 1))
            .and_then(|_| dest_file.write_all(&[0u8]))
            .map_err(|e| io_err(dest, e))?;
        dest_file
            .seek(SeekFrom::Start(0))
            .map_err(|e| io_err(dest, e))?;
    }

    let mut offset: i64 = 0;
    let mut total: u64 = 0;
    let mut buf = vec![0u8; BUF_CHUNK];

    loop {
        // SAFETY: `src_file`'s fd is valid for the call's duration; `offset`
        let data_start = unsafe { libc::lseek(src_file.as_raw_fd(), offset, SEEK_DATA) };
        if data_start < 0 {
            break;
        }

        // SAFETY: same reasoning — valid fd, plain offset, `SEEK_HOLE`
        let hole_start = unsafe { libc::lseek(src_file.as_raw_fd(), data_start, SEEK_HOLE) };
        let hole_start = if hole_start < 0 {
            src_size as i64
        } else {
            hole_start
        };

        src_file
            .seek(SeekFrom::Start(data_start as u64))
            .map_err(|e| io_err(src, e))?;
        dest_file
            .seek(SeekFrom::Start(data_start as u64))
            .map_err(|e| io_err(dest, e))?;

        let mut remaining = (hole_start - data_start).max(0) as u64;

        while remaining > 0 {
            let to_read = remaining.min(buf.len() as u64) as usize;
            let n = src_file
                .read(&mut buf[..to_read])
                .map_err(|e| io_err(src, e))?;
            if n == 0 {
                break;
            }
            dest_file
                .write_all(&buf[..n])
                .map_err(|e| io_err(dest, e))?;
            remaining -= n as u64;
            total += n as u64;

            let _ = tx.send(ProgressEvent::Progress {
                path: src.to_owned(),
                bytes: n as u64,
            });
        }

        offset = hole_start;
        if offset as u64 >= src_size {
            break;
        }
    }

    Ok(total)
}

#[cfg(target_os = "redox")]
fn copy_sparse(src: &Path, dest: &Path, src_size: u64, tx: &Sender<ProgressEvent>) -> Result<u64> {
    buffered_copy(src, dest, src_size, tx)
}

fn copy_file_range_loop(
    src: &Path,
    dest: &Path,
    src_size: u64,
    tx: &Sender<ProgressEvent>,
) -> Result<u64> {
    let src_file = File::open(src).map_err(|e| io_err(src, e))?;
    let dest_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(|e| io_err(dest, e))?;

    let mut src_off: i64 = 0;
    let mut dest_off: i64 = 0;
    let mut total: u64 = 0;

    loop {
        let remaining = src_size.saturating_sub(total) as usize;
        if remaining == 0 {
            break;
        }
        let chunk = remaining.min(COPY_CHUNK);

        // SAFETY: `src_file`/`dest_file` are open for the duration of this
        #[cfg(target_os = "linux")]
        let n = unsafe {
            libc::syscall(
                libc::SYS_copy_file_range,
                src_file.as_raw_fd(),
                &mut src_off as *mut i64,
                dest_file.as_raw_fd(),
                &mut dest_off as *mut i64,
                chunk,
                0u32,
            )
        };

        #[cfg(target_os = "redox")]
        let n: i64 = -1;

        match n {
            n if n > 0 => {
                total += n as u64;
                let _ = tx.send(ProgressEvent::Progress {
                    path: src.to_owned(),
                    bytes: n as u64,
                });
            }
            0 => break,
            _ => {
                drop(src_file);
                drop(dest_file);
                let _ = fs::remove_file(dest);
                return buffered_copy(src, dest, src_size, tx);
            }
        }
    }

    Ok(total)
}

fn buffered_copy(
    src: &Path,
    dest: &Path,
    src_size: u64,
    tx: &Sender<ProgressEvent>,
) -> Result<u64> {
    let mut src_file = File::open(src).map_err(|e| io_err(src, e))?;
    let mut dest_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest)
        .map_err(|e| io_err(dest, e))?;

    let mut buf = vec![0u8; BUF_CHUNK];
    let mut total: u64 = 0;

    loop {
        let n = src_file.read(&mut buf).map_err(|e| io_err(src, e))?;
        if n == 0 {
            break;
        }
        dest_file
            .write_all(&buf[..n])
            .map_err(|e| io_err(dest, e))?;
        total += n as u64;

        let _ = tx.send(ProgressEvent::Progress {
            path: src.to_owned(),
            bytes: n as u64,
        });

        if total >= src_size {
            break;
        }
    }

    Ok(total)
}
