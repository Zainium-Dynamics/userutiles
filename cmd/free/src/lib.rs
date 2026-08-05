//! user free — display free and used memory from /proc/meminfo.
use std::fs;
use std::io::{self, Write};

use usercore::Ui;

/// Entry point for the `free` utility. Parses `std::env::args()` and
/// prints a table of total/used/free/shared/buff-cache/available memory
/// and swap, read from `/proc/meminfo`.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("free");
    let mut human = false;
    let mut bytes = false;
    let mut wide = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" => {
                print!("Usage: free [OPTION]\nDisplay free and used memory.\n -h, --human human readable\n -b, --bytes show bytes\n -w, --wide wide output\n");
                return 0;
            }
            "--version" => {
                println!("free (user_utils) 0.1.0");
                return 0;
            }
            "-h" | "--human" => human = true,
            "-b" | "--bytes" => bytes = true,
            "-w" | "--wide" => wide = true,
            s if s.starts_with('-') => {
                for c in s.chars().skip(1) {
                    match c {
                        'h' => human = true,
                        'b' => bytes = true,
                        'w' => wide = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let info = match read_meminfo() {
        Ok(i) => i,
        Err(e) => {
            ui.err(&format!("{e}"));
            return 1;
        }
    };
    let unit = if bytes { 1u64 } else { 1024 }; // default KiB like free
    let fmt = |n: u64| {
        let v = n / unit;
        if human {
            humanize(n)
        } else {
            format!("{v:>12}")
        }
    };
    let mut out = io::stdout().lock();
    if wide {
        let _ = writeln!(out, " total used free shared buffers cache available");
    } else {
        let _ = writeln!(out, " total used free shared buff/cache available");
    }
    let used = info
        .total
        .saturating_sub(info.free + info.buffers + info.cached);
    let buff_cache = info.buffers + info.cached;
    if wide {
        let _ = writeln!(
            out,
            "Mem: {} {} {} {} {} {} {}",
            fmt(info.total),
            fmt(used),
            fmt(info.free),
            fmt(info.shared),
            fmt(info.buffers),
            fmt(info.cached),
            fmt(info.available)
        );
    } else {
        let _ = writeln!(
            out,
            "Mem: {} {} {} {} {} {}",
            fmt(info.total),
            fmt(used),
            fmt(info.free),
            fmt(info.shared),
            fmt(buff_cache),
            fmt(info.available)
        );
    }
    let _ = writeln!(
        out,
        "Swap: {} {} {}",
        fmt(info.swap_total),
        fmt(info.swap_total.saturating_sub(info.swap_free)),
        fmt(info.swap_free)
    );
    0
}

/// Fields of `/proc/meminfo` that `free` reports, all in bytes.
struct MemInfo {
    total: u64,
    free: u64,
    available: u64,
    shared: u64,
    buffers: u64,
    cached: u64,
    swap_total: u64,
    swap_free: u64,
}

/// Read and parse `/proc/meminfo`.
fn read_meminfo() -> io::Result<MemInfo> {
    let s = fs::read_to_string("/proc/meminfo")?;
    Ok(parse_meminfo(&s))
}

/// Parse `/proc/meminfo`-format text (`KEY:` followed by a value in KiB,
/// one per line) into a [`MemInfo`], converting values to bytes. Unknown
/// keys are ignored; a missing or unparseable value for a known key is
/// treated as 0.
fn parse_meminfo(s: &str) -> MemInfo {
    let mut m = MemInfo {
        total: 0,
        free: 0,
        available: 0,
        shared: 0,
        buffers: 0,
        cached: 0,
        swap_total: 0,
        swap_free: 0,
    };
    for line in s.lines() {
        let mut it = line.split_whitespace();
        let key = it.next().unwrap_or("");
        let val: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0) * 1024; // kB -> bytes
        match key {
            "MemTotal:" => m.total = val,
            "MemFree:" => m.free = val,
            "MemAvailable:" => m.available = val,
            "Shmem:" => m.shared = val,
            "Buffers:" => m.buffers = val,
            "Cached:" => m.cached = val,
            "SwapTotal:" => m.swap_total = val,
            "SwapFree:" => m.swap_free = val,
            _ => {}
        }
    }
    m
}

/// Format a byte count as a human-readable size using binary (1024-based)
/// single-letter units, right-padded to match the fixed-width numeric
/// columns `free` otherwise prints.
fn humanize(n: u64) -> String {
    const U: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n:>9}")
    } else {
        format!("{v:>8.1}{}", U[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_meminfo_reads_known_keys() {
        let data = "MemTotal:       16384000 kB\nMemFree:         1024000 kB\nMemAvailable:    8192000 kB\nShmem:                 0 kB\nBuffers:          512000 kB\nCached:          2048000 kB\nSwapTotal:       2000000 kB\nSwapFree:        1000000 kB\n";
        let m = parse_meminfo(data);
        assert_eq!(m.total, 16384000 * 1024);
        assert_eq!(m.free, 1024000 * 1024);
        assert_eq!(m.available, 8192000 * 1024);
        assert_eq!(m.buffers, 512000 * 1024);
        assert_eq!(m.cached, 2048000 * 1024);
        assert_eq!(m.swap_total, 2000000 * 1024);
        assert_eq!(m.swap_free, 1000000 * 1024);
    }

    #[test]
    fn parse_meminfo_ignores_unknown_keys() {
        let m = parse_meminfo("Bogus:  123 kB\nMemTotal: 1 kB\n");
        assert_eq!(m.total, 1024);
    }

    #[test]
    fn parse_meminfo_empty_input_is_all_zero() {
        let m = parse_meminfo("");
        assert_eq!(m.total, 0);
        assert_eq!(m.free, 0);
    }

    #[test]
    fn parse_meminfo_missing_value_defaults_to_zero() {
        let m = parse_meminfo("MemTotal:\n");
        assert_eq!(m.total, 0);
    }

    #[test]
    fn humanize_formats_units() {
        assert_eq!(humanize(0).trim(), "0");
        assert_eq!(humanize(1024).trim(), "1.0K");
        assert_eq!(humanize(1024 * 1024).trim(), "1.0M");
    }
}
