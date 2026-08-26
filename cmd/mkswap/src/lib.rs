//! user mkswap — set up a swap area.
//!
//! Writes a real v1 `SWAPSPACE2` header (`linux/swap.h`'s
//! `union swap_header`, verified against util-linux's own
//! `libblkid/src/superblocks/swap.c`), so the result is byte-for-byte
//! what the kernel's `swapon(2)` and real `blkid`/`mkswap` expect — not
//! a stub format.
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use usercore::Ui;

/// `struct swap_header_v1_2`'s on-disk size (from `lo_offset` field
/// layout: version + lastpage + nr_badpages + uuid[16] + volume[16] +
/// padding[117] + badpages[1], all after the 1024-byte `bootbits` area).
const HEADER_SIZE: usize = 4 + 4 + 4 + 16 + 16 + 117 * 4 + 4;
const SB_OFFSET: u64 = 1024;
const MIN_PAGES: u64 = 10;

/// The system's real page size — swap always uses it, and it varies by
/// architecture (4096 on x86_64/aarch64, but not universally).
fn page_size() -> u64 {
    // SAFETY: `sysconf` takes a plain integer constant and cannot fail
    // in a way that causes UB; a negative return (only possible if the
    // parameter were invalid, which `_SC_PAGESIZE` never is) is handled
    // by the fallback below.
    let r = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if r > 0 {
        r as u64
    } else {
        4096
    }
}

fn random_uuid() -> [u8; 16] {
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            // Stamp the standard "version 4, variant 1" bits so this
            // reads as a conventional random UUID like `uuidgen -r` would
            // produce, even though swap doesn't itself require that.
            buf[6] = (buf[6] & 0x0f) | 0x40;
            buf[8] = (buf[8] & 0x3f) | 0x80;
            return buf;
        }
    }
    buf
}

fn parse_uuid(s: &str) -> Option<[u8; 16]> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, chunk) in out.iter_mut().enumerate() {
        *chunk = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn set_label(buf: &mut [u8; 16], label: &str) {
    let bytes = label.as_bytes();
    let n = bytes.len().min(16);
    buf[..n].copy_from_slice(&bytes[..n]);
}

/// Determine `path`'s size in bytes: `metadata().len()` works for a
/// regular file, but block devices always report 0 there — `SEEK_END`
/// is what actually reports a block device's real size on Linux.
fn device_size(f: &mut std::fs::File) -> io::Result<u64> {
    let size = f.seek(SeekFrom::End(0))?;
    f.seek(SeekFrom::Start(0))?;
    Ok(size)
}

/// Build the 516-byte `swap_header_v1_2` payload (bootbits excluded —
/// this workspace never touches the first 1024 bytes, matching real
/// `mkswap`'s behavior of not wiping any existing boot sector there).
fn build_header(lastpage: u32, uuid: [u8; 16], label: &str) -> [u8; HEADER_SIZE] {
    let mut hdr = [0u8; HEADER_SIZE];
    hdr[0..4].copy_from_slice(&1u32.to_le_bytes());
    hdr[4..8].copy_from_slice(&lastpage.to_le_bytes());
    // nr_badpages stays 0.
    hdr[12..28].copy_from_slice(&uuid);
    let mut vol = [0u8; 16];
    set_label(&mut vol, label);
    hdr[28..44].copy_from_slice(&vol);
    hdr
}

fn format_uuid(raw: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
        raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
    )
}

/// Write a v1 swap signature to `path`: the header at offset 1024, and
/// the `SWAPSPACE2` magic at the last 10 bytes of the first page.
fn do_mkswap(path: &Path, label: &str, uuid: [u8; 16]) -> io::Result<u64> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let size = device_size(&mut f)?;
    let page = page_size();
    if size < page * MIN_PAGES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("swap area needs to be at least {MIN_PAGES} pages"),
        ));
    }
    let lastpage = (size / page) as u32 - 1;

    let hdr = build_header(lastpage, uuid, label);
    f.seek(SeekFrom::Start(SB_OFFSET))?;
    f.write_all(&hdr)?;

    f.seek(SeekFrom::Start(page - 10))?;
    f.write_all(b"SWAPSPACE2")?;
    f.flush()?;
    Ok(lastpage as u64 + 1)
}

fn print_help() {
    print!(
        "Usage: mkswap [-L LABEL] [-U UUID] DEVICE\n\
 Set up a Linux swap area on DEVICE.\n\
 -L, --label LABEL set a volume label\n\
 -U, --uuid UUID set a specific UUID (default: random)\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `mkswap` utility. Parses `std::env::args()` and
/// writes a v1 swap signature (with an optional `-L` label and `-U`
/// UUID, otherwise random) to `DEVICE`.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("mkswap");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut label = String::new();
    let mut uuid: Option<[u8; 16]> = None;
    let mut device: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("mkswap (user_utils) 0.1.0");
                return 0;
            }
            "-L" | "--label" => {
                i += 1;
                match args.get(i) {
                    Some(v) => label = v.clone(),
                    None => {
                        ui.err("option requires an argument -- 'L'");
                        return 1;
                    }
                }
            }
            "-U" | "--uuid" => {
                i += 1;
                match args.get(i).and_then(|v| parse_uuid(v)) {
                    Some(v) => uuid = Some(v),
                    None => {
                        ui.err("invalid or missing UUID");
                        return 1;
                    }
                }
            }
            "-f" | "--force" => {}
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => device = Some(other.to_string()),
        }
        i += 1;
    }

    let Some(device) = device else {
        ui.err("usage: mkswap [-L LABEL] [-U UUID] DEVICE");
        return 1;
    };
    let uuid = uuid.unwrap_or_else(random_uuid);

    match do_mkswap(Path::new(&device), &label, uuid) {
        Ok(pages) => {
            println!(
                "Setting up swapspace, size = {pages} pages, UUID={}",
                format_uuid(&uuid)
            );
            0
        }
        Err(e) => {
            ui.err(&format!("{device}: {e}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uuid_accepts_and_rejects() {
        assert!(parse_uuid("11111111-1111-1111-1111-111111111111").is_some());
        assert!(parse_uuid("not-a-uuid").is_none());
        assert!(parse_uuid("1111").is_none());
    }

    #[test]
    fn build_header_round_trips_via_our_own_probe() {
        let dir = std::env::temp_dir().join(format!("user_mkswap_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("swap.img");
        // 10 pages minimum; use the real page size to size the file.
        let page = page_size();
        std::fs::write(&path, vec![0u8; (page * 12) as usize]).unwrap();

        let uuid = [7u8; 16];
        let pages = do_mkswap(&path, "myswap", uuid).unwrap();
        assert_eq!(pages, 12);

        let probe = usercore::blkprobe::probe_path(&path).unwrap().unwrap();
        assert_eq!(probe.fstype, "swap");
        assert_eq!(probe.label.as_deref(), Some("myswap"));
        assert_eq!(probe.uuid, Some(format_uuid(&uuid)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn do_mkswap_rejects_too_small_a_file() {
        let dir =
            std::env::temp_dir().join(format!("user_mkswap_test_small_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tiny.img");
        std::fs::write(&path, vec![0u8; 4096]).unwrap();
        assert!(do_mkswap(&path, "", [0u8; 16]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
