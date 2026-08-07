//! user dd — convert and copy a file (GNU-compatible core operands).
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};

use std::path::Path;
use usercore::{protect, Ui};

/// Entry point for the `dd` utility. Parses `key=value` operands from
/// `std::env::args()` (`if=`, `of=`, `bs=`, `ibs=`, `obs=`, `count=`,
/// `skip=`, `seek=`, `conv=notrunc,sync`, `status=none|noxfer|progress`)
/// and copies `if` to `of` in `ibs`/`obs`-sized blocks.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("dd");
    let mut ifile = String::from("-");
    let mut ofile = String::from("-");
    let mut bs: Option<u64> = None;
    let mut ibs: u64 = 512;
    let mut obs: u64 = 512;
    let mut count: Option<u64> = None;
    let mut skip: u64 = 0;
    let mut seek: u64 = 0;
    let mut conv_notrunc = false;
    let mut conv_sync = false;
    let mut status_level = 1; // 0 none, 1 default, 2 noxfer

    for arg in std::env::args().skip(1) {
        if arg == "--help" || arg == "-h" {
            print!(
                "Usage: dd [OPERAND]...\n\
 Copy a file, converting and formatting according to operands.\n\n\
 if=FILE read from FILE instead of stdin\n\
 of=FILE write to FILE instead of stdout\n\
 bs=BYTES read and write up to BYTES bytes at a time\n\
 ibs=BYTES read up to BYTES bytes at a time (default 512)\n\
 obs=BYTES write BYTES bytes at a time (default 512)\n\
 count=N copy only N input blocks\n\
 skip=N skip N input blocks at start\n\
 seek=N skip N output blocks at start\n\
 conv=notrunc,sync\n\
 status=none|noxfer|progress\n"
            );
            return 0;
        }
        if arg == "--version" {
            println!("dd (user_utils) 0.1.0");
            return 0;
        }
        if let Some((k, v)) = arg.split_once('=') {
            match k {
                "if" => ifile = v.to_string(),
                "of" => ofile = v.to_string(),
                "bs" => {
                    bs = Some(parse_bytes(v).unwrap_or(512));
                }
                "ibs" => ibs = parse_bytes(v).unwrap_or(512),
                "obs" => obs = parse_bytes(v).unwrap_or(512),
                "count" => count = v.parse().ok(),
                "skip" => skip = v.parse().unwrap_or(0),
                "seek" => seek = v.parse().unwrap_or(0),
                "conv" => {
                    for c in v.split(',') {
                        match c {
                            "notrunc" => conv_notrunc = true,
                            "sync" => conv_sync = true,
                            _ => {}
                        }
                    }
                }
                "status" => {
                    status_level = match v {
                        "none" => 0,
                        "noxfer" => 2,
                        _ => 1,
                    };
                }
                _ => {
                    ui.err(&format!("unrecognized operand '{arg}'"));
                    return 1;
                }
            }
        } else {
            ui.err(&format!("unrecognized operand '{arg}'"));
            return 1;
        }
    }

    if let Some(b) = bs {
        ibs = b;
        obs = b;
    }
    if ibs == 0 || obs == 0 {
        ui.err("invalid block size");
        return 1;
    }

    let mut reader: Box<dyn Read> = if ifile == "-" {
        Box::new(io::stdin())
    } else {
        match OpenOptions::new().read(true).open(&ifile) {
            Ok(f) => Box::new(f),
            Err(e) => {
                ui.err(&format!("failed to open '{ifile}': {e}"));
                return 1;
            }
        }
    };

    if skip > 0 {
        let skip_bytes = skip.saturating_mul(ibs);
        // try seek, else read-discard
        let mut discarded = 0u64;
        let mut buf = vec![0u8; ibs as usize];
        while discarded < skip_bytes {
            let want = ((skip_bytes - discarded) as usize).min(buf.len());
            match reader.read(&mut buf[..want]) {
                Ok(0) => break,
                Ok(n) => discarded += n as u64,
                Err(e) => {
                    ui.err(&format!("{e}"));
                    return 1;
                }
            }
        }
    }

    let mut writer: Box<dyn Write> = if ofile == "-" {
        Box::new(io::stdout())
    } else {
        if let Some(reason) = protect::modification_denied(Path::new(&ofile)) {
            ui.err(&format!("{ofile}: {}", reason.message()));
            return 1;
        }
        let mut opts = OpenOptions::new();
        opts.write(true).create(true);
        if !conv_notrunc {
            opts.truncate(true);
        }
        match opts.open(&ofile) {
            Ok(mut f) => {
                if seek > 0 {
                    let off = seek.saturating_mul(obs) as i64;
                    let _ = f.seek(SeekFrom::Start(off as u64));
                }
                Box::new(f)
            }
            Err(e) => {
                ui.err(&format!("failed to open '{ofile}': {e}"));
                return 1;
            }
        }
    };

    let mut in_full = 0u64;
    let mut in_partial = 0u64;
    let mut out_full = 0u64;
    let mut out_partial = 0u64;
    let mut total_bytes = 0u64;
    let mut blocks_done = 0u64;
    let mut ibuf = vec![0u8; ibs as usize];
    let mut pending = Vec::new();

    loop {
        if let Some(c) = count {
            if blocks_done >= c {
                break;
            }
        }
        match reader.read(&mut ibuf) {
            Ok(0) => break,
            Ok(n) => {
                blocks_done += 1;
                if n as u64 == ibs {
                    in_full += 1;
                } else {
                    in_partial += 1;
                }
                let mut chunk = ibuf[..n].to_vec();
                if conv_sync && (n as u64) < ibs {
                    chunk.resize(ibs as usize, 0);
                }
                pending.extend_from_slice(&chunk);
                // flush obs-sized chunks
                while pending.len() as u64 >= obs {
                    let out = &pending[..obs as usize];
                    if let Err(e) = writer.write_all(out) {
                        ui.err(&format!("writing '{ofile}': {e}"));
                        return 1;
                    }
                    out_full += 1;
                    total_bytes += obs;
                    pending.drain(..obs as usize);
                }
            }
            Err(e) => {
                ui.err(&format!("reading '{ifile}': {e}"));
                return 1;
            }
        }
    }
    if !pending.is_empty() {
        if let Err(e) = writer.write_all(&pending) {
            ui.err(&format!("writing '{ofile}': {e}"));
            return 1;
        }
        out_partial += 1;
        total_bytes += pending.len() as u64;
    }
    let _ = writer.flush();

    if status_level > 0 {
        eprintln!("{in_full}+{in_partial} records in");
        eprintln!("{out_full}+{out_partial} records out");
        if status_level == 1 {
            eprintln!("{total_bytes} bytes ({}) copied", human(total_bytes));
        }
    }
    0
}

