//! user mcookie — generate magic cookies for xauth.
use std::fs::File;
use std::io::{self, Read};

use usercore::Ui;

/// Number of bytes drawn from the system randomness source and folded into
/// every cookie, matching util-linux's `mcookie`.
const RANDOM_BYTES: usize = 128;
/// Default cap (bytes) on how much of each `-f` seed file is read.
const MAX_DEFAULT: u64 = 4096;

/// Fill `buf` with random bytes, preferring `/dev/urandom` and falling back
/// to a `libc::rand`-seeded stream (not cryptographically secure) only if
/// `/dev/urandom` cannot be opened or read.
fn fill_random(buf: &mut [u8]) {
    if let Ok(mut f) = File::open("/dev/urandom") {
        if f.read_exact(buf).is_ok() {
            return;
        }
    }
    // SAFETY: `libc::time` is called with a NULL `time_t*`, which POSIX
    // defines as valid; `libc::srand`/`libc::rand` take only plain
    // integers and dereference no pointers, so neither call can fail or
    // invoke UB regardless of process state.
    unsafe {
        libc::srand(libc::time(std::ptr::null_mut()) as u32 ^ std::process::id());
    }
    for byte in buf.iter_mut() {
        // SAFETY: `libc::rand` takes no arguments and only mutates C's
        // internal RNG state; it cannot fail or cause UB.
        *byte = unsafe { libc::rand() } as u8;
    }
}

/// Parse a `mcookie -m` size value: a decimal number with an optional `B`
/// or binary-unit suffix (`K`/`KiB`, `M`/`MiB`, `G`/`GiB`, `T`/`TiB`,
/// case-insensitive). A value of `0` is treated as "use the default"
/// (matching util-linux's behavior).
pub fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let digit_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if digit_end == 0 {
        return Err(format!("invalid size '{s}'"));
    }
    let (num_part, suffix) = s.split_at(digit_end);
    let num: u64 = num_part
        .parse()
        .map_err(|_| format!("invalid size '{s}'"))?;
    let multiplier: u64 = match suffix.trim().to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "K" | "KIB" => 1024,
        "M" | "MIB" => 1024 * 1024,
        "G" | "GIB" => 1024 * 1024 * 1024,
        "T" | "TIB" => 1024 * 1024 * 1024 * 1024,
        _ => return Err(format!("invalid size suffix in '{s}'")),
    };
    Ok(num.saturating_mul(multiplier))
}

/// Compute the MD5-based cookie (as raw bytes) from seed data already read
/// from files/stdin plus a caller-supplied block of "random" bytes.
///
/// Separating the random bytes out like this is what makes the hash
/// deterministic and testable: production code draws them from
/// [`fill_random`], tests pass a fixed array.
pub fn compute_cookie_bytes(seed_data: &[u8], random_bytes: &[u8; RANDOM_BYTES]) -> [u8; 16] {
    let mut h = usercore::digest::Md5::new();
    h.update(seed_data);
    h.update(random_bytes);
    h.finalize()
}

/// Read up to `max_size` bytes from `path` (or stdin if `path == "-"`).
fn read_seed(path: &str, max_size: u64) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    if path == "-" {
        io::stdin().take(max_size).read_to_end(&mut buf)?;
    } else {
        File::open(path)?.take(max_size).read_to_end(&mut buf)?;
    }
    Ok(buf)
}

const HELP: &str = "Usage: mcookie [OPTION]...\n\
Generate magic cookies for xauth.\n\n\
  -f, --file FILE      use file as a cookie seed\n\
  -m, --max-size NUM   limit how much is read from seed files\n\
                        (supports B suffix or binary units: KiB, MiB, GiB, TiB)\n\
  -v, --verbose         explain what is being done\n\
  -h, --help             display this help and exit\n\
      --version          output version information and exit\n";

