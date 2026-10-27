//! user od — dump files in octal and other formats, with GNU od's options and layout.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Named,
    Char,
    Signed,
    Unsigned,
    Octal,
    Hex,
    Float,
}

#[derive(Clone, Copy, Debug)]
struct Spec {
    kind: Kind,
    size: usize,
    /// GNU's field width for this type; `pad` spreads the spare columns of a line
    width: usize,
    pad: usize,
    trailer: bool,
}

const CHARNAME: [&str; 33] = [
    "nul", "soh", "stx", "etx", "eot", "enq", "ack", "bel", "bs", "ht", "nl", "vt", "ff", "cr", "so", "si", "dle",
    "dc1", "dc2", "dc3", "dc4", "nak", "syn", "etb", "can", "em", "sub", "esc", "fs", "gs", "rs", "us", "sp",
];

struct Opts {
    radix: char,
    specs: Vec<Spec>,
    skip: u64,
    limit: Option<u64>,
    width: Option<usize>,
    verbose: bool,
    swap: bool,
    strings: Option<u64>,
}

/// Entry point for the `od` utility.
pub fn run() -> i32 {
    let mut o = Opts { radix: 'o', specs: Vec::new(), skip: 0, limit: None, width: None, verbose: false, swap: false, strings: None };
    let mut files: Vec<String> = Vec::new();
    let mut modern = false;
    let mut traditional = false;
    let fail = |m: &str| {
        eprintln!("od: {m}\nTry 'od --help' for more information.");
        1
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut opts_done = false;
    while i < args.len() {
        let a = args[i].clone();
        i += 1;
        if opts_done || a == "-" || !a.starts_with('-') {
            files.push(a);
            continue;
        }
        if let Some(long) = a.strip_prefix("--") {
            let (key, val) = match long.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (long, None),
            };
            let mut need = |name: &str| -> Result<String, i32> {
                match val.clone() {
                    Some(v) => Ok(v),
                    None if i < args.len() => {
                        i += 1;
                        Ok(args[i - 1].clone())
                    }
                    None => Err(fail(&format!("option '--{name}' requires an argument"))),
                }
            };
            let r: Result<(), String> = match key {
                "" => {
                    opts_done = true;
                    Ok(())
                }
                "help" => {
                    print!("{HELP}");
                    return 0;
                }
                "version" => {
                    println!("od (user_utils) 0.2.0");
                    return 0;
                }
                "address-radix" => match need(key) {
                    Ok(v) => set_radix(&mut o, &v),
                    Err(rc) => return rc,
                },
                "skip-bytes" => match need(key) {
                    Ok(v) => parse_size(&v).map(|n| o.skip = n),
                    Err(rc) => return rc,
                },
                "read-bytes" => match need(key) {
                    Ok(v) => parse_size(&v).map(|n| o.limit = Some(n)),
                    Err(rc) => return rc,
                },
                "format" => match need(key) {
                    Ok(v) => parse_type(&v, &mut o.specs),
                    Err(rc) => return rc,
                },
                "strings" => match &val {
                    Some(v) => parse_size(v).map(|n| o.strings = Some(n)),
                    None => {
                        o.strings = Some(3);
                        Ok(())
                    }
                },
                "width" => match &val {
                    Some(v) => parse_width(v).map(|n| o.width = Some(n)),
                    None => {
                        o.width = Some(32);
                        Ok(())
                    }
                },
                "output-duplicates" => {
                    o.verbose = true;
                    Ok(())
                }
                "traditional" => {
                    traditional = true;
                    Ok(())
                }
                "endian" => match need(key).as_deref() {
                    Ok("big") => {
                        o.swap = cfg!(target_endian = "little");
                        Ok(())
                    }
                    Ok("little") => {
                        o.swap = cfg!(target_endian = "big");
                        Ok(())
                    }
                    Ok(v) => Err(format!("invalid argument '{v}' for '--endian'")),
                    Err(rc) => return *rc,
                },
                _ => return fail(&format!("unrecognized option '{a}'")),
            };
            if let Err(e) = r {
                return fail(&e);
            }
            if !matches!(key, "traditional" | "") {
                modern = true;
            }
            continue;
        }

        let chars: Vec<char> = a[1..].chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            let rest: String = chars[k + 1..].iter().collect();
            if matches!(c, 'A' | 'j' | 'N' | 't' | 'S') {
                let v = if !rest.is_empty() {
                    rest
                } else if i < args.len() {
                    i += 1;
                    args[i - 1].clone()
                } else {
                    return fail(&format!("option requires an argument -- '{c}'"));
                };
                let r = match c {
                    'A' => set_radix(&mut o, &v),
                    'j' => parse_size(&v).map(|n| o.skip = n),
                    'N' => parse_size(&v).map(|n| o.limit = Some(n)),
                    't' => parse_type(&v, &mut o.specs),
                    _ => parse_size(&v).map(|n| o.strings = Some(n)),
                };
                if let Err(e) = r {
                    return fail(&e);
                }
                modern = true;
                break;
            }
            if c == 'w' {
                // the width is optional and only ever attached: -w, -w8
                o.width = Some(if rest.is_empty() {
                    32
                } else {
                    match parse_width(&rest) {
                        Ok(n) => n,
                        Err(e) => return fail(&e),
                    }
                });
                modern = true;
                break;
            }
            let old = match c {
                'a' => "a",
                'b' => "o1",
                'c' => "c",
                'D' => "u4",
                'd' => "u2",
                'e' | 'F' => "fD",
                'f' => "fF",
                'H' | 'X' => "x4",
                'i' => "dI",
                'I' | 'L' | 'l' => "dL",
                'O' => "o4",
                'o' | 'B' => "o2",
                's' => "d2",
                'x' | 'h' => "x2",
                'v' => {
                    o.verbose = true;
                    modern = true;
                    k += 1;
                    continue;
                }
                _ => return fail(&format!("invalid option -- '{c}'")),
            };
            if let Err(e) = parse_type(old, &mut o.specs) {
                return fail(&e);
            }
            k += 1;
        }
    }

    // POSIX: `od [FILE] [+]OFFSET` when only traditional options were used
    if !modern || traditional {
        let n = files.len();
        let last_is_offset = n >= 1
            && (traditional || files[n - 1].starts_with('+') || (n == 2 && files[n - 1].starts_with(|c: char| c.is_ascii_digit())));
        if last_is_offset {
            if let Some(off) = parse_old_offset(&files[n - 1]) {
                o.skip = off;
                files.pop();
            }
        }
    }

    if o.strings.is_some() && !o.specs.is_empty() {
        eprintln!("od: no type may be specified when dumping strings");
        return 1;
    }
    if o.specs.is_empty() {
        let _ = parse_type("o2", &mut o.specs);
    }
    if files.is_empty() {
        files.push("-".into());
    }

    let mut input = Input { files, idx: 0, cur: None, failed: false };
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if !input.skip(o.skip) {
        let _ = out.flush();
        eprintln!("od: cannot skip past end of combined input");
        return 1;
    }
    let r = match o.strings {
        Some(min) => dump_strings(&mut input, &o, min, &mut out),
        None => dump(&mut input, &mut o, &mut out),
    };
    let _ = out.flush();
    if r.is_err() || input.failed {
        1
    } else {
        0
    }
}