/// Parse a `dd`-style byte-count operand, e.g. `512`, `4K`, `2M`, `1GB`,
/// or the multiplicative `AxB` form (e.g. `2x512`). Returns `None` if `s`
/// is empty or not a valid count.
fn parse_bytes(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    let mut mult = 1u64;
    let mut num = s;
    // suffixes: c w b kB K MB M GB G ...
    if let Some(rest) = s.strip_suffix("GB").or_else(|| s.strip_suffix("G")) {
        num = rest;
        mult = 1024 * 1024 * 1024;
        if s.ends_with("GB") {
            mult = 1000 * 1000 * 1000;
        }
    } else if let Some(rest) = s.strip_suffix("MB").or_else(|| s.strip_suffix("M")) {
        num = rest;
        mult = if s.ends_with("MB") {
            1000 * 1000
        } else {
            1024 * 1024
        };
    } else if let Some(rest) = s.strip_suffix("kB") {
        num = rest;
        mult = 1000;
    } else if let Some(rest) = s.strip_suffix('K').or_else(|| s.strip_suffix('k')) {
        num = rest;
        mult = 1024;
    } else if let Some(rest) = s.strip_suffix('b') {
        num = rest;
        mult = 512;
    } else if let Some(rest) = s.strip_suffix('w') {
        num = rest;
        mult = 2;
    } else if let Some(rest) = s.strip_suffix('c') {
        num = rest;
        mult = 1;
    }
    // xM form like 2x512
    if let Some((a, b)) = num.split_once('x') {
        let av: u64 = a.parse().ok()?;
        let bv: u64 = b.parse().ok()?;
        return Some(av.saturating_mul(bv).saturating_mul(mult));
    }
    num.parse::<u64>().ok().map(|n| n.saturating_mul(mult))
}

/// Format a byte count as a human-readable size using binary (1024-based)
/// units, e.g. `1536` -> `"1.5 KiB"`.
fn human(n: u64) -> String {
    const U: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bytes_plain_number() {
        assert_eq!(parse_bytes("512"), Some(512));
        assert_eq!(parse_bytes(""), None);
    }

    #[test]
    fn parse_bytes_suffixes() {
        assert_eq!(parse_bytes("1K"), Some(1024));
        assert_eq!(parse_bytes("1k"), Some(1024));
        assert_eq!(parse_bytes("1kB"), Some(1000));
        assert_eq!(parse_bytes("1M"), Some(1024 * 1024));
        assert_eq!(parse_bytes("1MB"), Some(1_000_000));
        assert_eq!(parse_bytes("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_bytes("1GB"), Some(1_000_000_000));
        assert_eq!(parse_bytes("1b"), Some(512));
        assert_eq!(parse_bytes("1w"), Some(2));
        assert_eq!(parse_bytes("1c"), Some(1));
    }

    #[test]
    fn parse_bytes_multiplicative_form() {
        assert_eq!(parse_bytes("2x512"), Some(1024));
        assert_eq!(parse_bytes("2x1K"), Some(2048));
    }

    #[test]
    fn parse_bytes_invalid_input() {
        assert_eq!(parse_bytes("abc"), None);
        assert_eq!(parse_bytes("--"), None);
    }

    #[test]
    fn human_formats_units() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(1024), "1.0 KiB");
        assert_eq!(human(1536), "1.5 KiB");
        assert_eq!(human(1024 * 1024), "1.0 MiB");
    }
}
