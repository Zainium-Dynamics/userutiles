//! MBR (DOS) and GPT partition table reading/writing: a from-scratch
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const SECTOR_SIZE: u64 = 512;
const GPT_ENTRY_SIZE: u64 = 128;
const GPT_ENTRY_COUNT: u64 = 128;
/// Bytes occupied by the GPT partition entry array (128 entries × 128
/// bytes = 16 sectors at 512 bytes/sector: the value every real GPT
/// implementation uses).
const GPT_ENTRY_ARRAY_SECTORS: u64 = (GPT_ENTRY_COUNT * GPT_ENTRY_SIZE) / SECTOR_SIZE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Label {
    Dos,
    Gpt,
}

#[derive(Debug, Clone)]
pub struct Partition {
    /// 1-based slot number (MBR: 1..=4; GPT: 1..=128, first non-empty
    /// entries in array order).
    pub number: u32,
    pub start_lba: u64,
    pub size_lba: u64,
    /// MBR: two-hex-digit type byte (`"83"`, `"82"`, …). GPT: the
    /// canonical hyphenated type GUID.
    pub part_type: String,
    pub bootable: bool,
    /// GPT only: the UTF-16 partition name; always `None` for MBR.
    pub name: Option<String>,
    /// GPT only: the partition's unique GUID; `None` when writing makes a new random one.
    pub uuid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PartitionTable {
    pub label: Label,
    pub partitions: Vec<Partition>,
}

fn read_at(f: &mut File, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    f.seek(SeekFrom::Start(offset))?;
    f.read_exact(&mut buf)?;
    Ok(buf)
}

fn write_at(f: &mut File, offset: u64, data: &[u8]) -> io::Result<()> {
    f.seek(SeekFrom::Start(offset))?;
    f.write_all(data)
}

/// Determine `path`'s size in sectors: `SEEK_END` works uniformly for
/// both a regular (image) file and a real block device on Linux.
pub fn device_size_sectors(f: &mut File) -> io::Result<u64> {
    let size = f.seek(SeekFrom::End(0))?;
    f.seek(SeekFrom::Start(0))?;
    Ok(size / SECTOR_SIZE)
}

fn parse_mbr(boot: &[u8]) -> Vec<Partition> {
    let mut out = Vec::new();
    for slot in 0..4 {
        let off = 446 + slot * 16;
        let entry = &boot[off..off + 16];
        let part_type = entry[4];
        if part_type == 0 {
            continue;
        }
        let start_lba = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as u64;
        let size_lba = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as u64;
        out.push(Partition {
            number: slot as u32 + 1,
            start_lba,
            size_lba,
            part_type: format!("{part_type:02x}"),
            bootable: entry[0] == 0x80,
            name: None,
            uuid: None,
        });
    }
    out
}

/// Write a primary MBR table. Preserves the existing bootstrap code
/// (bytes 0..446) if the target already has a valid-looking sector
/// there: matches `sfdisk`'s own behavior of never touching boot code
/// it didn't write.
pub fn write_mbr(path: &Path, partitions: &[Partition]) -> io::Result<()> {
    if partitions.len() > 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "MBR supports at most 4 primary partitions",
        ));
    }
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let mut sector = read_at(&mut f, 0, 512).unwrap_or_else(|_| vec![0u8; 512]);

    for slot in 0..4 {
        let off = 446 + slot * 16;
        sector[off..off + 16].fill(0);
        let Some(p) = partitions.get(slot) else {
            continue;
        };
        let type_byte = u8::from_str_radix(&p.part_type, 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad MBR type byte"))?;
        sector[off] = if p.bootable { 0x80 } else { 0x00 };
        sector[off + 4] = type_byte;
        sector[off + 8..off + 12].copy_from_slice(&(p.start_lba as u32).to_le_bytes());
        sector[off + 12..off + 16].copy_from_slice(&(p.size_lba as u32).to_le_bytes());
    }
    sector[510] = 0x55;
    sector[511] = 0xAA;
    write_at(&mut f, 0, &sector)?;
    f.flush()
}