const HELP: &str = "Usage: od [OPTION]... [FILE]...\nWrite an unambiguous representation, octal bytes by default, of FILE to standard output.\n\n  -A, --address-radix=RADIX   offsets in d(ecimal), o(ctal), x (hex) or n(one)\n      --endian={big|little}   swap input bytes as needed\n  -j, --skip-bytes=BYTES      skip BYTES input bytes first\n  -N, --read-bytes=BYTES      limit dump to BYTES input bytes\n  -S BYTES, --strings[=BYTES] show only NUL-terminated strings of at least BYTES (3) graphic chars\n  -t, --format=TYPE           select output format or formats\n  -v, --output-duplicates     do not use * to mark line suppression\n  -w[BYTES], --width[=BYTES]  output BYTES bytes per output line (32 without a number)\n      --traditional           accept arguments in third form above\n\nTraditional format specifications may be intermixed:\n  -a   same as -t a,  select named characters, ignoring high-order bit\n  -b   same as -t o1, select octal bytes\n  -c   same as -t c,  select printable characters or backslash escapes\n  -d   same as -t u2, select unsigned decimal 2-byte units\n  -f   same as -t fF, select floats\n  -i   same as -t dI, select decimal ints\n  -l   same as -t dL, select decimal longs\n  -o   same as -t o2, select octal 2-byte units\n  -s   same as -t d2, select decimal 2-byte units\n  -x   same as -t x2, select hexadecimal 2-byte units\n\nTYPE is made up of one or more of these specifications:\n  a, c, d[SIZE], f[SIZE], o[SIZE], u[SIZE], x[SIZE]\nSIZE is a number of bytes (1, 2, 4, 8; f: 4, 8) or C, S, I, L (f: F, D).\nAdding a z suffix to any type displays printable characters at the end of each output line.\n";

