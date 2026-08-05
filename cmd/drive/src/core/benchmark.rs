use anyhow::{Context, Result};
use colored::*;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::ui::display::{print_header, print_row, print_success, print_warning};

#[derive(Debug)]
struct BenchResult {
    seq_read_mbs: f64,
    seq_write_mbs: f64,
    rand_read_iops: f64,
    rand_write_iops: f64,
    rand_read_lat_us: f64,
    rand_write_lat_us: f64,
}

pub fn run_benchmark(target: &str, block_size_kib: u64, duration_secs: u64) -> Result<()> {
    print_header("I/O Performance Benchmark");
    println!();

    let block_bytes = block_size_kib * 1024;
    let duration = Duration::from_secs(duration_secs);

    // Determine whether target is a block device or a directory/file path
    let bench_file = resolve_bench_path(target)?;
    let is_device = Path::new(target).to_string_lossy().starts_with("/dev/");

    print_row("Target", &target.bright_blue().to_string());
    print_row(
        "Block size",
        &format!("{} KiB", block_size_kib)
            .bright_magenta()
            .to_string(),
    );
    print_row(
        "Test duration",
        &format!("{duration_secs}s per test")
            .bright_magenta()
            .to_string(),
    );
    if !is_device {
        print_warning("Testing on filesystem path — results reflect OS cache + underlying device");
    }
    println!();

    // -- Sequential Write --------------------------------------------------
    println!("  {} Sequential write...", "→".bright_cyan());
    let seq_write = measure_seq_write(&bench_file, block_bytes as usize, duration)
        .context("Sequential write test failed")?;

    // -- Sequential Read ---------------------------------------------------
    println!("  {} Sequential read...", "→".bright_cyan());
    let seq_read = measure_seq_read(&bench_file, block_bytes as usize, duration)
        .context("Sequential read test failed")?;

    // -- Random Write ------------------------------------------------------
    println!("  {} Random write (4K)...", "→".bright_cyan());
    let (rw_iops, rw_lat) =
        measure_rand_write(&bench_file, duration).context("Random write test failed")?;

    // -- Random Read -------------------------------------------------------
    println!("  {} Random read (4K)...", "→".bright_cyan());
    let (rr_iops, rr_lat) =
        measure_rand_read(&bench_file, duration).context("Random read test failed")?;

    // Clean up temp file if we created one
    if !is_device {
        let _ = std::fs::remove_file(&bench_file);
    }

    let result = BenchResult {
        seq_read_mbs: seq_read,
        seq_write_mbs: seq_write,
        rand_read_iops: rr_iops,
        rand_write_iops: rw_iops,
        rand_read_lat_us: rr_lat,
        rand_write_lat_us: rw_lat,
    };

    print_results(&result);
    Ok(())
}

// --- Measurements ------------------------------------------------------------

fn measure_seq_write(path: &str, block: usize, duration: Duration) -> Result<f64> {
    let buf = vec![0xA5u8; block];
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .context("Cannot open file for sequential write")?;

    let start = Instant::now();
    let mut total_bytes: u64 = 0;

    while start.elapsed() < duration {
        f.write_all(&buf)?;
        total_bytes += block as u64;
    }
    f.flush()?;
    // fsync to flush OS cache to device
    f.sync_all()?;

    let elapsed = start.elapsed().as_secs_f64();
    Ok(total_bytes as f64 / elapsed / 1_000_000.0)
}

fn measure_seq_read(path: &str, block: usize, duration: Duration) -> Result<f64> {
    let mut buf = vec![0u8; block];
    let mut f = File::open(path).context("Cannot open file for sequential read")?;
    let file_size = f.metadata()?.len();

    if file_size == 0 {
        return Ok(0.0);
    }

    let start = Instant::now();
    let mut total_bytes: u64 = 0;

    while start.elapsed() < duration {
        let n = f.read(&mut buf)?;
        if n == 0 {
            // Wrap around
            f.seek(SeekFrom::Start(0))?;
            continue;
        }
        total_bytes += n as u64;
    }

    let elapsed = start.elapsed().as_secs_f64();
    Ok(total_bytes as f64 / elapsed / 1_000_000.0)
}

fn measure_rand_write(path: &str, duration: Duration) -> Result<(f64, f64)> {
    const RAND_BLOCK: usize = 4096;
    let buf = vec![0xBBu8; RAND_BLOCK];

    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .context("Cannot open file for random write")?;

    let file_size = f.metadata()?.len().max(64 * 1024 * 1024); // at least 64 MiB
    let max_offset = file_size / RAND_BLOCK as u64;

    let start = Instant::now();
    let mut ops: u64 = 0;
    let mut seed: u64 = 0xDEAD_BEEF_CAFE_1234;

    while start.elapsed() < duration {
        seed = lcg_next(seed);
        let offset = (seed % max_offset) * RAND_BLOCK as u64;
        f.seek(SeekFrom::Start(offset))?;
        f.write_all(&buf)?;
        ops += 1;
    }
    f.flush()?;

    let elapsed = start.elapsed().as_secs_f64();
    let iops = ops as f64 / elapsed;
    let lat_us = elapsed * 1_000_000.0 / ops as f64;
    Ok((iops, lat_us))
}