/// GPT GUIDs are mixed-endian: the first three fields are little-endian,
/// the last two are stored (and printed) big-endian/network order: per
/// the UEFI spec (and matching every real GPT tool's canonical display).
fn format_guid(raw: &[u8; 16]) -> String {
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes(raw[0..4].try_into().unwrap()),
        u16::from_le_bytes(raw[4..6].try_into().unwrap()),
        u16::from_le_bytes(raw[6..8].try_into().unwrap()),
        raw[8],
        raw[9],
        raw[10],
        raw[11],
        raw[12],
        raw[13],
        raw[14],
        raw[15],
    )
}

fn parse_guid(s: &str) -> Option<[u8; 16]> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    let mut raw = [0u8; 16];
    raw[0..4].copy_from_slice(&u32::from_be_bytes(bytes[0..4].try_into().unwrap()).to_le_bytes());
    raw[4..6].copy_from_slice(&u16::from_be_bytes(bytes[4..6].try_into().unwrap()).to_le_bytes());
    raw[6..8].copy_from_slice(&u16::from_be_bytes(bytes[6..8].try_into().unwrap()).to_le_bytes());
    raw[8..16].copy_from_slice(&bytes[8..16]);
    Some(raw)
}

/// Common short type aliases `sfdisk` itself accepts, resolved to the
/// GPT type GUID (or MBR type byte: see [`resolve_mbr_type`]).
pub fn resolve_gpt_type(alias: &str) -> Option<String> {
    match alias {
        "L" | "linux" => Some("0FC63DAF-8483-4772-8E79-3D69D8477DE4".to_string()),
        "S" | "swap" => Some("0657FD6D-A4AB-43C4-84E5-0933C84B4F4F".to_string()),
        "U" | "esp" | "uefi" => Some("C12A7328-F81F-11D2-BA4B-00A0C93EC93B".to_string()),
        _ if parse_guid(alias).is_some() => Some(alias.to_uppercase()),
        _ => None,
    }
}

/// Same short aliases, resolved to an MBR type byte instead.
pub fn resolve_mbr_type(alias: &str) -> Option<String> {
    match alias {
        "L" | "linux" => Some("83".to_string()),
        "S" | "swap" => Some("82".to_string()),
        "U" | "esp" | "uefi" => Some("ef".to_string()),
        _ if u8::from_str_radix(alias, 16).is_ok() && alias.len() <= 2 => {
            Some(format!("{:02x}", u8::from_str_radix(alias, 16).unwrap()))
        }
        _ => None,
    }
}