fn set_radix(o: &mut Opts, v: &str) -> Result<(), String> {
    match v {
        "d" | "o" | "x" | "n" => {
            o.radix = v.chars().next().unwrap_or('o');
            Ok(())
        }
        _ => Err(format!("invalid output address radix '{v}'; it must be one character from [doxn]")),
    }
}

fn parse_width(v: &str) -> Result<usize, String> {
    match v.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("invalid -w argument '{v}'")),
    }
}

/// A byte count: decimal, 0x hex or 0 octal, with b (512), K/KiB (1024), KB (1000), M, G, ... suffixes.
fn parse_size(s: &str) -> Result<u64, String> {
    let bad = || format!("invalid number '{s}'");
    let (radix, body) = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, h)
    } else if s.len() > 1 && s.starts_with('0') && s[1..].starts_with(|c: char| c.is_ascii_digit()) {
        (8, &s[1..])
    } else {
        (10, s)
    };
    let end = body.find(|c: char| !c.is_digit(radix)).unwrap_or(body.len());
    let (num, suffix) = body.split_at(end);
    if num.is_empty() {
        return Err(bad());
    }
    let n = u64::from_str_radix(num, radix).map_err(|_| bad())?;
    let pow = |base: u64, e: u32| base.checked_pow(e);
    let mult = match suffix {
        "" => Some(1),
        "b" => Some(512),
        _ => {
            let mut cs = suffix.chars();
            let letter = cs.next().unwrap_or(' ');
            let e = match letter.to_ascii_uppercase() {
                'K' => 1,
                'M' => 2,
                'G' => 3,
                'T' => 4,
                'P' => 5,
                'E' => 6,
                _ => return Err(bad()),
            };
            match cs.as_str() {
                "" | "iB" => pow(1024, e),
                "B" => pow(1000, e),
                _ => return Err(bad()),
            }
        }
    };
    mult.and_then(|m| n.checked_mul(m)).ok_or_else(bad)
}

/// The traditional `[+]OFFSET[.][b]` operand: octal, or decimal with `.`, hex with 0x; `b` is 512-byte blocks.
fn parse_old_offset(s: &str) -> Option<u64> {
    let s = s.strip_prefix('+').unwrap_or(s);
    let (s, mult) = match s.strip_suffix('b') {
        Some(r) if !r.starts_with("0x") && !r.starts_with("0X") => (r, 512),
        _ => (s, 1),
    };
    let n = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()?
    } else if let Some(d) = s.strip_suffix('.') {
        d.parse().ok()?
    } else {
        u64::from_str_radix(s, 8).ok()?
    };
    n.checked_mul(mult)
}

