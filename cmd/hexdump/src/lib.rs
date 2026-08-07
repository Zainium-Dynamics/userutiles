//! user hexdump — display file contents in hexadecimal, decimal, octal, or
//! ASCII.
//!
//! Supports the fixed-format flags of util-linux's `hexdump`
//! (`-b -X -c -C -d -o -x`, plus `-n`/`-s`/`-v`); it does not implement the
//! `-e`/`-f` custom format-string mini-language.
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};

use usercore::Ui;

/// Which fixed on-screen layout a `-b/-X/-c/-C/-d/-o/-x` flag selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayFormat {
    /// `-C`: hex + ASCII display.
    Canonical,
    /// `-b`: one-byte octal display.
    OneByteOctal,
    /// `-X`: one-byte hex display.
    OneByteHex,
    /// `-c`: one-byte character display.
    OneByteChar,
    /// `-d`: two-byte decimal display.
    TwoBytesDecimal,
    /// `-o`: two-byte octal display.
    TwoBytesOctal,
    /// `-x`: two-byte hex display.
    TwoBytesHex,
    /// No format flag given: two-byte hex display with compact spacing.
    TwoBytesHexDefault,
}

/// Parsed, validated `hexdump` invocation.
#[derive(Debug)]
struct HexdumpOptions {
    formats: Vec<DisplayFormat>,
    length: Option<u64>,
    skip: u64,
    no_squeezing: bool,
    files: Vec<String>,
}

/// Reads a fixed-size window (bounded by an optional `length`) across a
/// sequence of files back to back, as if they were concatenated.
struct ChainedFileReader {
    file_paths: Vec<String>,
    current_file: Option<BufReader<File>>,
    current_file_index: usize,
    remaining_bytes: Option<u64>,
    open_error_count: usize,
    had_error: bool,
}

impl ChainedFileReader {
    fn new(file_paths: Vec<String>, length_limit: Option<u64>) -> Self {
        Self {
            file_paths,
            current_file: None,
            current_file_index: 0,
            remaining_bytes: length_limit,
            open_error_count: 0,
            had_error: false,
        }
    }

    fn ensure_current_file(&mut self) -> bool {
        if self.current_file.is_some() {
            return true;
        }

        while self.current_file_index < self.file_paths.len() {
            let file_path = &self.file_paths[self.current_file_index];
            match File::open(file_path) {
                Ok(file) => {
                    self.current_file = Some(BufReader::new(file));
                    return true;
                }
                Err(e) => {
                    show_error(&format!("cannot open '{file_path}': {e}"));
                    self.open_error_count += 1;
                    self.had_error = true;
                    self.current_file_index += 1;
                }
            }
        }
        false
    }

    fn current_file(&mut self) -> &mut BufReader<File> {
        self.current_file.as_mut().unwrap()
    }

    /// Fill `buf` from the chained files; callers assume partial reads
    /// won't happen except at end-of-input (matching the original).
    fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.remaining_bytes == Some(0) {
            return 0;
        }

        let mut offset = 0;
        while self.ensure_current_file() {
            let remaining_in_buffer = (buf.len() - offset) as u64;
            let nbytes = remaining_in_buffer.min(self.remaining_bytes.unwrap_or(u64::MAX)) as usize;
            if nbytes == 0 {
                break;
            }

            match self.current_file().read(&mut buf[offset..offset + nbytes]) {
                Ok(0) => {
                    self.current_file = None;
                    self.current_file_index += 1;
                }
                Ok(n) => {
                    offset += n;
                    self.remaining_bytes = self.remaining_bytes.map(|x| x - n as u64);
                }
                Err(e) => {
                    show_error(&format!(
                        "cannot read '{}': {e}",
                        self.file_paths[self.current_file_index]
                    ));
                    self.had_error = true;
                    self.current_file = None;
                    self.current_file_index += 1;
                }
            }
        }