fn parse_gpt_entries(raw: &[u8]) -> Vec<Partition> {
    let mut out = Vec::new();
    for (i, chunk) in raw.chunks(GPT_ENTRY_SIZE as usize).enumerate() {
        let type_guid: [u8; 16] = chunk[0..16].try_into().unwrap();
        if type_guid == [0u8; 16] {
            continue;
        }
        let start_lba = u64::from_le_bytes(chunk[32..40].try_into().unwrap());
        let end_lba = u64::from_le_bytes(chunk[40..48].try_into().unwrap());
        let name_utf16: Vec<u16> = chunk[56..128]
            .chunks(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .take_while(|&c| c != 0)
            .collect();
        let name = String::from_utf16_lossy(&name_utf16);
        out.push(Partition {
            number: i as u32 + 1,
            start_lba,
            size_lba: end_lba.saturating_sub(start_lba) + 1,
            part_type: format_guid(&type_guid),
            bootable: false,
            name: if name.is_empty() { None } else { Some(name) },
            uuid: Some(format_guid(chunk[16..32].try_into().unwrap())),
        });
    }
    out
}

fn read_gpt(f: &mut File) -> io::Result<Option<Vec<Partition>>> {
    let header = read_at(f, SECTOR_SIZE, 512)?;
    if &header[0..8] != b"EFI PART" {
        return Ok(None);
    }
    let entry_lba = u64::from_le_bytes(header[72..80].try_into().unwrap());
    let num_entries = u32::from_le_bytes(header[80..84].try_into().unwrap());
    let entry_size = u32::from_le_bytes(header[84..88].try_into().unwrap());
    let array_len = num_entries as u64 * entry_size as u64;
    let raw = read_at(f, entry_lba * SECTOR_SIZE, array_len as usize)?;
    Ok(Some(parse_gpt_entries(&raw)))
}

fn gpt_header_bytes(
    current_lba: u64,
    backup_lba: u64,
    first_usable: u64,
    last_usable: u64,
    disk_guid: &[u8; 16],
    entry_lba: u64,
    entries_crc: u32,
) -> [u8; 92] {
    let mut h = [0u8; 92];
    h[0..8].copy_from_slice(b"EFI PART");
    h[8..12].copy_from_slice(&1u32.to_le_bytes());
    h[12..16].copy_from_slice(&92u32.to_le_bytes());
    h[24..32].copy_from_slice(&current_lba.to_le_bytes());
    h[32..40].copy_from_slice(&backup_lba.to_le_bytes());
    h[40..48].copy_from_slice(&first_usable.to_le_bytes());
    h[48..56].copy_from_slice(&last_usable.to_le_bytes());
    h[56..72].copy_from_slice(disk_guid);
    h[72..80].copy_from_slice(&entry_lba.to_le_bytes());
    h[80..84].copy_from_slice(&(GPT_ENTRY_COUNT as u32).to_le_bytes());
    h[84..88].copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
    h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    h
}

fn build_entry_array(partitions: &[Partition]) -> io::Result<Vec<u8>> {
    let mut arr = vec![0u8; (GPT_ENTRY_COUNT * GPT_ENTRY_SIZE) as usize];
    for (i, p) in partitions.iter().enumerate() {
        if i as u64 >= GPT_ENTRY_COUNT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("GPT supports at most {GPT_ENTRY_COUNT} partitions"),
            ));
        }
        let off = i * GPT_ENTRY_SIZE as usize;
        let type_guid = parse_guid(&p.part_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad GPT type GUID"))?;
        arr[off..off + 16].copy_from_slice(&type_guid);
        let unique = match &p.uuid {
            Some(u) => parse_guid(u).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("bad partition GUID {u}"),
                )
            })?,
            None => random_guid(),
        };
        arr[off + 16..off + 32].copy_from_slice(&unique);
        arr[off + 32..off + 40].copy_from_slice(&p.start_lba.to_le_bytes());
        let end_lba = p.start_lba + p.size_lba.saturating_sub(1);
        arr[off + 40..off + 48].copy_from_slice(&end_lba.to_le_bytes());
        if let Some(name) = &p.name {
            let utf16: Vec<u16> = name.encode_utf16().take(35).collect();
            for (j, unit) in utf16.iter().enumerate() {
                arr[off + 56 + j * 2..off + 56 + j * 2 + 2].copy_from_slice(&unit.to_le_bytes());
            }
        }
    }
    Ok(arr)
}

/// Write a fresh GPT table (protective MBR + primary header/entries +
/// backup header/entries) to `path`.
pub fn write_gpt(path: &Path, partitions: &[Partition]) -> io::Result<()> {
    write_gpt_with_id(path, partitions, None)
}

/// A random version 4 GUID, in GPT's byte order.
fn random_guid() -> [u8; 16] {
    let mut g = [0u8; 16];
    if let Ok(mut urandom) = File::open("/dev/urandom") {
        let _ = urandom.read_exact(&mut g);
    }
    g[7] = (g[7] & 0x0f) | 0x40;
    g[8] = (g[8] & 0x3f) | 0x80;
    g
}

/// `write_gpt` with the disk GUID given (`sfdisk`'s `label-id:`), or a random one for `None`.
pub fn write_gpt_with_id(
    path: &Path,
    partitions: &[Partition],
    disk_id: Option<&str>,
) -> io::Result<()> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let total_sectors = device_size_sectors(&mut f)?;
    if total_sectors < 2 + GPT_ENTRY_ARRAY_SECTORS * 2 + 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "device too small for a GPT table",
        ));
    }

    let mut mbr = vec![0u8; 512];
    mbr[446 + 4] = 0xEE;
    mbr[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes());
    let protective_size = (total_sectors - 1).min(0xFFFF_FFFF) as u32;
    mbr[446 + 12..446 + 16].copy_from_slice(&protective_size.to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    write_at(&mut f, 0, &mbr)?;

    let entries = build_entry_array(partitions)?;
    let entries_crc = crc32fast::hash(&entries);

    let first_usable = 2 + GPT_ENTRY_ARRAY_SECTORS;
    let backup_lba = total_sectors - 1;
    let last_usable = backup_lba - GPT_ENTRY_ARRAY_SECTORS - 1;
    let disk_guid = match disk_id {
        Some(id) => parse_guid(id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("bad disk GUID {id}"))
        })?,
        None => random_guid(),
    };

    let mut primary_header = gpt_header_bytes(
        1,
        backup_lba,
        first_usable,
        last_usable,
        &disk_guid,
        2,
        entries_crc,
    );
    let crc = crc32fast::hash(&primary_header);
    primary_header[16..20].copy_from_slice(&crc.to_le_bytes());
    write_at(&mut f, SECTOR_SIZE, &primary_header)?;
    write_at(&mut f, 2 * SECTOR_SIZE, &entries)?;

    let backup_entries_lba = backup_lba - GPT_ENTRY_ARRAY_SECTORS;
    let mut backup_header = gpt_header_bytes(
        backup_lba,
        1,
        first_usable,
        last_usable,
        &disk_guid,
        backup_entries_lba,
        entries_crc,
    );
    let crc = crc32fast::hash(&backup_header);
    backup_header[16..20].copy_from_slice(&crc.to_le_bytes());
    write_at(&mut f, backup_entries_lba * SECTOR_SIZE, &entries)?;
    write_at(&mut f, backup_lba * SECTOR_SIZE, &backup_header)?;

    f.flush()
}

