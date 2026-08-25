//! user tar — ustar create/extract/list (basic POSIX subset).
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Entry point for the `tar` utility. Parses `std::env::args()` and
/// dispatches to create (`-c`), list (`-t`), or extract (`-x`) mode.
///
/// Returns 0 on success, 2 on a usage or I/O error (matching GNU tar's
/// convention of using 2 for hard errors).
pub fn run() -> i32 {
    let ui = Ui::new("tar");
    let mut create = false;
    let mut extract = false;
    let mut list = false;
    let mut verbose = false;
    let mut file: Option<PathBuf> = None;
    let mut paths: Vec<PathBuf> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        // support clustered -xvf
        if a.starts_with('-') && a.len() > 1 && !a.starts_with("--") {
            for c in a.chars().skip(1) {
                match c {
                    'c' => create = true,
                    'x' => extract = true,
                    't' => list = true,
                    'v' => verbose = true,
                    'f' => {
                        i += 1;
                        file = args.get(i).map(PathBuf::from);
                    }
                    'h' => {
                        print_help();
                        return 0;
                    }
                    _ => {}
                }
            }
            i += 1;
            continue;
        }
        match a.as_str() {
            "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("tar (user_utils) 0.1.0");
                return 0;
            }
            "-c" | "--create" => create = true,
            "-x" | "--extract" => extract = true,
            "-t" | "--list" => list = true,
            "-v" | "--verbose" => verbose = true,
            "-f" | "--file" => {
                i += 1;
                file = args.get(i).map(PathBuf::from);
            }
            s if s.starts_with("-f") && s.len() > 2 => file = Some(PathBuf::from(&s[2..])),
            other => paths.push(PathBuf::from(other)),
        }
        i += 1;
    }
    let modes = [create, extract, list].iter().filter(|x| **x).count();
    if modes != 1 {
        ui.err("must specify exactly one of -c, -x, -t");
        return 2;
    }
    let archive = file.unwrap_or_else(|| PathBuf::from("-"));
    if create {
        if paths.is_empty() {
            ui.err("Cowardly refusing to create an empty archive");
            return 2;
        }
        return match create_tar(&archive, &paths, verbose) {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&e.to_string());
                2
            }
        };
    }
    if list {
        return match list_tar(&archive, verbose) {
            Ok(()) => 0,
            Err(e) => {
                ui.err(&e.to_string());
                2
            }
        };
    }
    // extract
    match extract_tar(&archive, verbose) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&e.to_string());
            2
        }
    }
}

fn print_help() {
    print!(
        "Usage: tar [-c|-x|-t] [-v] [-f ARCHIVE] [FILE]...\n\
Create, extract, or list a ustar archive.\n\n\
  -c, --create      create a new archive\n\
  -x, --extract     extract files from an archive\n\
  -t, --list        list the contents of an archive\n\
  -v, --verbose     verbosely list files processed\n\
  -f, --file FILE   use archive FILE (or '-' for stdio)\n\
      --help        display this help and exit\n\
      --version     output version information and exit\n"
    );
}

fn create_tar(archive: &Path, paths: &[PathBuf], verbose: bool) -> io::Result<()> {
    let mut out: Box<dyn Write> = if archive.as_os_str() == "-" {
        Box::new(io::stdout())
    } else {
        Box::new(File::create(archive)?)
    };
    for p in paths {
        add_path(&mut out, p, p, verbose)?;
    }
    // two zero blocks
    out.write_all(&[0u8; 512])?;
    out.write_all(&[0u8; 512])?;
    out.flush()
}

