//! user truncate — shrink or extend file size.
use std::fs::OpenOptions;
use std::os::unix::fs::MetadataExt;

use usercore::Ui;

/// A parsed `-s`/`--size` argument: either an absolute target size, or a
/// size to add to / subtract from the file's current size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SizeSpec {
    Absolute(u64),
    RelPlus(u64),
    RelMinus(u64),
}

/// Entry point for the `truncate` utility. Parses `std::env::args()` and
/// shrinks or extends each `FILE` to the size given by `-s`/`--size`
/// (absolute, or relative via a leading `+`/`-`) or `-r`/`--reference`.
///
/// Returns 0 on success, 1 if any file could not be resolved or resized.
pub fn run() -> i32 {
    let ui = Ui::new("truncate");
    let mut size: Option<SizeSpec> = None;
    let mut reference: Option<String> = None;
    let mut io_blocks = false;
    let mut create = true;
    let mut files: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: truncate OPTION... FILE...\n\
Shrink or extend the size of each FILE.\n\
  -c, --no-create      do not create any files\n\
  -o, --io-blocks      treat SIZE as number of IO blocks\n\
  -r, --reference=RFILE  base size on RFILE\n\
  -s, --size=SIZE      set or adjust the file size to SIZE\n\
SIZE may be prefixed with + or - to adjust relative size.\n"
                );
                return 0;
            }
            "--version" => {
                println!("truncate (user_utils) 0.1.0");
                return 0;
            }
            "-c" | "--no-create" => create = false,
            "-o" | "--io-blocks" => io_blocks = true,
            "-s" | "--size" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 's'");
                    return 1;
                };
                match parse_size_spec(arg) {
                    Ok(s) => size = Some(s),
                    Err(e) => {
                        ui.err(&e);
                        return 1;
                    }
                }
            }
            "-r" | "--reference" => {
                i += 1;
                let Some(arg) = args.get(i) else {
                    ui.err("option requires an argument -- 'r'");
                    return 1;
                };
                reference = Some(arg.clone());
            }
            s if s.starts_with("--size=") => match parse_size_spec(&s["--size=".len()..]) {
                Ok(v) => size = Some(v),
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with("--reference=") => {
                reference = Some(s["--reference=".len()..].to_string());
            }
            s if s.starts_with("-s") && s.len() > 2 => match parse_size_spec(&s[2..]) {
                Ok(v) => size = Some(v),
                Err(e) => {
                    ui.err(&e);
                    return 1;
                }
            },
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }
    if size.is_none() && reference.is_none() {
        ui.err("you must specify either '--size' or '--reference'");
        return 1;
    }
    if files.is_empty() {
        ui.err("missing file operand");
        return 1;
    }

    let mut status = 0;
    for f in &files {
        match resize_one(f, size, reference.as_deref(), io_blocks, create) {
            Ok(()) => {}
            Err(e) => {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
    }
    status
}

/// Resize a single file per the resolved `size`/`reference`/`io_blocks`
/// options. Returns `Ok(())` without creating the file if it is missing
/// and `create` is `false`.
fn resize_one(
    path: &str,
    size: Option<SizeSpec>,
    reference: Option<&str>,
    io_blocks: bool,
    create: bool,
) -> Result<(), String> {
    let meta = std::fs::metadata(path);
    if meta.is_err() && !create {
        return Ok(());
    }
    let cur = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let blksize = meta
        .as_ref()
        .map(|m| m.blksize().max(1))
        .unwrap_or(4096);

    let new_len = match reference {
        Some(r) => {
            let rmeta = std::fs::metadata(r).map_err(|e| format!("{r}: {e}"))?;
            match size {
                // GNU: SIZE together with --reference adjusts relative to
                // the reference file's size.
                Some(s) => apply_spec(rmeta.len(), s, io_blocks, blksize),
                None => rmeta.len(),
            }
        }
        None => {
            let Some(s) = size else {
                return Err("no size specified".to_string());
            };
            apply_spec(cur, s, io_blocks, blksize)
        }
    };

    let mut opts = OpenOptions::new();
    opts.write(true);
    if create {
        opts.create(true);
    }
    let fh = opts.open(path).map_err(|e| e.to_string())?;
    fh.set_len(new_len).map_err(|e| e.to_string())
}

/// Apply a `SizeSpec` against a base length, optionally scaling by the
/// filesystem's IO block size (`-o`/`--io-blocks`). Uses saturating
/// arithmetic throughout since sizes come from untrusted CLI input.
fn apply_spec(base: u64, spec: SizeSpec, io_blocks: bool, blksize: u64) -> u64 {
    let scale = |n: u64| if io_blocks { n.saturating_mul(blksize) } else { n };
    match spec {
        SizeSpec::Absolute(n) => scale(n),
        SizeSpec::RelPlus(n) => base.saturating_add(scale(n)),
        SizeSpec::RelMinus(n) => base.saturating_sub(scale(n)),
    }
}

/// Parse a `-s`/`--size` argument, e.g. `10K`, `+1M`, `-512`, into a
/// [`SizeSpec`]. Returns `Err` (rather than silently defaulting) on any
/// malformed number or unit suffix.
fn parse_size_spec(s: &str) -> Result<SizeSpec, String> {
    if let Some(rest) = s.strip_prefix('+') {
        return Ok(SizeSpec::RelPlus(parse_bytes(rest)?));
    }
    if let Some(rest) = s.strip_prefix('-') {
        return Ok(SizeSpec::RelMinus(parse_bytes(rest)?));
    }
    Ok(SizeSpec::Absolute(parse_bytes(s)?))
}

/// Parse a byte count with an optional `K`/`M`/`G`/`T`/`P` (binary,
/// 1024-based) or `B`/`C` (no-op) suffix. Errors on empty input, a
/// non-numeric magnitude, or an unrecognized suffix rather than silently
/// treating the value as zero or unscaled.
fn parse_bytes(s: &str) -> Result<u64, String> {
    if s.is_empty() {
        return Err("invalid number: ''".to_string());
    }
    let last = s.as_bytes()[s.len() - 1];
    let (num_s, mult) = if last.is_ascii_digit() {
        (s, 1u64)
    } else {
        let mult = match last.to_ascii_uppercase() {
            b'K' => 1024,
            b'M' => 1024 * 1024,
            b'G' => 1024 * 1024 * 1024,
            b'T' => 1024u64.pow(4),
            b'P' => 1024u64.pow(5),
            b'B' | b'C' => 1,
            _ => return Err(format!("invalid number '{s}'")),
        };
        (&s[..s.len() - 1], mult)
    };
    let n: u64 = num_s.parse().map_err(|_| format!("invalid number '{s}'"))?;
    Ok(n.saturating_mul(mult))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_bytes_plain_and_suffixed() {
        assert_eq!(parse_bytes("100").unwrap(), 100);
        assert_eq!(parse_bytes("1K").unwrap(), 1024);
        assert_eq!(parse_bytes("1k").unwrap(), 1024);
        assert_eq!(parse_bytes("2M").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_bytes("1B").unwrap(), 1);
    }

    #[test]
    fn parse_bytes_rejects_garbage_instead_of_defaulting() {
        assert!(parse_bytes("").is_err());
        assert!(parse_bytes("abc").is_err());
        assert!(parse_bytes("12X").is_err());
        assert!(parse_bytes("--").is_err());
    }

    #[test]
    fn parse_size_spec_absolute_and_relative() {
        assert_eq!(parse_size_spec("10").unwrap(), SizeSpec::Absolute(10));
        assert_eq!(parse_size_spec("+10").unwrap(), SizeSpec::RelPlus(10));
        assert_eq!(parse_size_spec("-10").unwrap(), SizeSpec::RelMinus(10));
        assert_eq!(parse_size_spec("+1K").unwrap(), SizeSpec::RelPlus(1024));
    }

    #[test]
    fn parse_size_spec_invalid_errors() {
        assert!(parse_size_spec("+").is_err());
        assert!(parse_size_spec("nope").is_err());
    }

    #[test]
    fn apply_spec_absolute_and_relative_saturate() {
        assert_eq!(apply_spec(100, SizeSpec::Absolute(50), false, 4096), 50);
        assert_eq!(apply_spec(100, SizeSpec::RelPlus(50), false, 4096), 150);
        assert_eq!(apply_spec(100, SizeSpec::RelMinus(50), false, 4096), 50);
        // Underflow must saturate to 0, not panic or wrap.
        assert_eq!(apply_spec(10, SizeSpec::RelMinus(50), false, 4096), 0);
    }

    #[test]
    fn apply_spec_io_blocks_scales_by_blksize() {
        assert_eq!(apply_spec(0, SizeSpec::Absolute(2), true, 512), 1024);
        assert_eq!(apply_spec(1024, SizeSpec::RelPlus(1), true, 512), 1536);
    }

    #[test]
    fn resize_one_extends_and_shrinks_existing_file() {
        let dir = std::env::temp_dir().join(format!("user_truncate_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.txt");
        std::fs::write(&path, b"hello").unwrap();

        resize_one(
            path.to_str().unwrap(),
            Some(SizeSpec::Absolute(10)),
            None,
            false,
            true,
        )
        .unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 10);

        resize_one(
            path.to_str().unwrap(),
            Some(SizeSpec::Absolute(2)),
            None,
            false,
            true,
        )
        .unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_one_relative_plus_extends_from_current_size() {
        let dir = std::env::temp_dir().join(format!("user_truncate_test2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.txt");
        let mut fh = std::fs::File::create(&path).unwrap();
        fh.write_all(&[0u8; 5]).unwrap();
        drop(fh);

        resize_one(
            path.to_str().unwrap(),
            Some(SizeSpec::RelPlus(5)),
            None,
            false,
            true,
        )
        .unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 10);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_one_no_create_skips_missing_file() {
        let missing = format!(
            "/nonexistent_user_truncate_test_{}",
            std::process::id()
        );
        let r = resize_one(&missing, Some(SizeSpec::Absolute(5)), None, false, false);
        assert!(r.is_ok());
        assert!(std::fs::metadata(&missing).is_err());
    }

    #[test]
    fn resize_one_creates_missing_file_by_default() {
        let dir = std::env::temp_dir().join(format!("user_truncate_test3_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("new.txt");
        assert!(std::fs::metadata(&path).is_err());

        resize_one(
            path.to_str().unwrap(),
            Some(SizeSpec::Absolute(4)),
            None,
            false,
            true,
        )
        .unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 4);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_one_reference_sizes_to_reference_file() {
        let dir = std::env::temp_dir().join(format!("user_truncate_test4_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rfile = dir.join("ref.txt");
        std::fs::write(&rfile, [0u8; 7]).unwrap();
        let target = dir.join("target.txt");
        std::fs::write(&target, b"x").unwrap();

        resize_one(
            target.to_str().unwrap(),
            None,
            Some(rfile.to_str().unwrap()),
            false,
            true,
        )
        .unwrap();
        assert_eq!(std::fs::metadata(&target).unwrap().len(), 7);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_one_missing_reference_errors() {
        let dir = std::env::temp_dir().join(format!("user_truncate_test5_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.txt");
        std::fs::write(&target, b"x").unwrap();
        let missing_ref = dir.join("does_not_exist.txt");

        let r = resize_one(
            target.to_str().unwrap(),
            None,
            Some(missing_ref.to_str().unwrap()),
            false,
            true,
        );
        assert!(r.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