/// Add the specs in a type string like `x1z`, `ac` or `o2x4`.
fn parse_type(s: &str, specs: &mut Vec<Spec>) -> Result<(), String> {
    let bad = |c: char| format!("invalid character '{c}' in type string '{s}'");
    let b: Vec<char> = s.chars().collect();
    if b.is_empty() {
        return Err(format!("invalid type string '{s}'"));
    }
    let mut k = 0;
    while k < b.len() {
        let c = b[k];
        k += 1;
        let digits: String = b[k..].iter().take_while(|c| c.is_ascii_digit()).collect();
        let (kind, size) = match c {
            'a' => (Kind::Named, 1),
            'c' => (Kind::Char, 1),
            'd' | 'o' | 'u' | 'x' => {
                let kind = match c {
                    'd' => Kind::Signed,
                    'o' => Kind::Octal,
                    'u' => Kind::Unsigned,
                    _ => Kind::Hex,
                };
                let size = if !digits.is_empty() {
                    k += digits.len();
                    digits.parse().unwrap_or(0)
                } else {
                    let named = match b.get(k) {
                        Some('C') => Some(1),
                        Some('S') => Some(2),
                        Some('I') => Some(4),
                        Some('L') => Some(8),
                        _ => None,
                    };
                    if named.is_some() {
                        k += 1;
                    }
                    named.unwrap_or(4)
                };
                if !matches!(size, 1 | 2 | 4 | 8) {
                    return Err(format!("invalid type string '{s}';\nthis system doesn't provide a {size}-byte integral type"));
                }
                (kind, size)
            }
            'f' => {
                let size = if !digits.is_empty() {
                    k += digits.len();
                    digits.parse().unwrap_or(0)
                } else {
                    let named = match b.get(k) {
                        Some('F') => Some(4),
                        Some('D') => Some(8),
                        Some('L') => Some(16),
                        _ => None,
                    };
                    if named.is_some() {
                        k += 1;
                    }
                    named.unwrap_or(8)
                };
                if !matches!(size, 4 | 8) {
                    return Err(format!("invalid type string '{s}';\nthis system doesn't provide a {size}-byte floating point type"));
                }
                (Kind::Float, size)
            }
            other => return Err(bad(other)),
        };
        let trailer = b.get(k) == Some(&'z');
        if trailer {
            k += 1;
        }
        let width = match (kind, size) {
            (Kind::Named | Kind::Char, _) => 3,
            (Kind::Signed, n) => int_bits_strlen(n * 8 - 1) + 1,
            (Kind::Unsigned, n) => int_bits_strlen(n * 8),
            (Kind::Octal, n) => (n * 8 + 2) / 3,
            (Kind::Hex, n) => n * 2,
            (Kind::Float, 4) => 15,
            (Kind::Float, _) => 24,
        };
        specs.push(Spec { kind, size, width, pad: 0, trailer });
    }
    Ok(())
}

/// gnulib's INT_BITS_STRLEN_BOUND: decimal digits for a `bits`-bit unsigned value.
fn int_bits_strlen(bits: usize) -> usize {
    (bits * 146 + 484) / 485
}

/// The concatenated input files, opened one at a time.
struct Input {
    files: Vec<String>,
    idx: usize,
    cur: Option<Box<dyn Read>>,
    failed: bool,
}

impl Input {
    fn open_next(&mut self) -> bool {
        while self.idx < self.files.len() {
            let f = self.files[self.idx].clone();
            self.idx += 1;
            if f == "-" {
                self.cur = Some(Box::new(io::stdin()));
                return true;
            }
            match File::open(&f) {
                Ok(h) => {
                    self.cur = Some(Box::new(h));
                    return true;
                }
                Err(e) => {
                    eprintln!("od: {f}: {}", errmsg(&e));
                    self.failed = true;
                }
            }
        }
        false
    }

    /// Skip `n` bytes; false when the input is shorter. Regular files are skipped whole or by seeking.
    fn skip(&mut self, mut n: u64) -> bool {
        while n > 0 {
            if self.cur.is_none() {
                // seek or skip whole regular files without reading them
                while self.idx < self.files.len() && self.files[self.idx] != "-" {
                    let f = self.files[self.idx].clone();
                    let Ok(meta) = std::fs::metadata(&f) else { break };
                    if !meta.is_file() {
                        break;
                    }
                    if meta.len() <= n {
                        n -= meta.len();
                        self.idx += 1;
                        continue;
                    }
                    if let Ok(mut h) = File::open(&f) {
                        if h.seek(SeekFrom::Start(n)).is_ok() {
                            self.idx += 1;
                            self.cur = Some(Box::new(h));
                            return true;
                        }
                    }
                    break;
                }
                if n == 0 {
                    return true;
                }
                if !self.open_next() {
                    return false;
                }
            }
            let mut buf = [0u8; 8192];
            let want = n.min(buf.len() as u64) as usize;
            match self.cur.as_mut().map(|r| r.read(&mut buf[..want])) {
                Some(Ok(0)) | None => self.cur = None,
                Some(Ok(got)) => n -= got as u64,
                Some(Err(e)) => {
                    eprintln!("od: {}: {}", self.files[self.idx - 1], errmsg(&e));
                    self.failed = true;
                    self.cur = None;
                }
            }
        }
        true
    }