/// Recursively add `path` (relative to `base`) to `out` as one or more
/// ustar header+data blocks. Uses [`fs::symlink_metadata`] (not
/// `Path::is_dir`) so a symlink to a directory is archived as a symlink
/// rather than followed into — this avoids both infinite loops on cyclic
/// symlinks and archiving a target the caller didn't ask for.
fn add_path(out: &mut dyn Write, base: &Path, path: &Path, verbose: bool) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let name = path
        .strip_prefix(base.parent().unwrap_or(base))
        .unwrap_or(path);
    let name_s = name.to_string_lossy().replace('\\', "/");
    if meta.is_dir() {
        write_header(
            out,
            &(name_s.trim_end_matches('/').to_string() + "/"),
            0o40755,
            0,
            b'5',
        )?;
        if verbose {
            eprintln!("{}/", name_s);
        }
        for ent in fs::read_dir(path)? {
            let ent = ent?;
            add_path(out, base, &ent.path(), verbose)?;
        }
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(path)?;
        write_header_link(out, &name_s, &target.to_string_lossy(), 0o120777)?;
        if verbose {
            eprintln!("{}", name_s);
        }
    } else {
        let mut f = File::open(path)?;
        let size = meta.len();
        write_header(out, &name_s, 0o100644, size, b'0')?;
        if verbose {
            eprintln!("{}", name_s);
        }
        io::copy(&mut f, out)?;
        let pad = (512 - (size % 512) as usize) % 512;
        if pad > 0 {
            out.write_all(&vec![0u8; pad])?;
        }
    }
    Ok(())
}

fn write_header(out: &mut dyn Write, name: &str, mode: u32, size: u64, typ: u8) -> io::Result<()> {
    let mut blk = [0u8; 512];
    put(&mut blk[0..100], name.as_bytes());
    put(&mut blk[100..108], format!("{:o}", mode).as_bytes());
    put(&mut blk[108..116], b"0000000 ");
    put(&mut blk[116..124], b"0000000 ");
    put(&mut blk[124..136], format!("{:o}", size).as_bytes());
    let mtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    put(&mut blk[136..148], format!("{:o}", mtime).as_bytes());
    blk[156] = typ;
    // checksum
    put(&mut blk[148..156], b" ");
    let sum: u32 = blk.iter().map(|b| *b as u32).sum();
    let mut chk = format!("{:06o}", sum).into_bytes();
    chk.push(0);
    chk.push(b' ');
    put(&mut blk[148..156], &chk);
    out.write_all(&blk)
}

fn write_header_link(out: &mut dyn Write, name: &str, link: &str, mode: u32) -> io::Result<()> {
    let mut blk = [0u8; 512];
    put(&mut blk[0..100], name.as_bytes());
    put(&mut blk[100..108], format!("{:o}", mode).as_bytes());
    put(&mut blk[108..116], b"0000000 ");
    put(&mut blk[116..124], b"0000000 ");
    put(&mut blk[124..136], b"00000000000 ");
    let mtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    put(&mut blk[136..148], format!("{:o}", mtime).as_bytes());
    blk[156] = b'2';
    put(&mut blk[157..257], link.as_bytes());
    put(&mut blk[148..156], b" ");
    let sum: u32 = blk.iter().map(|b| *b as u32).sum();
    let mut chk = format!("{:06o}", sum).into_bytes();
    chk.push(0);
    chk.push(b' ');
    put(&mut blk[148..156], &chk);
    out.write_all(&blk)
}

fn put(dst: &mut [u8], src: &[u8]) {
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);
}

fn list_tar(archive: &Path, verbose: bool) -> io::Result<()> {
    let mut input: Box<dyn Read> = if archive.as_os_str() == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(archive)?)
    };
    loop {
        let mut hdr = [0u8; 512];
        let n = input.read(&mut hdr)?;
        if n == 0 || hdr.iter().all(|&b| b == 0) {
            break;
        }
        let name = cstr(&hdr[0..100]);
        let size = oct(&hdr[124..136])?;
        let typ = hdr[156];
        if verbose {
            println!("{} {:>10} {}", typ as char, size, name);
        } else {
            println!("{name}");
        }
        let skip = ((size + 511) / 512) * 512;
        io::copy(&mut input.by_ref().take(skip), &mut io::sink())?;
    }
    Ok(())
}