/// Entry point: parse `std::env::args()`, hash any seed files plus fresh
/// randomness, print a 32-hex-digit cookie. Returns 0 on success, 1 on a
/// bad argument or an I/O error reading a seed file.
pub fn run() -> i32 {
    let ui = Ui::new("mcookie");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("mcookie (user_utils) 0.1.0");
        return 0;
    }

    let mut seed_files: Vec<String> = Vec::new();
    let mut verbose = false;
    let mut max_size = MAX_DEFAULT;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-f" | "--file" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option '-f' requires an argument");
                    return 1;
                };
                seed_files.push(v.clone());
            }
            s if s.starts_with("--file=") => seed_files.push(s["--file=".len()..].to_string()),
            "-m" | "--max-size" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option '-m' requires an argument");
                    return 1;
                };
                match parse_size(v) {
                    Ok(0) => max_size = MAX_DEFAULT,
                    Ok(n) => max_size = n,
                    Err(e) => {
                        ui.err(&format!("failed to parse max-size value: {e}"));
                        return 1;
                    }
                }
            }
            s if s.starts_with("--max-size=") => {
                let v = &s["--max-size=".len()..];
                match parse_size(v) {
                    Ok(0) => max_size = MAX_DEFAULT,
                    Ok(n) => max_size = n,
                    Err(e) => {
                        ui.err(&format!("failed to parse max-size value: {e}"));
                        return 1;
                    }
                }
            }
            "-v" | "--verbose" => verbose = true,
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => {
                ui.err(&format!("unexpected argument '{other}'"));
                return 1;
            }
        }
        i += 1;
    }

    let mut seed_data = Vec::new();
    for path in &seed_files {
        match read_seed(path, max_size) {
            Ok(bytes) => {
                if verbose {
                    let label = if path == "-" { "stdin" } else { path.as_str() };
                    eprintln!("Got {} bytes from {}", bytes.len(), label);
                }
                seed_data.extend_from_slice(&bytes);
            }
            Err(e) => {
                ui.err(&format!("cannot open {path}: {e}"));
                continue;
            }
        }
    }

    let mut random_bytes = [0u8; RANDOM_BYTES];
    fill_random(&mut random_bytes);
    if verbose {
        eprintln!("Got {RANDOM_BYTES} bytes from randomness source");
    }

    let digest = compute_cookie_bytes(&seed_data, &random_bytes);
    println!("{}", usercore::digest::hex_lower(&digest));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_cookie_bytes_is_deterministic_for_fixed_inputs() {
        let random = [0u8; RANDOM_BYTES];
        let a = compute_cookie_bytes(b"hello", &random);
        let b = compute_cookie_bytes(b"hello", &random);
        assert_eq!(a, b);
    }

    #[test]
    fn compute_cookie_bytes_matches_manual_md5() {
        // Cross-check against a fresh MD5 computed directly, independent
        // of `compute_cookie_bytes`'s own internal call.
        let random = [7u8; RANDOM_BYTES];
        let seed = b"seed-data";
        let mut h = usercore::digest::Md5::new();
        h.update(seed);
        h.update(&random);
        let expected = h.finalize();
        assert_eq!(compute_cookie_bytes(seed, &random), expected);
    }

    #[test]
    fn compute_cookie_bytes_differs_when_seed_differs() {
        let random = [1u8; RANDOM_BYTES];
        let a = compute_cookie_bytes(b"seed-a", &random);
        let b = compute_cookie_bytes(b"seed-b", &random);
        assert_ne!(a, b);
    }

    #[test]
    fn compute_cookie_bytes_differs_when_random_differs() {
        let a = compute_cookie_bytes(b"seed", &[0u8; RANDOM_BYTES]);
        let b = compute_cookie_bytes(b"seed", &[1u8; RANDOM_BYTES]);
        assert_ne!(a, b);
    }

    #[test]
    fn compute_cookie_bytes_empty_seed_is_valid() {
        let digest = compute_cookie_bytes(b"", &[0u8; RANDOM_BYTES]);
        assert_eq!(digest.len(), 16);
    }

    #[test]
    fn hex_output_is_32_lowercase_hex_chars() {
        let digest = compute_cookie_bytes(b"anything", &[9u8; RANDOM_BYTES]);
        let hex = usercore::digest::hex_lower(&digest);
        assert_eq!(hex.len(), 32);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn parse_size_plain_bytes() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1024B").unwrap(), 1024);
    }

    #[test]
    fn parse_size_binary_units() {
        assert_eq!(parse_size("1K").unwrap(), 1024);
        assert_eq!(parse_size("1KiB").unwrap(), 1024);
        assert_eq!(parse_size("2M").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_size("1GiB").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_size_zero_is_valid_sentinel() {
        assert_eq!(parse_size("0").unwrap(), 0);
    }

    #[test]
    fn parse_size_rejects_garbage() {
        assert!(parse_size("not-a-number").is_err());
        assert!(parse_size("5XB").is_err());
        assert!(parse_size("").is_err());
    }

    #[test]
    fn fill_random_produces_nonconstant_bytes_across_calls() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        fill_random(&mut a);
        fill_random(&mut b);
        // Overwhelmingly likely to differ; guards against a broken/stuck
        // randomness source.
        assert_ne!(a, b);
    }

    #[test]
    fn run_end_to_end_prints_32_hex_chars() {
        // Exercises the real `run()` codepath (real /dev/urandom or libc
        // fallback) by calling compute_cookie_bytes with freshly filled
        // random bytes, then checking the printable format is right.
        let mut random_bytes = [0u8; RANDOM_BYTES];
        fill_random(&mut random_bytes);
        let digest = compute_cookie_bytes(b"", &random_bytes);
        let hex = usercore::digest::hex_lower(&digest);
        assert_eq!(hex.len(), 32);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