    /// Fill `buf` as far as the input goes; returns the byte count.
    fn fill(&mut self, buf: &mut [u8]) -> usize {
        let mut got = 0;
        while got < buf.len() {
            if self.cur.is_none() && !self.open_next() {
                break;
            }
            match self.cur.as_mut().map(|r| r.read(&mut buf[got..])) {
                Some(Ok(0)) | None => self.cur = None,
                Some(Ok(n)) => got += n,
                Some(Err(e)) => {
                    eprintln!("od: {}: {}", self.files[self.idx - 1], errmsg(&e));
                    self.failed = true;
                    self.cur = None;
                }
            }
        }
        got
    }
}

fn errmsg(e: &io::Error) -> String {
    let s = e.to_string();
    s.split(" (os error").next().unwrap_or(&s).to_string()
}

fn address(out: &mut dyn Write, radix: char, a: u64) -> io::Result<()> {
    match radix {
        'd' => write!(out, "{a:07}"),
        'x' => write!(out, "{a:06x}"),
        'n' => Ok(()),
        _ => write!(out, "{a:07o}"),
    }
}

fn address_width(radix: char) -> usize {
    match radix {
        'x' => 6,
        'n' => 0,
        _ => 7,
    }
}

/// GNU's pad_at: the spare columns left over after the first `i` of `fields` fields.
fn pad_at(fields: usize, i: usize, pad: usize) -> usize {
    pad / fields * i + pad % fields * i / fields
}

fn dump(input: &mut Input, o: &mut Opts, out: &mut dyn Write) -> io::Result<()> {
    let lcm = o.specs.iter().map(|s| s.size).max().unwrap_or(1);
    let bpl = match o.width {
        Some(w) if w % lcm == 0 => w,
        Some(w) => {
            eprintln!("od: warning: invalid width {w}; using {lcm} instead");
            lcm
        }
        None if lcm < 16 => 16 - 16 % lcm,
        None => lcm,
    };
    let width_per_line = o.specs.iter().map(|s| (s.width + 1) * (bpl / s.size)).max().unwrap_or(0);
    for s in &mut o.specs {
        s.pad = width_per_line - s.width * (bpl / s.size);
    }

    let mut offset = o.skip;
    let mut left = o.limit;
    let mut prev: Option<Vec<u8>> = None;
    let mut prev_equal = false;
    loop {
        let want = left.map_or(bpl, |l| (l as usize).min(bpl));
        if want == 0 {
            break;
        }
        let mut block = vec![0u8; want];
        let n = input.fill(&mut block);
        if n == 0 {
            break;
        }
        block.truncate(n);
        if let Some(l) = left.as_mut() {
            *l -= n as u64;
        }
        let same = !o.verbose && n == bpl && prev.as_deref() == Some(&block[..]);
        if same {
            if !prev_equal {
                writeln!(out, "*")?;
                prev_equal = true;
            }
        } else {
            prev_equal = false;
            write_block(out, o, offset, &block, bpl)?;
        }
        offset += n as u64;
        if n < bpl {
            break;
        }
        prev = Some(block);
    }
    if o.radix != 'n' {
        address(out, o.radix, offset)?;
        writeln!(out)?;
    }
    Ok(())
}

fn write_block(out: &mut dyn Write, o: &Opts, offset: u64, block: &[u8], bpl: usize) -> io::Result<()> {
    let mut padded = block.to_vec();
    padded.resize(bpl, 0);
    for (si, s) in o.specs.iter().enumerate() {
        if si == 0 {
            address(out, o.radix, offset)?;
        } else {
            write!(out, "{:w$}", "", w = address_width(o.radix))?;
        }
        let fields = bpl / s.size;
        let blank = (bpl - block.len()) / s.size;
        let mut pad_left = s.pad;
        for f in 0..fields - blank {
            let next_pad = pad_at(fields, fields - f - 1, s.pad);
            let w = pad_left - next_pad + s.width;
            let mut bytes = padded[f * s.size..(f + 1) * s.size].to_vec();
            if o.swap {
                bytes.reverse();
            }
            write!(out, "{:>w$}", render(s, &bytes))?;
            pad_left = next_pad;
        }
        if s.trailer {
            write!(out, "{:w$}", "", w = blank * s.width + pad_at(fields, blank, s.pad))?;
            let shown: String = block.iter().map(|&c| if (0x20..0x7f).contains(&c) { c as char } else { '.' }).collect();
            write!(out, "  >{shown}<")?;
        }
        writeln!(out)?;
    }
    Ok(())
}

