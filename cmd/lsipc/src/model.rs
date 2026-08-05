//! Data model for System V IPC resources (shared memory, semaphores,
//! message queues), read from `/proc/sysvipc/*` and `/proc/sys/kernel/*`.
//! No syscall-based fallback is implemented for systems without
//! `/proc/sysvipc` mounted — that's an exotic configuration on any real
//! Linux/Zainium system (`CONFIG_SYSVIPC=y` + procfs is the default), and
//! the reference itself only falls back to raw `shmctl`/`semctl`/`msgctl`
//! `IPC_INFO`/`*_STAT` calls in that case.

use std::fs;

pub(crate) struct ShmEntry {
    pub(crate) key: i32,
    pub(crate) id: i32,
    pub(crate) perms: u32,
    pub(crate) size: u64,
    pub(crate) cpid: i32,
    pub(crate) lpid: i32,
    pub(crate) nattch: u64,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) cuid: u32,
    pub(crate) cgid: u32,
    pub(crate) atime: i64,
    pub(crate) dtime: i64,
    pub(crate) ctime: i64,
}

pub(crate) struct SemEntry {
    pub(crate) key: i32,
    pub(crate) id: i32,
    pub(crate) perms: u32,
    pub(crate) nsems: u64,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) cuid: u32,
    pub(crate) cgid: u32,
    pub(crate) otime: i64,
    pub(crate) ctime: i64,
}

pub(crate) struct MsgEntry {
    pub(crate) key: i32,
    pub(crate) id: i32,
    pub(crate) perms: u32,
    pub(crate) cbytes: u64,
    pub(crate) qnum: u64,
    pub(crate) lspid: i32,
    pub(crate) lrpid: i32,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) cuid: u32,
    pub(crate) cgid: u32,
    pub(crate) stime: i64,
    pub(crate) rtime: i64,
    pub(crate) ctime: i64,
}

/// One semaphore within a set, fetched live via `semctl(2)` — real syscalls,
/// only performed when a caller actually needs per-semaphore detail (the
/// `-i` pretty view), not on every listing (the reference eagerly fetches
/// these for every semaphore set it parses, even when never displayed;
/// `NSEMS` only needs the count, already present in `/proc/sysvipc/sem`).
pub(crate) struct SemElement {
    pub(crate) semnum: usize,
    pub(crate) val: i32,
    pub(crate) ncount: u64,
    pub(crate) zcount: u64,
    pub(crate) pid: i32,
}

fn parse_fields(line: &str, count: usize) -> Option<Vec<&str>> {
    let fields: Vec<&str> = line.split_ascii_whitespace().collect();
    if fields.len() < count {
        None
    } else {
        Some(fields)
    }
}

fn parse_octal(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 8).ok()
}

pub(crate) fn read_shm_table(id_filter: Option<i32>) -> Result<Vec<ShmEntry>, String> {
    let content = fs::read_to_string("/proc/sysvipc/shm")
        .map_err(|e| format!("cannot read /proc/sysvipc/shm: {e}"))?;
    let mut out = Vec::new();
    for line in content.lines().skip(1) {
        let Some(f) = parse_fields(line, 14) else {
            continue;
        };
        let Some(entry) = (|| -> Option<ShmEntry> {
            Some(ShmEntry {
                key: f[0].parse().ok()?,
                id: f[1].parse().ok()?,
                perms: parse_octal(f[2])?,
                size: f[3].parse().ok()?,
                cpid: f[4].parse().ok()?,
                lpid: f[5].parse().ok()?,
                nattch: f[6].parse().ok()?,
                uid: f[7].parse().ok()?,
                gid: f[8].parse().ok()?,
                cuid: f[9].parse().ok()?,
                cgid: f[10].parse().ok()?,
                atime: f[11].parse().ok()?,
                dtime: f[12].parse().ok()?,
                ctime: f[13].parse().ok()?,
            })
        })() else {
            continue;
        };
        if id_filter.map_or(true, |want| want == entry.id) {
            out.push(entry);
        }
    }
    Ok(out)
}

pub(crate) fn read_sem_table(id_filter: Option<i32>) -> Result<Vec<SemEntry>, String> {
    let content = fs::read_to_string("/proc/sysvipc/sem")
        .map_err(|e| format!("cannot read /proc/sysvipc/sem: {e}"))?;
    let mut out = Vec::new();
    for line in content.lines().skip(1) {
        let Some(f) = parse_fields(line, 10) else {
            continue;
        };
        let Some(entry) = (|| -> Option<SemEntry> {
            Some(SemEntry {
                key: f[0].parse().ok()?,
                id: f[1].parse().ok()?,
                perms: parse_octal(f[2])?,
                nsems: f[3].parse().ok()?,
                uid: f[4].parse().ok()?,
                gid: f[5].parse().ok()?,
                cuid: f[6].parse().ok()?,
                cgid: f[7].parse().ok()?,
                otime: f[8].parse().ok()?,
                ctime: f[9].parse().ok()?,
            })
        })() else {
            continue;
        };
        if id_filter.map_or(true, |want| want == entry.id) {
            out.push(entry);
        }
    }
    Ok(out)
}

