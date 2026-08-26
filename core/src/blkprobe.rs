//! Minimal filesystem superblock probing (ext2/3/4, swap, xfs, vfat/FAT,
//! iso9660) — a practical, from-scratch stand-in for libblkid, since this
//! workspace doesn't vendor or link it. Not a full port: just the common
//! on-disk signature/UUID/label fields, verified field-for-field against
//! util-linux's own `libblkid/src/superblocks/{ext,swap,xfs,vfat,
//! iso9660}.c` struct layouts and magic offsets, so the numbers here are
//! real ABI, not guessed.
use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;
use std::path::Path;

/// One probe result: filesystem type plus whatever UUID/label it carries
/// (FAT has no true UUID — its 4-byte volume serial is formatted the same
/// `XXXX-XXXX` way `blkid` reports it).
pub struct Probe {
    pub fstype: String,
    pub uuid: Option<String>,
    pub label: Option<String>,
}

fn read_at(f: &File, offset: u64, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    f.read_exact_at(&mut buf, offset).ok()?;
    Some(buf)
}

fn format_uuid(raw: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
        raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
    )
}

/// Trim trailing NULs/spaces and decode as UTF-8 (lossily — labels are
/// usually plain ASCII in practice); `None` if nothing's left.
fn clean_label(raw: &[u8]) -> Option<String> {
    let end = raw
        .iter()
        .rposition(|&b| b != 0 && b != b' ')
        .map(|i| i + 1)
        .unwrap_or(0);
    if end == 0 {
        return None;
    }
    let s = String::from_utf8_lossy(&raw[..end]).into_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// --- ext2/3/4 --- (libblkid's ext.c: struct ext2_super_block)
const EXT3_FEATURE_COMPAT_HAS_JOURNAL: u32 = 0x0004;
const EXT4_FEATURE_INCOMPAT_EXTENTS: u32 = 0x0040;
const EXT4_FEATURE_INCOMPAT_64BIT: u32 = 0x0080;

fn probe_ext(f: &File) -> Option<Probe> {
    let sb = read_at(f, 1024, 264)?;
    if sb[56] != 0x53 || sb[57] != 0xEF {
        return None;
    }
    let feature_compat = u32::from_le_bytes(sb[92..96].try_into().ok()?);
    let feature_incompat = u32::from_le_bytes(sb[96..100].try_into().ok()?);
    let uuid: [u8; 16] = sb[104..120].try_into().ok()?;
    let label = clean_label(&sb[120..136]);

    let fstype = if feature_compat & EXT3_FEATURE_COMPAT_HAS_JOURNAL == 0 {
        "ext2"
    } else if feature_incompat & (EXT4_FEATURE_INCOMPAT_EXTENTS | EXT4_FEATURE_INCOMPAT_64BIT) != 0
    {
        "ext4"
    } else {
        "ext3"
    };

    Some(Probe {
        fstype: fstype.to_string(),
        uuid: Some(format_uuid(&uuid)),
        label,
    })
}

// --- swap --- (libblkid's swap.c: struct swap_header_v1_2, header always
// at absolute offset 1024 regardless of which page size the magic sits
// at — try each real page size the kernel supports in turn).
fn probe_swap(f: &File) -> Option<Probe> {
    for pagesize in [4096u64, 8192, 16384, 32768, 65536] {
        let magic_off = pagesize - 10;
        let Some(magic) = read_at(f, magic_off, 10) else {
            continue;
        };
        let is_v1 = magic == b"SWAPSPACE2";
        let is_v0 = magic == b"SWAP-SPACE";
        if !is_v1 && !is_v0 {
            continue;
        }
        if is_v0 {
            // v0 has no UUID/label field at all.
            return Some(Probe {
                fstype: "swap".to_string(),
                uuid: None,
                label: None,
            });
        }
        let hdr = read_at(f, 1024, 44)?;
        let uuid: [u8; 16] = hdr[12..28].try_into().ok()?;
        let label = clean_label(&hdr[28..44]);
        return Some(Probe {
            fstype: "swap".to_string(),
            uuid: Some(format_uuid(&uuid)),
            label,
        });
    }
    None
}

// --- xfs --- (libblkid's xfs.c: struct xfs_super_block)
fn probe_xfs(f: &File) -> Option<Probe> {
    let sb = read_at(f, 0, 120)?;
    if &sb[0..4] != b"XFSB" {
        return None;
    }
    let uuid: [u8; 16] = sb[32..48].try_into().ok()?;
    let label = clean_label(&sb[108..120]);
    Some(Probe {
        fstype: "xfs".to_string(),
        uuid: Some(format_uuid(&uuid)),
        label,
    })
}

// --- vfat / FAT12/16/32 --- (libblkid's vfat.c: struct vfat_super_block /
// msdos_super_block — the magic-string path, which covers the vast
// majority of real-world FAT filesystems; the cluster-count fallback
// libblkid also has for magic-less filesystems isn't implemented here).
fn probe_vfat(f: &File) -> Option<Probe> {
    let boot = read_at(f, 0, 0x200)?;
    if boot[0x1FE] != 0x55 || boot[0x1FF] != 0xAA {
        return None;
    }
    // FAT32 BPB: magic at 0x52, ext-boot-sign at 0x42, serial at 0x43,
    // label at 0x47.
    if &boot[0x52..0x5A] == b"FAT32   " {
        return Some(fat_result(&boot, 0x42, 0x43, 0x47));
    }
    // FAT12/16 BPB: magic at 0x36, ext-boot-sign at 0x26, serial at
    // 0x27, label at 0x2b.
    let magic = &boot[0x36..0x3E];
    if magic == b"FAT12   " || magic == b"FAT16   " || magic == b"FAT     " {
        return Some(fat_result(&boot, 0x26, 0x27, 0x2b));
    }
    None
}

fn fat_result(boot: &[u8], ext_sign_off: usize, serial_off: usize, label_off: usize) -> Probe {
    let uuid = if boot[ext_sign_off] == 0x29 {
        let serial = &boot[serial_off..serial_off + 4];
        Some(format!(
            "{:02X}{:02X}-{:02X}{:02X}",
            serial[3], serial[2], serial[1], serial[0]
        ))
    } else {
        None
    };
    let label = if boot[ext_sign_off] == 0x29 {
        clean_label(&boot[label_off..label_off + 11])
    } else {
        None
    };
    Probe {
        fstype: "vfat".to_string(),
        uuid,
        label,
    }
}

// --- iso9660 --- (libblkid's iso9660.c: primary volume descriptor at
// sector 16; no native UUID field, so `uuid` is always `None`).
fn probe_iso9660(f: &File) -> Option<Probe> {
    let pvd = read_at(f, 32768, 190)?;
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return None;
    }
    let label = clean_label(&pvd[40..72]);
    Some(Probe {
        fstype: "iso9660".to_string(),
        uuid: None,
        label,
    })
}