/// One field: `bytes` is a native-endian value of `s.size` bytes.
fn render(s: &Spec, bytes: &[u8]) -> String {
    let mut raw = [0u8; 8];
    let le = cfg!(target_endian = "little");
    if le {
        raw[..bytes.len()].copy_from_slice(bytes);
    } else {
        raw[8 - bytes.len()..].copy_from_slice(bytes);
    }
    let u = if le { u64::from_le_bytes(raw) } else { u64::from_be_bytes(raw) };
    match s.kind {
        Kind::Named => {
            let c = (bytes[0] & 0x7f) as usize;
            match c {
                127 => "del".into(),
                0..=32 => CHARNAME[c].into(),
                _ => (c as u8 as char).to_string(),
            }
        }
        Kind::Char => match bytes[0] {
            0 => "\\0".into(),
            7 => "\\a".into(),
            8 => "\\b".into(),
            12 => "\\f".into(),
            b'\n' => "\\n".into(),
            b'\r' => "\\r".into(),
            b'\t' => "\\t".into(),
            11 => "\\v".into(),
            c if (0x20..0x7f).contains(&c) => (c as char).to_string(),
            c => format!("{c:03o}"),
        },
        Kind::Unsigned => u.to_string(),
        Kind::Signed => {
            let shift = 64 - s.size * 8;
            (((u << shift) as i64) >> shift).to_string()
        }
        Kind::Octal => format!("{u:0w$o}", w = s.width),
        Kind::Hex => format!("{u:0w$x}", w = s.width),
        Kind::Float if s.size == 4 => {
            let x = f32::from_bits(u as u32);
            shortest_g(x.is_finite() && x.abs() < f32::MIN_POSITIVE, 6, 9, f64::from(x), |t| t.parse::<f32>().ok() == Some(x))
        }
        Kind::Float => {
            let x = f64::from_bits(u);
            shortest_g(x.is_finite() && x.abs() < f64::MIN_POSITIVE, 15, 17, x, |t| t.parse::<f64>().ok() == Some(x))
        }
    }
}

/// gnulib ftoastr: `%.Ng` with the smallest N (from the type's DIG, or 1 for tiny values)
/// that reads back as the same value.
fn shortest_g(tiny: bool, dig: usize, max: usize, x: f64, same: impl Fn(&str) -> bool) -> String {
    if x.is_nan() {
        return if x.is_sign_negative() { "-nan".into() } else { "nan".into() };
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf".into() } else { "inf".into() };
    }
    let mut p = if tiny { 1 } else { dig };
    loop {
        let s = fmt_g(x, p);
        if p >= max || same(&s) {
            return s;
        }
        p += 1;
    }
}

/// C's `%.{p}g`.
fn fmt_g(x: f64, p: usize) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let e = format!("{:.*e}", p - 1, x);
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let trim = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if exp < -4 || exp >= p as i32 {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", trim(mant), exp.abs())
    } else {
        trim(&format!("{:.*}", (p as i32 - 1 - exp).max(0) as usize, x))
    }
}

/// `-S MIN`: NUL-terminated runs of at least `min` printable bytes (or runs cut off by -N).
fn dump_strings(input: &mut Input, o: &Opts, min: u64, out: &mut dyn Write) -> io::Result<()> {
    let end = o.limit.map(|l| o.skip + l);
    let mut addr = o.skip;
    let mut run: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if end.is_some_and(|e| addr >= e) {
            break;
        }
        if input.fill(&mut byte) == 0 {
            // at EOF an unterminated run is dropped, as GNU does
            return Ok(());
        }
        addr += 1;
        let c = byte[0];
        if !(0x20..0x7f).contains(&c) {
            if c == 0 && run.len() as u64 >= min {
                print_string(out, o, addr - run.len() as u64 - 1, &run)?;
            }
            run.clear();
            continue;
        }
        run.push(c);
    }
    if run.len() as u64 >= min {
        print_string(out, o, addr - run.len() as u64, &run)?;
    }
    Ok(())
}