/// Read whichever partition table `path` has (GPT preferred when a
/// protective-MBR + valid GPT header is present, else plain MBR).
/// `Ok(None)` means no recognizable signature: not an error.
pub fn read_table(path: &Path) -> io::Result<Option<PartitionTable>> {
    let mut f = File::open(path)?;
    let boot = match read_at(&mut f, 0, 512) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    if boot[510] != 0x55 || boot[511] != 0xAA {
        return Ok(None);
    }

    let is_protective = boot[446 + 4] == 0xEE;
    if is_protective {
        if let Some(partitions) = read_gpt(&mut f)? {
            return Ok(Some(PartitionTable {
                label: Label::Gpt,
                partitions,
            }));
        }
    }
    Ok(Some(PartitionTable {
        label: Label::Dos,
        partitions: parse_mbr(&boot),
    }))
}

/// The GUIDs of a GPT disk, for `sfdisk -d`: the disk GUID (label-id) and each partition's
/// unique GUID by partition number. `Ok(None)` when the disk has no GPT.
pub fn gpt_ids(path: &Path) -> io::Result<Option<(String, Vec<(u32, String)>)>> {
    let mut f = File::open(path)?;
    let header = read_at(&mut f, SECTOR_SIZE, 512)?;
    if &header[0..8] != b"EFI PART" {
        return Ok(None);
    }
    let disk: [u8; 16] = header[56..72].try_into().unwrap();
    let entry_lba = u64::from_le_bytes(header[72..80].try_into().unwrap());
    let count = u32::from_le_bytes(header[80..84].try_into().unwrap()) as u64;
    let esize = u32::from_le_bytes(header[84..88].try_into().unwrap()) as u64;
    let raw = read_at(&mut f, entry_lba * SECTOR_SIZE, (count * esize) as usize)?;
    let mut ids = Vec::new();
    for (i, e) in raw.chunks(esize as usize).enumerate() {
        if e[0..16].iter().all(|&b| b == 0) {
            continue;
        }
        let uuid: [u8; 16] = e[16..32].try_into().unwrap();
        ids.push((i as u32 + 1, format_guid(&uuid)));
    }
    Ok(Some((format_guid(&disk), ids)))
}

