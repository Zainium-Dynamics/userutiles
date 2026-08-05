// io_bench.rs — I/O performance benchmarks
// Run with: cargo bench

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::time::Instant;

const BLOCK_SIZE: usize = 4 * 1024 * 1024; // 4 MiB
const BLOCKS: usize = 256; // 1 GiB total

fn main() {
    println!("drive I/O Benchmark\n");

    let buf = vec![0xABu8; BLOCK_SIZE];
    let path = "/tmp/drive_bench_tmp";

    // ─── Sequential Write ─────────────────────────────────────────────────
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .expect("Cannot create temp file for benchmark");

    let start = Instant::now();
    for _ in 0..BLOCKS {
        f.write_all(&buf).expect("Write failed");
    }
    f.flush().expect("Flush failed");
    drop(f);
    let elapsed = start.elapsed().as_secs_f64();
    let mb = (BLOCK_SIZE * BLOCKS) as f64 / 1_000_000.0;
    println!("Sequential Write : {:.0} MB/s", mb / elapsed);

    // ─── Sequential Read ──────────────────────────────────────────────────
    let mut f = OpenOptions::new()
        .read(true)
        .open(path)
        .expect("Cannot open temp file for benchmark");

    let mut rbuf = vec![0u8; BLOCK_SIZE];
    let start = Instant::now();
    for _ in 0..BLOCKS {
        f.read_exact(&mut rbuf).expect("Read failed");
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("Sequential Read : {:.0} MB/s", mb / elapsed);

    let _ = std::fs::remove_file(path);
}