pub(crate) fn read_msg_table(id_filter: Option<i32>) -> Result<Vec<MsgEntry>, String> {
    let content = fs::read_to_string("/proc/sysvipc/msg")
        .map_err(|e| format!("cannot read /proc/sysvipc/msg: {e}"))?;
    let mut out = Vec::new();
    for line in content.lines().skip(1) {
        let Some(f) = parse_fields(line, 14) else {
            continue;
        };
        let Some(entry) = (|| -> Option<MsgEntry> {
            Some(MsgEntry {
                key: f[0].parse().ok()?,
                id: f[1].parse().ok()?,
                perms: parse_octal(f[2])?,
                cbytes: f[3].parse().ok()?,
                qnum: f[4].parse().ok()?,
                lspid: f[5].parse().ok()?,
                lrpid: f[6].parse().ok()?,
                uid: f[7].parse().ok()?,
                gid: f[8].parse().ok()?,
                cuid: f[9].parse().ok()?,
                cgid: f[10].parse().ok()?,
                stime: f[11].parse().ok()?,
                rtime: f[12].parse().ok()?,
                ctime: f[13].parse().ok()?,
            })
        })() else {
            continue;
        };
        if id_filter.map_or(true, |want| want == entry.id) {
            out.push(entry);
        }
    }
    Ok(out)
}

/// Fetches live per-semaphore values via real `semctl(2)` calls — used only
/// for the `-i` detail view's `Elements:` sub-table.
pub(crate) fn fetch_sem_elements(semid: i32, nsems: u64) -> Vec<SemElement> {
    let mut out = Vec::new();
    for semnum in 0..nsems as usize {
        // SAFETY: `semid`/`semnum` are validated ids read straight back
        // from `/proc/sysvipc/sem`, which the kernel itself just produced;
        // `semctl` with these read-only commands takes no pointer argument
        // and returns the value (or -1) directly, so there is no buffer to
        // initialize or invalidate.
        let (val, ncount, zcount, pid) = unsafe {
            (
                libc::semctl(semid, semnum as i32, libc::GETVAL),
                libc::semctl(semid, semnum as i32, libc::GETNCNT),
                libc::semctl(semid, semnum as i32, libc::GETZCNT),
                libc::semctl(semid, semnum as i32, libc::GETPID),
            )
        };
        if val == -1 && ncount == -1 && zcount == -1 && pid == -1 {
            // The set was removed between listing and querying it; skip.
            continue;
        }
        out.push(SemElement {
            semnum,
            val,
            ncount: ncount.max(0) as u64,
            zcount: zcount.max(0) as u64,
            pid,
        });
    }
    out
}

pub(crate) struct ShmLimits {
    pub(crate) max: u64,
    pub(crate) min: u64,
    pub(crate) mni: u64,
    pub(crate) all: u64,
}

pub(crate) struct SemLimits {
    pub(crate) vmx: u64,
    pub(crate) mni: u64,
    pub(crate) msl: u64,
    pub(crate) mns: u64,
    pub(crate) opm: u64,
}

pub(crate) struct MsgLimits {
    pub(crate) mni: u64,
    pub(crate) mnb: u64,
    pub(crate) max: u64,
}

fn read_u64(path: &str) -> Result<u64, String> {
    fs::read_to_string(path)
        .map_err(|e| format!("cannot read {path}: {e}"))?
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("invalid data in {path}"))
}

pub(crate) fn read_shm_limits() -> Result<ShmLimits, String> {
    Ok(ShmLimits {
        max: read_u64("/proc/sys/kernel/shmmax")?,
        min: 1,
        mni: read_u64("/proc/sys/kernel/shmmni")?,
        all: read_u64("/proc/sys/kernel/shmall")?,
    })
}

pub(crate) fn read_msg_limits() -> Result<MsgLimits, String> {
    Ok(MsgLimits {
        mni: read_u64("/proc/sys/kernel/msgmni")?,
        mnb: read_u64("/proc/sys/kernel/msgmnb")?,
        max: read_u64("/proc/sys/kernel/msgmax")?,
    })
}

/// `/proc/sys/kernel/sem` is one line: `SEMMSL SEMMNS SEMOPM SEMMNI`.
/// `SEMVMX` (max value a semaphore can hold) isn't exposed there — it's a
/// compile-time kernel constant, `0x7fff`, matching the reference.
pub(crate) fn read_sem_limits() -> Result<SemLimits, String> {
    const SEMVMX: u64 = 0x7fff;
    let content = fs::read_to_string("/proc/sys/kernel/sem")
        .map_err(|e| format!("cannot read /proc/sys/kernel/sem: {e}"))?;
    let mut fields = content.split_whitespace();
    let msl = fields
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "invalid data in /proc/sys/kernel/sem".to_string())?;
    let mns = fields
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "invalid data in /proc/sys/kernel/sem".to_string())?;
    let opm = fields
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "invalid data in /proc/sys/kernel/sem".to_string())?;
    let mni = fields
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "invalid data in /proc/sys/kernel/sem".to_string())?;
    Ok(SemLimits {
        vmx: SEMVMX,
        mni,
        msl,
        mns,
        opm,
    })
}