fn print_string(out: &mut dyn Write, o: &Opts, at: u64, s: &[u8]) -> io::Result<()> {
    if o.radix != 'n' {
        address(out, o.radix, at)?;
        write!(out, " ")?;
    }
    out.write_all(s)?;
    writeln!(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(t: &str) -> Vec<Spec> {
        let mut v = Vec::new();
        parse_type(t, &mut v).unwrap();
        v
    }

    #[test]
    fn type_strings() {
        let v = spec("x1zo2d4u8a");
        let got: Vec<_> = v.iter().map(|s| (s.kind, s.size, s.width)).collect();
        assert_eq!(
            got,
            vec![(Kind::Hex, 1, 2), (Kind::Octal, 2, 6), (Kind::Signed, 4, 11), (Kind::Unsigned, 8, 20), (Kind::Named, 1, 3)]
        );
        assert!(v[0].trailer);
        assert_eq!(spec("dL")[0].size, 8);
        assert_eq!(spec("f")[0].size, 8);
        assert_eq!(spec("fF")[0].width, 15);
        assert!(parse_type("q", &mut Vec::new()).is_err());
        assert!(parse_type("x3", &mut Vec::new()).is_err());
        assert!(parse_type("", &mut Vec::new()).is_err());
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("10"), Ok(10));
        assert_eq!(parse_size("0x10"), Ok(16));
        assert_eq!(parse_size("010"), Ok(8));
        assert_eq!(parse_size("1b"), Ok(512));
        assert_eq!(parse_size("2K"), Ok(2048));
        assert_eq!(parse_size("1KB"), Ok(1000));
        assert!(parse_size("x").is_err());
        assert!(parse_size("99999999999999999999").is_err());
        assert_eq!(parse_old_offset("+10"), Some(8));
        assert_eq!(parse_old_offset("10."), Some(10));
        assert_eq!(parse_old_offset("1b"), Some(512));
    }

    #[test]
    fn floats_like_c() {
        assert_eq!(fmt_g(0.5, 6), "0.5");
        assert_eq!(fmt_g(1e20, 6), "1e+20");
        assert_eq!(fmt_g(123456.0, 6), "123456");
        assert_eq!(fmt_g(1234567.0, 6), "1.23457e+06");
        assert_eq!(fmt_g(0.0001, 6), "0.0001");
        assert_eq!(fmt_g(0.00001, 6), "1e-05");
        let f = f32::from_le_bytes(*b"bana");
        assert_eq!(shortest_g(false, 6, 9, f64::from(f), |t| t.parse::<f32>().ok() == Some(f)), "2.748339e+20");
    }

    #[test]
    fn fields() {
        let s = spec("c")[0];
        assert_eq!(render(&s, b"\n"), "\\n");
        assert_eq!(render(&s, b"\x80"), "200");
        assert_eq!(render(&spec("a")[0], b" "), "sp");
        assert_eq!(render(&spec("a")[0], b"\xe1"), "a");
        assert_eq!(render(&spec("d1")[0], b"\xff"), "-1");
        assert_eq!(render(&spec("o2")[0], b"ba"), "060542");
        assert_eq!(render(&spec("x2")[0], b"\x01\x00"), "0001");
    }

    #[test]
    fn layout_matches_gnu() {
        let mut o = Opts { radix: 'o', specs: spec("o2"), skip: 0, limit: None, width: None, verbose: false, swap: false, strings: None };
        let mut input = Input { files: vec![], idx: 0, cur: Some(Box::new(&b"banana 3\napple 10\n"[..])), failed: false };
        let mut v = Vec::new();
        dump(&mut input, &mut o, &mut v).unwrap();
        assert_eq!(
            String::from_utf8(v).unwrap(),
            "0000000 060542 060556 060556 031440 060412 070160 062554 030440\n0000020 005060\n0000022\n"
        );
    }

    #[test]
    fn duplicates_collapse() {
        let mut o = Opts { radix: 'o', specs: spec("c"), skip: 0, limit: None, width: None, verbose: false, swap: false, strings: None };
        let mut input = Input { files: vec![], idx: 0, cur: Some(Box::new(&[0u8; 48][..])), failed: false };
        let mut v = Vec::new();
        dump(&mut input, &mut o, &mut v).unwrap();
        let text = String::from_utf8(v).unwrap();
        assert_eq!(text.lines().collect::<Vec<_>>()[1..], ["*", "0000060"]);
    }
}