/// Probe `path` for a recognized filesystem/swap signature. `Ok(None)`
/// means the file opened fine but nothing matched (not an error); a real
/// `Err` only for an open/seek failure the caller should surface (e.g.
/// permission denied).
pub fn probe_path(path: &Path) -> io::Result<Option<Probe>> {
    let f = File::open(path)?;
    for probe in [probe_ext, probe_xfs, probe_vfat, probe_iso9660, probe_swap] {
        if let Some(p) = probe(&f) {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch_file(tag: &str, size: usize) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "usercore_blkprobe_test_{tag}_{}",
            std::process::id()
        ));
        let mut f = File::create(&p).unwrap();
        f.write_all(&vec![0u8; size]).unwrap();
        p
    }

    #[test]
    fn format_uuid_matches_canonical_layout() {
        let raw: [u8; 16] = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef,
        ];
        assert_eq!(format_uuid(&raw), "01234567-89ab-cdef-0123-456789abcdef");
    }

    #[test]
    fn clean_label_trims_nul_and_spaces() {
        assert_eq!(clean_label(b"data\0\0\0\0"), Some("data".to_string()));
        assert_eq!(clean_label(b"root      "), Some("root".to_string()));
        assert_eq!(clean_label(b"\0\0\0\0\0\0\0\0"), None);
        assert_eq!(clean_label(b"          "), None);
    }

    #[test]
    fn probe_ext4_recognizes_magic_and_fields() {
        let p = scratch_file("ext4", 2048);
        {
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_at(&[0x53, 0xEF], 1024 + 56).unwrap();
            // feature_compat = HAS_JOURNAL
            f.write_at(&0x0004u32.to_le_bytes(), 1024 + 92).unwrap();
            // feature_incompat = EXTENTS
            f.write_at(&0x0040u32.to_le_bytes(), 1024 + 96).unwrap();
            let uuid = [1u8; 16];
            f.write_at(&uuid, 1024 + 104).unwrap();
            f.write_at(b"myroot\0\0\0\0\0\0\0\0\0\0", 1024 + 120)
                .unwrap();
        }
        let probe = probe_path(&p).unwrap().unwrap();
        assert_eq!(probe.fstype, "ext4");
        assert_eq!(probe.label.as_deref(), Some("myroot"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn probe_ext2_when_no_journal_feature() {
        let p = scratch_file("ext2", 2048);
        {
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_at(&[0x53, 0xEF], 1024 + 56).unwrap();
        }
        let probe = probe_path(&p).unwrap().unwrap();
        assert_eq!(probe.fstype, "ext2");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn probe_xfs_recognizes_magic_and_uuid() {
        let p = scratch_file("xfs", 512);
        {
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_at(b"XFSB", 0).unwrap();
            f.write_at(&[2u8; 16], 32).unwrap();
            f.write_at(b"xfsvol\0\0\0\0\0\0", 108).unwrap();
        }
        let probe = probe_path(&p).unwrap().unwrap();
        assert_eq!(probe.fstype, "xfs");
        assert_eq!(probe.label.as_deref(), Some("xfsvol"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn probe_swap_v1_recognizes_magic_and_uuid() {
        let p = scratch_file("swap", 8192);
        {
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_at(b"SWAPSPACE2", 4096 - 10).unwrap();
            f.write_at(&[3u8; 16], 1024 + 12).unwrap();
            f.write_at(b"myswap\0\0\0\0\0\0\0\0\0\0", 1024 + 28)
                .unwrap();
        }
        let probe = probe_path(&p).unwrap().unwrap();
        assert_eq!(probe.fstype, "swap");
        assert_eq!(probe.label.as_deref(), Some("myswap"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn probe_vfat_fat32_recognizes_magic_and_serial() {
        let p = scratch_file("fat32", 1024);
        {
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_at(&[0x55, 0xAA], 0x1FE).unwrap();
            f.write_at(b"FAT32   ", 0x52).unwrap();
            f.write_at(&[0x29], 0x42).unwrap();
            f.write_at(&[0x11, 0x22, 0x33, 0x44], 0x43).unwrap();
            f.write_at(b"USBDRIVE   ", 0x47).unwrap();
        }
        let probe = probe_path(&p).unwrap().unwrap();
        assert_eq!(probe.fstype, "vfat");
        assert_eq!(probe.uuid.as_deref(), Some("4433-2211"));
        assert_eq!(probe.label.as_deref(), Some("USBDRIVE"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn probe_iso9660_recognizes_pvd() {
        let p = scratch_file("iso", 32768 + 200);
        {
            let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_at(&[1], 32768).unwrap();
            f.write_at(b"CD001", 32769).unwrap();
            f.write_at(b"MY_DISC                        ", 32768 + 40)
                .unwrap();
        }
        let probe = probe_path(&p).unwrap().unwrap();
        assert_eq!(probe.fstype, "iso9660");
        assert_eq!(probe.label.as_deref(), Some("MY_DISC"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn probe_path_none_for_unrecognized_content() {
        let p = scratch_file("plain", 4096);
        assert!(probe_path(&p).unwrap().is_none());
        let _ = std::fs::remove_file(&p);
    }
}