/// Checks the partition table of `path` the way `sfdisk --verify` does and returns the
/// problems found; an empty list means none. For GPT: both headers (signature, CRC, where
/// they say they and each other are), both entry arrays (CRC, and that they match), every
/// partition inside the usable area, and no two partitions overlapping. For MBR: every
/// partition inside the disk, and no overlaps.
pub fn verify(path: &Path) -> io::Result<Vec<String>> {
    let mut f = File::open(path)?;
    let total = device_size_sectors(&mut f)?;
    let boot = read_at(&mut f, 0, 512)?;
    let mut problems = Vec::new();
    if boot[510] != 0x55 || boot[511] != 0xAA {
        problems.push("no partition table found (no 55AA signature in the MBR)".into());
        return Ok(problems);
    }
    let le32 = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let le64 = |b: &[u8], o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let mut ranges: Vec<(u32, u64, u64)> = Vec::new();

    if boot[446 + 4] == 0xEE {
        let mut header_of =
            |lba: u64, which: &str, problems: &mut Vec<String>| -> io::Result<Option<Vec<u8>>> {
                let h = read_at(&mut f, lba * SECTOR_SIZE, SECTOR_SIZE as usize)?;
                if &h[0..8] != b"EFI PART" {
                    problems.push(format!("{which} GPT header at LBA {lba} is missing"));
                    return Ok(None);
                }
                let size = le32(&h, 12) as usize;
                if !(92..=SECTOR_SIZE as usize).contains(&size) {
                    problems.push(format!("{which} GPT header has a bad size ({size})"));
                    return Ok(None);
                }
                let mut copy = h[..size].to_vec();
                copy[16..20].fill(0);
                if crc32fast::hash(&copy) != le32(&h, 16) {
                    problems.push(format!("{which} GPT header is corrupt (CRC mismatch)"));
                }
                if le64(&h, 24) != lba {
                    problems.push(format!(
                        "{which} GPT header says it is at LBA {}, not {lba}",
                        le64(&h, 24)
                    ));
                }
                Ok(Some(h))
            };
        let last = total.saturating_sub(1);
        let primary = header_of(1, "primary", &mut problems)?;
        let backup = header_of(last, "backup", &mut problems)?;
        let mut arrays = Vec::new();
        for (h, which) in [(&primary, "primary"), (&backup, "backup")] {
            let Some(h) = h else { continue };
            let count = le32(h, 80) as u64;
            let esize = le32(h, 84) as u64;
            if esize < 128 || count == 0 || count * esize > 1 << 20 {
                problems.push(format!(
                    "{which} GPT entry array has a bad shape ({count} x {esize})"
                ));
                continue;
            }
            let entries = read_at(&mut f, le64(h, 72) * SECTOR_SIZE, (count * esize) as usize)?;
            if crc32fast::hash(&entries) != le32(h, 88) {
                problems.push(format!("{which} GPT entry array is corrupt (CRC mismatch)"));
            }
            arrays.push((which, h.clone(), entries, esize));
        }
        if let (Some(p), Some(b)) = (&primary, &backup) {
            if le64(p, 32) != last {
                problems.push(format!(
                    "primary GPT header says the backup is at LBA {}, not {last}",
                    le64(p, 32)
                ));
            }
            if le64(b, 32) != 1 {
                problems.push(format!(
                    "backup GPT header says the primary is at LBA {}, not 1",
                    le64(b, 32)
                ));
            }
        }
        if arrays.len() == 2 && arrays[0].2 != arrays[1].2 {
            problems.push("the primary and backup GPT entry arrays differ".into());
        }
        if let Some((_, h, entries, esize)) = arrays.first() {
            let (first_usable, last_usable) = (le64(h, 40), le64(h, 48));
            for (i, e) in entries.chunks(*esize as usize).enumerate() {
                if e[0..16].iter().all(|&b| b == 0) {
                    continue;
                }
                let (start, end) = (le64(e, 32), le64(e, 40));
                let n = i as u32 + 1;
                if end < start {
                    problems.push(format!(
                        "partition {n} ends (LBA {end}) before it starts (LBA {start})"
                    ));
                } else if start < first_usable || end > last_usable {
                    problems.push(format!(
                        "partition {n} (LBA {start}-{end}) is outside the usable area (LBA {first_usable}-{last_usable})"
                    ));
                }
                ranges.push((n, start, end));
            }
        }
    } else {
        for slot in 0..4 {
            let e = &boot[446 + slot * 16..446 + slot * 16 + 16];
            if e[4] == 0 {
                continue;
            }
            let (start, size) = (le32(e, 8) as u64, le32(e, 12) as u64);
            let n = slot as u32 + 1;
            if size == 0 {
                problems.push(format!("partition {n} has no sectors"));
                continue;
            }
            let end = start + size - 1;
            if end >= total {
                problems.push(format!("partition {n} (LBA {start}-{end}) goes past the end of the disk ({total} sectors)"));
            }
            ranges.push((n, start, end));
        }
    }

    ranges.sort_by_key(|r| r.1);
    for w in ranges.windows(2) {
        if w[1].1 <= w[0].2 {
            problems.push(format!("partitions {} and {} overlap", w[0].0, w[1].0));
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_file(tag: &str, sectors: u64) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("usercore_ptable_test_{tag}_{}", std::process::id()));
        std::fs::write(&p, vec![0u8; (sectors * SECTOR_SIZE) as usize]).unwrap();
        p
    }

    #[test]
    fn format_guid_matches_canonical_mixed_endian_layout() {
        let raw = parse_guid("0FC63DAF-8483-4772-8E79-3D69D8477DE4").unwrap();
        assert_eq!(format_guid(&raw), "0FC63DAF-8483-4772-8E79-3D69D8477DE4");
    }

    #[test]
    fn mbr_round_trips_through_write_and_read() {
        let path = scratch_file("mbr", 2048);
        let partitions = vec![
            Partition {
                number: 1,
                start_lba: 2048,
                size_lba: 1000,
                part_type: "83".to_string(),
                bootable: true,
                name: None,
                uuid: None,
            },
            Partition {
                number: 2,
                start_lba: 3048,
                size_lba: 500,
                part_type: "82".to_string(),
                bootable: false,
                name: None,
                uuid: None,
            },
        ];
        write_mbr(&path, &partitions).unwrap();

        let table = read_table(&path).unwrap().unwrap();
        assert_eq!(table.label, Label::Dos);
        assert_eq!(table.partitions.len(), 2);
        assert_eq!(table.partitions[0].start_lba, 2048);
        assert_eq!(table.partitions[0].size_lba, 1000);
        assert_eq!(table.partitions[0].part_type, "83");
        assert!(table.partitions[0].bootable);
        assert!(!table.partitions[1].bootable);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn gpt_round_trips_through_write_and_read() {
        let path = scratch_file("gpt", 4096);
        let partitions = vec![Partition {
            number: 1,
            start_lba: 2048,
            size_lba: 1000,
            part_type: resolve_gpt_type("L").unwrap(),
            bootable: false,
            name: Some("root".to_string()),
            uuid: None,
        }];
        write_gpt(&path, &partitions).unwrap();

        let table = read_table(&path).unwrap().unwrap();
        assert_eq!(table.label, Label::Gpt);
        assert_eq!(table.partitions.len(), 1);
        assert_eq!(table.partitions[0].start_lba, 2048);
        assert_eq!(table.partitions[0].size_lba, 1000);
        assert_eq!(
            table.partitions[0].part_type,
            "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
        );
        assert_eq!(table.partitions[0].name.as_deref(), Some("root"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_table_none_for_unsigned_disk() {
        let path = scratch_file("blank", 2048);
        assert!(read_table(&path).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resolve_type_aliases() {
        assert_eq!(resolve_mbr_type("L"), Some("83".to_string()));
        assert_eq!(resolve_mbr_type("S"), Some("82".to_string()));
        assert_eq!(
            resolve_gpt_type("S"),
            Some("0657FD6D-A4AB-43C4-84E5-0933C84B4F4F".to_string())
        );
    }

    #[test]
    fn verify_accepts_a_written_gpt_and_catches_damage() {
        let p = scratch_file("verify", 4096);
        let parts = vec![Partition {
            number: 1,
            start_lba: 2048,
            size_lba: 1024,
            part_type: "0FC63DAF-8483-4772-8E79-3D69D8477DE4".into(),
            bootable: false,
            name: Some("root".into()),
            uuid: None,
        }];
        write_gpt(&p, &parts).unwrap();
        assert!(verify(&p).unwrap().is_empty(), "{:?}", verify(&p).unwrap());
        // flip a byte of the primary header
        let mut data = std::fs::read(&p).unwrap();
        data[512 + 40] ^= 0xff;
        std::fs::write(&p, &data).unwrap();
        let problems = verify(&p).unwrap();
        assert!(
            problems
                .iter()
                .any(|m| m.contains("primary GPT header is corrupt")),
            "{problems:?}"
        );
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn verify_reports_overlapping_mbr_partitions() {
        let p = scratch_file("verify_mbr", 4096);
        let part = |n, start| Partition {
            number: n,
            start_lba: start,
            size_lba: 1000,
            part_type: "83".into(),
            bootable: false,
            name: None,
            uuid: None,
        };
        write_mbr(&p, &[part(1, 2048), part(2, 2500)]).unwrap();
        let problems = verify(&p).unwrap();
        assert!(
            problems.iter().any(|m| m.contains("overlap")),
            "{problems:?}"
        );
        std::fs::remove_file(&p).ok();
    }
}