fn measure_rand_read(path: &str, duration: Duration) -> Result<(f64, f64)> {
    const RAND_BLOCK: usize = 4096;
    let mut buf = vec![0u8; RAND_BLOCK];

    let mut f = File::open(path).context("Cannot open file for random read")?;
    let file_size = f.metadata()?.len();
    if file_size < RAND_BLOCK as u64 {
        return Ok((0.0, 0.0));
    }
    let max_offset = file_size / RAND_BLOCK as u64;

    let start = Instant::now();
    let mut ops: u64 = 0;
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;

    while start.elapsed() < duration {
        seed = lcg_next(seed);
        let offset = (seed % max_offset) * RAND_BLOCK as u64;
        f.seek(SeekFrom::Start(offset))?;
        f.read_exact(&mut buf)?;
        ops += 1;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let iops = ops as f64 / elapsed;
    let lat_us = elapsed * 1_000_000.0 / ops as f64;
    Ok((iops, lat_us))
}

// --- Output ------------------------------------------------------------------

fn print_results(r: &BenchResult) {
    println!();
    println!("  {:<28} {:<20} {}",
        "TEST".bold().cyan(),
        "RESULT".bold().cyan(),
        "RATING".bold().cyan(),
    );
    println!("  {}", "─".repeat(64).truecolor(50, 50, 60));

    print_bench_row(
        "Sequential Read",
        &format!("{:.0} MB/s", r.seq_read_mbs),
        rate_seq(r.seq_read_mbs),
    );
    print_bench_row(
        "Sequential Write",
        &format!("{:.0} MB/s", r.seq_write_mbs),
        rate_seq(r.seq_write_mbs),
    );
    print_bench_row(
        "Random Read (4K)",
        &format!(
            "{:.0} IOPS ({:.1} µs)",
            r.rand_read_iops, r.rand_read_lat_us
        ),
        rate_iops(r.rand_read_iops),
    );
    print_bench_row(
        "Random Write (4K)",
        &format!(
            "{:.0} IOPS ({:.1} µs)",
            r.rand_write_iops, r.rand_write_lat_us
        ),
        rate_iops(r.rand_write_iops),
    );

    println!();
    print_success("Benchmark complete");
    println!();
}

fn print_bench_row(label: &str, value: &str, rating: &str) {
    println!("  {:<28} {:<30} {}",
        label.truecolor(100, 220, 200),
        value.bright_magenta(),
        rating,
    );
}

fn rate_seq(mbs: f64) -> &'static str {
    if mbs >= 3000.0 {
        "● Excellent (NVMe)"
    } else if mbs >= 500.0 {
        "* Good (SATA SSD)"
    } else if mbs >= 100.0 {
        "● Fair (HDD)"
    } else {
        "* Slow"
    }
}

fn rate_iops(iops: f64) -> &'static str {
    if iops >= 100_000.0 {
        "● Excellent"
    } else if iops >= 10_000.0 {
        "* Good"
    } else if iops >= 1_000.0 {
        "● Fair"
    } else {
        "* Slow"
    }
}

// --- Helpers -----------------------------------------------------------------

/// Resolve the path to use for benchmarking.
/// - Block device: use directly
/// - Directory: create a temp file inside it
/// - Otherwise: use as-is (will be created)
fn resolve_bench_path(target: &str) -> Result<String> {
    let p = Path::new(target);
    if p.is_dir() {
        Ok(format!("{}/drive_bench.tmp", target.trim_end_matches('/')))
    } else {
        Ok(target.to_string())
    }
}

/// Simple linear congruential generator for reproducible pseudo-random offsets.
/// No external dep needed — we only need uniform distribution across sectors.
#[inline]
fn lcg_next(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

// --- Unit tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lcg_produces_distinct_values() {
        let mut v = lcg_next(1);
        let mut prev = v;
        for _ in 0..1000 {
            v = lcg_next(v);
            assert_ne!(v, prev, "LCG should not repeat consecutively");
            prev = v;
        }
    }

    #[test]
    fn resolve_bench_path_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        let result = resolve_bench_path(path).unwrap();
        assert!(result.ends_with("/drive_bench.tmp"));
    }

    #[test]
    fn resolve_bench_path_device_passthrough() {
        let result = resolve_bench_path("/dev/null").unwrap();
        assert_eq!(result, "/dev/null");
    }

    #[test]
    fn seq_write_and_read_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bench.tmp");
        let path_str = path.to_str().unwrap();

        let mbs = measure_seq_write(path_str, 4096, Duration::from_millis(200)).unwrap();
        assert!(mbs > 0.0, "Write MB/s should be positive, got {mbs}");

        let mbs_r = measure_seq_read(path_str, 4096, Duration::from_millis(200)).unwrap();
        assert!(mbs_r > 0.0, "Read MB/s should be positive, got {mbs_r}");
    }

    #[test]
    fn rand_write_and_read_produce_iops() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rand.tmp");
        let path_str = path.to_str().unwrap();

        // Pre-fill so rand read has data
        measure_seq_write(path_str, 4096, Duration::from_millis(100)).unwrap();

        let (iops, lat) = measure_rand_write(path_str, Duration::from_millis(200)).unwrap();
        assert!(iops > 0.0, "Random write IOPS should be positive");
        assert!(lat > 0.0, "Latency should be positive");

        let (iops_r, lat_r) = measure_rand_read(path_str, Duration::from_millis(200)).unwrap();
        assert!(iops_r > 0.0, "Random read IOPS should be positive");
        assert!(lat_r > 0.0, "Read latency should be positive");
    }

    #[test]
    fn rate_seq_bands_are_correct() {
        assert_eq!(rate_seq(4000.0), "● Excellent (NVMe)");
        assert_eq!(rate_seq(600.0), "* Good (SATA SSD)");
        assert_eq!(rate_seq(120.0), "● Fair (HDD)");
        assert_eq!(rate_seq(10.0), "* Slow");
    }

    #[test]
    fn rate_iops_bands_are_correct() {
        assert_eq!(rate_iops(200_000.0), "● Excellent");
        assert_eq!(rate_iops(50_000.0), "* Good");
        assert_eq!(rate_iops(5_000.0), "● Fair");
        assert_eq!(rate_iops(500.0), "* Slow");
    }
}