        offset
    }

    fn skip_bytes(&mut self, bytes_to_skip: u64) {
        let mut remaining = bytes_to_skip;

        while remaining > 0 && self.ensure_current_file() {
            match self.current_file().seek(SeekFrom::End(0)) {
                Ok(file_size) => {
                    if remaining >= file_size {
                        remaining -= file_size;
                        self.current_file = None;
                        self.current_file_index += 1;
                    } else {
                        match self.current_file().seek(SeekFrom::Start(remaining)) {
                            Ok(_) => return,
                            Err(e) => {
                                show_error(&format!(
                                    "cannot seek '{}': {e}",
                                    self.file_paths[self.current_file_index]
                                ));
                                self.had_error = true;
                                self.current_file = None;
                                self.current_file_index += 1;
                            }
                        }
                    }
                }
                Err(_) => {
                    // Not seekable (e.g. a pipe) — skip via dummy reads.
                    while remaining > 0 {
                        let mut dummy_buf = vec![0u8; remaining.min(65536) as usize];
                        match self.current_file().read(&mut dummy_buf) {
                            Ok(0) => {
                                self.current_file = None;
                                self.current_file_index += 1;
                                break;
                            }
                            Ok(n) => remaining -= n as u64,
                            Err(e) => {
                                show_error(&format!(
                                    "cannot read '{}': {e}",
                                    self.file_paths[self.current_file_index]
                                ));
                                self.had_error = true;
                                self.current_file = None;
                                self.current_file_index += 1;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn show_error(msg: &str) {
    Ui::new("hexdump").err(msg);
}

const HELP: &str = "Usage: hexdump [options] <file>...\n\
Display file contents in hexadecimal, decimal, octal, or ascii.\n\n\
  -b, --one-byte-octal      one-byte octal display\n\
  -X, --one-byte-hex        one-byte hexadecimal display\n\
  -c, --one-byte-char       one-byte character display\n\
  -C, --canonical           canonical hex+ASCII display\n\
  -d, --two-bytes-decimal   two-byte decimal display\n\
  -o, --two-bytes-octal     two-byte octal display\n\
  -x, --two-bytes-hex       two-byte hexadecimal display\n\
  -n, --length LENGTH       interpret only LENGTH bytes of input\n\
  -s, --skip OFFSET         skip OFFSET bytes from the beginning\n\
  -v, --no-squeezing        output identical lines (don't squeeze with '*')\n\
  -h, --help                 display this help and exit\n\
      --version              output version information and exit\n";

/// Parse a byte-count argument (`-n`/`-s`), matching the exact grammar
/// `uucore::parser::parse_size::parse_size_u64` accepts with its default
/// settings (no allow-list, no `b`-as-byte-count, `B` alone not treated as
/// bytes) — verified field-by-field against `uucore` 0.2.2's source
/// (`Parser::parse`) after this crate's earlier port only supported
/// `K`/`M`/`G` binary suffixes and rejected the full `T/P/E/Z/Y/R/Q` range,
/// the `KB`/`MB`/... (decimal, 1000-based) family, the `b` (block, 512)
/// suffix, and octal input. The one deliberately unimplemented case is the
/// `%` suffix (a fraction of total physical memory, read from
/// `/proc/meminfo`) — not meaningful for a hexdump length/skip argument,
/// and real-world use of `-n`/`-s` with a percentage is not something this
/// tool's users would plausibly reach for.
fn parse_size_u64(s: &str) -> Result<u64, String> {
    let err = || format!("invalid number '{s}'");
    if s.is_empty() {
        return Err(err());
    }

    let is_hex = s.starts_with("0x");
    let is_octal = !is_hex
        && s.starts_with('0')
        && s.chars().take_while(char::is_ascii_digit).count() > 1
        && !s.chars().all(|c| c == '0');

    let numeric_len = if is_hex {
        2 + s[2..].chars().take_while(char::is_ascii_hexdigit).count()
    } else {
        s.chars().take_while(char::is_ascii_digit).count()
    };
    let (numeric, unit) = s.split_at(numeric_len);

    let (base, exponent): (u64, u32) = match unit {
        "" => (1, 0),
        "b" => (512, 1),
        "KiB" | "kiB" | "K" | "k" => (1024, 1),
        "MiB" | "miB" | "M" | "m" => (1024, 2),
        "GiB" | "giB" | "G" | "g" => (1024, 3),
        "TiB" | "tiB" | "T" | "t" => (1024, 4),
        "PiB" | "piB" | "P" | "p" => (1024, 5),
        "EiB" | "eiB" | "E" | "e" => (1024, 6),
        "ZiB" | "ziB" | "Z" | "z" => (1024, 7),
        "YiB" | "yiB" | "Y" | "y" => (1024, 8),
        "RiB" | "riB" | "R" | "r" => (1024, 9),
        "QiB" | "qiB" | "Q" | "q" => (1024, 10),
        "KB" | "kB" => (1000, 1),
        "MB" | "mB" => (1000, 2),
        "GB" | "gB" => (1000, 3),
        "TB" | "tB" => (1000, 4),
        "PB" | "pB" => (1000, 5),
        "EB" | "eB" => (1000, 6),
        "ZB" | "zB" => (1000, 7),
        "YB" | "yB" => (1000, 8),
        "RB" | "rB" => (1000, 9),
        "QB" | "qB" => (1000, 10),
        _ => return Err(err()),
    };
    let factor = base.checked_pow(exponent).ok_or_else(err)?;

    let number: u64 = if is_hex {
        u64::from_str_radix(&numeric[2..], 16).map_err(|_| err())?
    } else if is_octal {
        u64::from_str_radix(numeric.trim_start_matches('0'), 8).map_err(|_| err())?
    } else if numeric.is_empty() {
        1 // e.g. bare "K" means one of that unit, matching the reference.
    } else {
        numeric.parse().map_err(|_| err())?
    };

    number.checked_mul(factor).ok_or_else(err)
}

/// Parse `hexdump`'s options out of `args` (already stripped of `argv[0]`,
/// `--help`/`--version` handled by the caller).
fn parse_args(args: &[String]) -> Result<HexdumpOptions, String> {
    let mut formats: Vec<DisplayFormat> = Vec::new();
    let mut length: Option<u64> = None;
    let mut skip: u64 = 0;
    let mut no_squeezing = false;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-b" | "--one-byte-octal" => formats.push(DisplayFormat::OneByteOctal),
            "-X" | "--one-byte-hex" => formats.push(DisplayFormat::OneByteHex),
            "-c" | "--one-byte-char" => formats.push(DisplayFormat::OneByteChar),
            "-C" | "--canonical" => formats.push(DisplayFormat::Canonical),
            "-d" | "--two-bytes-decimal" => formats.push(DisplayFormat::TwoBytesDecimal),
            "-o" | "--two-bytes-octal" => formats.push(DisplayFormat::TwoBytesOctal),
            "-x" | "--two-bytes-hex" => formats.push(DisplayFormat::TwoBytesHex),
            "-v" | "--no-squeezing" => no_squeezing = true,
            "-n" | "--length" => {
                i += 1;
                let v = args.get(i).ok_or("option '-n' requires an argument")?;
                length = Some(parse_size_u64(v)?);
            }
            s if s.starts_with("--length=") => {
                length = Some(parse_size_u64(&s["--length=".len()..])?);
            }
            s if s.starts_with("-n") && s.len() > 2 => {
                length = Some(parse_size_u64(&s[2..])?);
            }
            "-s" | "--skip" => {
                i += 1;
                let v = args.get(i).ok_or("option '-s' requires an argument")?;
                skip = parse_size_u64(v)?;
            }
            s if s.starts_with("--skip=") => {
                skip = parse_size_u64(&s["--skip=".len()..])?;
            }
            s if s.starts_with("-s") && s.len() > 2 => {
                skip = parse_size_u64(&s[2..])?;
            }
            "--" => {
                files.extend(args[i + 1..].iter().cloned());
                break;
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("invalid option -- '{s}'"));
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }

    if formats.is_empty() {
        formats.push(DisplayFormat::TwoBytesHexDefault);
    }
    if files.is_empty() {
        files.push("/dev/stdin".to_string());
    }

    Ok(HexdumpOptions {
        formats,
        length,
        skip,
        no_squeezing,
        files,
    })
}

/// Entry point: parse `std::env::args()`, dump the requested files.
/// Returns 0 on success, 1 if every input file failed to open or any I/O
/// error occurred while reading, 2 on a bad argument.
pub fn run() -> i32 {
    let ui = Ui::new("hexdump");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("hexdump (user_utils) 0.1.0");
        return 0;
    }

    let options = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            ui.err(&e);
            return 2;
        }
    };

    run_hexdump(&options)
}

fn run_hexdump(options: &HexdumpOptions) -> i32 {
    let mut reader = ChainedFileReader::new(options.files.clone(), options.length);
    reader.skip_bytes(options.skip);

    let mut offset = options.skip;
    let mut last_line: Vec<u8> = Vec::new();
    let mut squeezing = false;
    let mut out = String::new();

    loop {
        let mut line_data = [0u8; 16];
        let bytes_read = reader.read(&mut line_data);
        if bytes_read == 0 {
            break;
        }
        let line_data = &line_data[..bytes_read];

        // Consolidate runs of identical 16-byte lines into a single '*'.
        if !options.no_squeezing && last_line == line_data {
            if !squeezing {
                out.push_str("*\n");
                squeezing = true;
            }
        } else {
            for format in &options.formats {
                print_hexdump_line(*format, offset, line_data, &mut out);
            }
            last_line.clear();
            last_line.extend_from_slice(line_data);
            squeezing = false;
        }

        offset += line_data.len() as u64;
    }

    if offset != 0 {
        // The trailing offset line's formatting must match the last
        // requested format.
        print_offset(offset, *options.formats.last().unwrap(), &mut out);
        out.push('\n');
    }

    print!("{out}");

    if reader.open_error_count == reader.file_paths.len() {
        Ui::new("hexdump").err("all input file arguments failed");
        1
    } else if reader.had_error {
        1
    } else {
        0
    }
}

fn print_hexdump_line(format: DisplayFormat, offset: u64, line_data: &[u8], out: &mut String) {
    print_offset(offset, format, out);
    match format {
        DisplayFormat::Canonical => print_canonical(line_data, out),
        DisplayFormat::OneByteOctal => {
            print_bytes(line_data, out, |b, o| o.push_str(&format!(" {b:03o}")))
        }
        DisplayFormat::OneByteHex => {
            print_bytes(line_data, out, |b, o| o.push_str(&format!("  {b:02x}")))
        }
        DisplayFormat::OneByteChar => print_bytes(line_data, out, print_char_byte),
        DisplayFormat::TwoBytesDecimal => {
            print_words(line_data, 8, out, |w, o| o.push_str(&format!("   {w:05}")))
        }
        DisplayFormat::TwoBytesOctal => {
            print_words(line_data, 8, out, |w, o| o.push_str(&format!("  {w:06o}")))
        }
        DisplayFormat::TwoBytesHex => print_words(line_data, 8, out, |w, o| {
            o.push_str(&format!("    {w:04x}"))
        }),
        DisplayFormat::TwoBytesHexDefault => {
            print_words(line_data, 5, out, |w, o| o.push_str(&format!(" {w:04x}")))
        }
    }
}

fn print_offset(offset: u64, format: DisplayFormat, out: &mut String) {
    if format == DisplayFormat::Canonical {
        out.push_str(&format!("{offset:08x}"));
    } else {
        out.push_str(&format!("{offset:07x}"));
    }
}

fn print_canonical(line_data: &[u8], out: &mut String) {
    out.push_str("  ");

    for i in 0..16 {
        if i == 8 {
            out.push(' ');
        }
        if i < line_data.len() {
            out.push_str(&format!("{:02x} ", line_data[i]));
        } else {
            out.push_str("   ");
        }
    }

    out.push_str(" |");
    for &byte in line_data {
        if byte.is_ascii_graphic() || byte == b' ' {
            out.push(byte as char);
        } else {
            out.push('.');
        }
    }
    out.push_str("|\n");
}

fn print_bytes<F>(line_data: &[u8], out: &mut String, byte_printer: F)
where
    F: Fn(u8, &mut String),
{
    for &byte in line_data {
        byte_printer(byte, out);
    }
    // Pad every line to the same length, matching the original hexdump.
    out.push_str(&format!(
        "{:width$}\n",
        "",
        width = (16 - line_data.len()) * 4
    ));
}

fn print_char_byte(byte: u8, out: &mut String) {
    match byte {
        b'\0' => out.push_str("  \\0"),
        b'\x07' => out.push_str("  \\a"),
        b'\x08' => out.push_str("  \\b"),
        b'\t' => out.push_str("  \\t"),
        b'\n' => out.push_str("  \\n"),
        b'\x0B' => out.push_str("  \\v"),
        b'\x0C' => out.push_str("  \\f"),
        b'\r' => out.push_str("  \\r"),
        b if b.is_ascii_graphic() || b == b' ' => out.push_str(&format!("   {}", b as char)),
        b => out.push_str(&format!(" {b:03o}")),
    }
}

fn print_words<F>(line_data: &[u8], chars_per_word: usize, out: &mut String, word_printer: F)
where
    F: Fn(u16, &mut String),
{
    for i in 0..(line_data.len() / 2) {
        word_printer(
            u16::from_le_bytes([line_data[i * 2], line_data[i * 2 + 1]]),
            out,
        );
    }

    if line_data.len() % 2 == 1 {
        word_printer(*line_data.last().unwrap() as u16, out);
    }

    // Pad every line to the same length, matching the original hexdump.
    // (Manual ceil-div: `div_ceil` isn't available until Rust 1.73, but
    // this crate targets rust-version 1.70.)
    let word_count = (line_data.len() + 1) / 2;
    out.push_str(&format!(
        "{:width$}\n",
        "",
        width = (8 - word_count) * chars_per_word
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(bytes: &[u8], options: &HexdumpOptions) -> String {
        // Route through the same line-formatting path `run_hexdump` uses,
        // but purely in-memory: capture into a String rather than
        // touching stdin/files.
        let mut out = String::new();
        let mut offset: u64 = options.skip;
        let mut last_line: Vec<u8> = Vec::new();
        let mut squeezing = false;
        for chunk in bytes.chunks(16) {
            if !options.no_squeezing && last_line == chunk {
                if !squeezing {
                    out.push_str("*\n");
                    squeezing = true;
                }
            } else {
                for format in &options.formats {
                    print_hexdump_line(*format, offset, chunk, &mut out);
                }
                last_line.clear();
                last_line.extend_from_slice(chunk);
                squeezing = false;
            }
            offset += chunk.len() as u64;
        }
        if offset != options.skip || !bytes.is_empty() {
            print_offset(offset, *options.formats.last().unwrap(), &mut out);
            out.push('\n');
        }
        out
    }

    fn default_options() -> HexdumpOptions {
        HexdumpOptions {
            formats: vec![DisplayFormat::TwoBytesHexDefault],
            length: None,
            skip: 0,
            no_squeezing: false,
            files: vec![],
        }
    }

    #[test]
    fn default_format_known_bytes() {
        // 16 bytes 0x00..0x0f, default two-byte-hex-compact format.
        let bytes: Vec<u8> = (0u8..16).collect();
        let out = dump(&bytes, &default_options());
        assert_eq!(
            out,
            "0000000 0100 0302 0504 0706 0908 0b0a 0d0c 0f0e\n0000010\n"
        );
    }

    #[test]
    fn canonical_format_known_bytes() {
        let bytes: Vec<u8> = b"Hello, World!!!!".to_vec(); // exactly 16 bytes
        let mut options = default_options();
        options.formats = vec![DisplayFormat::Canonical];
        let out = dump(&bytes, &options);
        assert_eq!(
            out,
            "00000000  48 65 6c 6c 6f 2c 20 57  6f 72 6c 64 21 21 21 21  |Hello, World!!!!|\n00000010\n"
        );
    }

    #[test]
    fn one_byte_hex_format() {
        let bytes = vec![0x00u8, 0xff, 0x41];
        let mut options = default_options();
        options.formats = vec![DisplayFormat::OneByteHex];
        let out = dump(&bytes, &options);
        assert_eq!(
            out,
            "0000000  00  ff  41                                                    \n0000003\n"
        );
    }

    #[test]
    fn one_byte_char_format_escapes_control_bytes() {
        let bytes = vec![b'A', b'\n', 0u8];
        let mut options = default_options();
        options.formats = vec![DisplayFormat::OneByteChar];
        let out = dump(&bytes, &options);
        assert_eq!(
            out,
            "0000000   A  \\n  \\0                                                    \n0000003\n"
        );
    }

    #[test]
    fn empty_input_prints_nothing() {
        let out = dump(&[], &default_options());
        assert_eq!(out, "");
    }

    #[test]
    fn identical_lines_are_squeezed_by_default() {
        // Three identical 16-byte lines followed by a different one.
        let mut bytes = vec![0xAAu8; 16 * 3];
        bytes.extend_from_slice(&[0xBBu8; 4]);
        let out = dump(&bytes, &default_options());
        // Expect: line 1 in full, then '*' once (not three times), then
        // the final differing partial line, then the trailing offset.
        assert_eq!(out.matches('*').count(), 1);
        let first_line_dump = {
            let mut o = String::new();
            print_hexdump_line(DisplayFormat::TwoBytesHexDefault, 0, &[0xAAu8; 16], &mut o);
            o
        };
        assert!(out.starts_with(&first_line_dump));
        assert!(out.contains("*\n"));
    }

    #[test]
    fn no_squeezing_flag_repeats_identical_lines() {
        let bytes = vec![0xAAu8; 16 * 3];
        let mut options = default_options();
        options.no_squeezing = true;
        let out = dump(&bytes, &options);
        assert_eq!(out.matches('*').count(), 0);
        // Each of the 3 identical lines should appear (offset differs).
        assert!(out.contains("0000000"));
        assert!(out.contains("0000010"));
        assert!(out.contains("0000020"));
    }

    #[test]
    fn multiple_formats_print_each_per_line() {
        let bytes = vec![0x41u8; 4];
        let mut options = default_options();
        options.formats = vec![DisplayFormat::OneByteHex, DisplayFormat::Canonical];
        let out = dump(&bytes, &options);
        let lines: Vec<&str> = out.lines().collect();
        // First line rendered twice (once per format) plus final offset line.
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("0000000"));
        assert!(lines[1].starts_with("00000000"));
    }

    #[test]
    fn parse_args_defaults_to_stdin_and_compact_hex() {
        let options = parse_args(&[]).unwrap();
        assert_eq!(options.files, vec!["/dev/stdin".to_string()]);
        assert_eq!(options.formats, vec![DisplayFormat::TwoBytesHexDefault]);
        assert_eq!(options.skip, 0);
        assert!(options.length.is_none());
    }

    #[test]
    fn parse_args_collects_multiple_formats_in_order() {
        let options = parse_args(&["-x".to_string(), "-C".to_string()]).unwrap();
        assert_eq!(
            options.formats,
            vec![DisplayFormat::TwoBytesHex, DisplayFormat::Canonical]
        );
    }

    #[test]
    fn parse_args_length_and_skip() {
        let options = parse_args(&[
            "-s".to_string(),
            "16".to_string(),
            "-n".to_string(),
            "32".to_string(),
            "file.bin".to_string(),
        ])
        .unwrap();
        assert_eq!(options.skip, 16);
        assert_eq!(options.length, Some(32));
        assert_eq!(options.files, vec!["file.bin".to_string()]);
    }

    #[test]
    fn parse_size_u64_hex_and_suffixes() {
        assert_eq!(parse_size_u64("0x10").unwrap(), 16);
        assert_eq!(parse_size_u64("1K").unwrap(), 1024);
        assert_eq!(parse_size_u64("10").unwrap(), 10);
        assert!(parse_size_u64("bogus").is_err());
    }

    /// Regression tests for the `uucore::parser::parse_size::parse_size_u64`
    /// grammar-parity gap flagged in `checklist/hexdump.md`: verifies the
    /// binary (`KiB`) family, the decimal/SI (`KB`) family, the full
    /// `T/P/E/Z/Y/R/Q` range, the `b` (block=512) suffix, octal input, and
    /// the specific case-sensitivity rules (`"K"`/`"k"` both bind to 1024,
    /// but only `"KB"`/`"kB"` — not `"Kb"`/`"kb"` — bind to 1000), all
    /// checked directly against `uucore` 0.2.2's own test expectations.
    #[test]
    fn parse_size_u64_matches_uucore_default_parser_grammar() {
        // Binary (KiB) family + bare unit means "1 of that unit".
        assert_eq!(parse_size_u64("K").unwrap(), 1024);
        assert_eq!(parse_size_u64("k").unwrap(), 1024);
        assert_eq!(parse_size_u64("2K").unwrap(), 2048);
        assert_eq!(parse_size_u64("2KiB").unwrap(), 2048);
        assert_eq!(parse_size_u64("2kiB").unwrap(), 2048);

        // Decimal/SI (KB) family — was entirely unsupported before this fix.
        assert_eq!(parse_size_u64("9kB").unwrap(), 9000);
        assert_eq!(parse_size_u64("123KB").unwrap(), 123_000);
        assert_eq!(parse_size_u64("KB").unwrap(), 1000);

        // Full unit range up to Q (was capped at G before this fix).
        assert_eq!(parse_size_u64("1T").unwrap(), 1_099_511_627_776);
        assert_eq!(parse_size_u64("1P").unwrap(), 1_125_899_906_842_624);
        assert_eq!(parse_size_u64("2TB").unwrap(), 2_000_000_000_000);

        // Block suffix (was entirely unsupported before this fix).
        assert_eq!(parse_size_u64("3b").unwrap(), 3 * 512);

        // Octal input (was entirely unsupported before this fix).
        assert_eq!(parse_size_u64("077").unwrap(), 63);
        assert_eq!(parse_size_u64("01234K").unwrap(), 668 * 1024);

        // Case sensitivity: "Kb"/"kb" are not valid units (only a
        // lowercase-only "b" suffix or an exact "KB"/"kB" pair are).
        assert!(parse_size_u64("5Kb").is_err());
        assert!(parse_size_u64("5kb").is_err());

        // Bare "B" is not a valid suffix under the reference's default
        // settings (hexdump doesn't opt into `capital_b_bytes`).
        assert!(parse_size_u64("5B").is_err());

        // Overflow must error (not silently saturate, which the pre-fix
        // implementation did via `saturating_mul`).
        assert!(parse_size_u64("100000P").is_err());
        assert!(parse_size_u64("1Y").is_err());

        assert!(parse_size_u64("").is_err());
    }

    #[test]
    fn parse_args_unknown_option_errors() {
        assert!(parse_args(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn chained_file_reader_reads_across_two_files() {
        let dir = std::env::temp_dir().join(format!("user_hexdump_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f1 = dir.join("a.bin");
        let f2 = dir.join("b.bin");
        std::fs::write(&f1, [1u8, 2, 3]).unwrap();
        std::fs::write(&f2, [4u8, 5, 6]).unwrap();

        let mut reader = ChainedFileReader::new(
            vec![
                f1.to_string_lossy().into_owned(),
                f2.to_string_lossy().into_owned(),
            ],
            None,
        );
        let mut buf = [0u8; 16];
        let n = reader.read(&mut buf);
        assert_eq!(n, 6);
        assert_eq!(&buf[..6], &[1, 2, 3, 4, 5, 6]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn chained_file_reader_reports_open_errors() {
        let mut reader =
            ChainedFileReader::new(vec!["/no/such/file-user-hexdump".to_string()], None);
        let mut buf = [0u8; 16];
        let n = reader.read(&mut buf);
        assert_eq!(n, 0);
        assert_eq!(reader.open_error_count, 1);
        assert!(reader.had_error);
    }
}