fn extract_tar(archive: &Path, verbose: bool) -> io::Result<()> {
    let mut input: Box<dyn Read> = if archive.as_os_str() == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(archive)?)
    };
    loop {
        let mut hdr = [0u8; 512];
        let n = input.read(&mut hdr)?;
        if n == 0 || hdr.iter().all(|&b| b == 0) {
            break;
        }
        let name = cstr(&hdr[0..100]);
        let size = oct(&hdr[124..136])?;
        let typ = hdr[156];
        if name.is_empty() {
            break;
        }
        // security: reject absolute / ..
        let path = PathBuf::from(&name);
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            eprintln!("tar: skipping unsafe path {name}");
            let skip = ((size + 511) / 512) * 512;
            io::copy(&mut input.by_ref().take(skip), &mut io::sink())?;
            continue;
        }
        match typ {
            b'5' | b'/' => {
                fs::create_dir_all(&path)?;
                if verbose {
                    eprintln!("{name}");
                }
            }
            b'2' => {
                let link = cstr(&hdr[157..257]);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let _ = fs::remove_file(&path);
                std::os::unix::fs::symlink(&link, &path)?;
                if verbose {
                    eprintln!("{name}");
                }
            }
            _ => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut f = File::create(&path)?;
                let mut left = size;
                let mut buf = [0u8; 8192];
                while left > 0 {
                    let n = (left as usize).min(buf.len());
                    let r = input.read(&mut buf[..n])?;
                    if r == 0 {
                        break;
                    }
                    f.write_all(&buf[..r])?;
                    left -= r as u64;
                }
                let pad = (512 - (size % 512) as usize) % 512;
                if pad > 0 {
                    io::copy(&mut input.by_ref().take(pad as u64), &mut io::sink())?;
                }
                if verbose {
                    eprintln!("{name}");
                }
            }
        }
    }
    Ok(())
}

/// Decode a NUL/space-terminated (or full-width) string field from a ustar
/// header block.
fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// Decode an octal-ASCII numeric ustar header field (e.g. the size field).
/// Unlike a `.unwrap_or(0)` fallback, a malformed field is reported as an
/// error instead of being silently treated as size zero — which would
/// otherwise desynchronize block-skipping and corrupt the rest of the
/// archive read without any indication to the user.
fn oct(b: &[u8]) -> io::Result<u64> {
    let s = cstr(b);
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 8).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("corrupt archive: invalid octal field {trimmed:?}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oct_parses_valid_field() {
        assert_eq!(oct(b"00000000644\0").unwrap(), 0o644);
    }

    #[test]
    fn oct_empty_field_is_zero() {
        assert_eq!(oct(b"\0\0\0\0").unwrap(), 0);
        assert_eq!(oct(b"           \0").unwrap(), 0);
    }

    #[test]
    fn oct_rejects_corrupt_field_instead_of_defaulting() {
        // Regression: previously `unwrap_or(0)` silently treated a
        // corrupt size field as zero, desynchronizing the rest of the
        // archive read instead of surfacing the corruption.
        assert!(oct(b"not-octal!!\0").is_err());
        assert!(oct(b"999999999999\0").is_err());
    }

    #[test]
    fn cstr_stops_at_first_nul() {
        assert_eq!(cstr(b"hello\0world"), "hello");
    }

    #[test]
    fn cstr_handles_no_nul() {
        assert_eq!(cstr(b"nopadding"), "nopadding");
    }

    #[test]
    fn put_truncates_to_destination_length() {
        let mut dst = [0u8; 4];
        put(&mut dst, b"abcdefgh");
        assert_eq!(&dst, b"abcd");
    }

    #[test]
    fn round_trip_create_list_extract() {
        let root =
            std::env::temp_dir().join(format!("user_tar_test_{}_roundtrip", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("hello.txt"), b"hello world").unwrap();
        let archive = root.join("out.tar");

        create_tar(&archive, std::slice::from_ref(&src_dir), false).unwrap();
        assert!(archive.exists());

        list_tar(&archive, false).unwrap();

        let extract_dir = root.join("extracted");
        fs::create_dir_all(&extract_dir).unwrap();
        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&extract_dir).unwrap();
        let result = extract_tar(&archive, false);
        std::env::set_current_dir(orig_dir).unwrap();
        result.unwrap();

        let extracted_file = extract_dir.join("src/hello.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read(&extracted_file).unwrap(), b"hello world");

        let _ = fs::remove_dir_all(&root);
    }
}
