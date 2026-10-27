//! The shared front end of `md5sum`, `sha*sum`, `b2sum` and `cksum -a`: GNU's options,
//! output lines (`HASH  FILE`, `HASH *FILE`, `ALGO (FILE) = HASH`) and `--check`.

use std::io::{self, BufRead, BufReader, Write};

use super::{hash_path_update, hex_lower, Blake2b, Md5, Sha1, Sha224, Sha256, Sha384, Sha512};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algo {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Blake2b,
}

impl Algo {
    /// `cksum -a` names.
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "md5" => Algo::Md5,
            "sha1" => Algo::Sha1,
            "sha224" => Algo::Sha224,
            "sha256" => Algo::Sha256,
            "sha384" => Algo::Sha384,
            "sha512" => Algo::Sha512,
            "blake2b" => Algo::Blake2b,
            _ => return None,
        })
    }

    fn bits(self) -> usize {
        match self {
            Algo::Md5 => 128,
            Algo::Sha1 => 160,
            Algo::Sha224 => 224,
            Algo::Sha256 => 256,
            Algo::Sha384 => 384,
            Algo::Sha512 | Algo::Blake2b => 512,
        }
    }

    /// The name in tagged lines; BLAKE2b with a short digest is `BLAKE2b-N`.
    fn tag(self, bits: usize) -> String {
        match self {
            Algo::Md5 => "MD5".into(),
            Algo::Sha1 => "SHA1".into(),
            Algo::Sha224 => "SHA224".into(),
            Algo::Sha256 => "SHA256".into(),
            Algo::Sha384 => "SHA384".into(),
            Algo::Sha512 => "SHA512".into(),
            Algo::Blake2b if bits == 512 => "BLAKE2b".into(),
            Algo::Blake2b => format!("BLAKE2b-{bits}"),
        }
    }

    /// Lowercase hex digest of `path` (`-` is stdin); `bits` only matters for BLAKE2b.
    pub fn hash(self, path: &str, bits: usize) -> io::Result<String> {
        macro_rules! digest {
            ($h:expr) => {{
                let mut h = $h;
                hash_path_update(path, |c| h.update(c))?;
                hex_lower(&h.finalize())
            }};
        }
        Ok(match self {
            Algo::Md5 => digest!(Md5::new()),
            Algo::Sha1 => digest!(Sha1::new()),
            Algo::Sha224 => digest!(Sha224::new()),
            Algo::Sha256 => digest!(Sha256::new()),
            Algo::Sha384 => digest!(Sha384::new()),
            Algo::Sha512 => digest!(Sha512::new()),
            Algo::Blake2b => digest!(Blake2b::with_out_len(bits / 8)),
        })
    }
}

#[derive(Default)]
struct Opts {
    binary: bool,
    check: bool,
    tag: bool,
    zero: bool,
    ignore_missing: bool,
    quiet: bool,
    status: bool,
    strict: bool,
    warn: bool,
    bits: Option<usize>,
}

/// `io::Error` text without Rust's " (os error N)" tail, as C tools print it.
fn msg(e: &io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// Run `name` (md5sum, sha256sum, ...) with the process arguments.
pub fn main(name: &str, algo: Algo) -> i32 {
    run_args(name, algo, std::env::args().skip(1).collect(), false)
}

/// The checksum tool over `args`; `tag_default` is cksum's tagged output (`--untagged` undoes it).
pub fn run_args(name: &str, algo: Algo, args: Vec<String>, tag_default: bool) -> i32 {
    let mut o = Opts { tag: tag_default, ..Opts::default() };
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;
    let mut opts_done = false;
    let usage = |bad: &str| {
        eprintln!("{name}: {bad}\nTry '{name} --help' for more information.");
        1
    };
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if opts_done || a == "-" || !a.starts_with('-') {
            files.push(a.clone());
            continue;
        }
        if let Some(long) = a.strip_prefix("--") {
            let (key, val) = match long.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (long, None),
            };
            match key {
                "" => opts_done = true,
                "help" => {
                    print!("Usage: {name} [OPTION]... [FILE]...\nPrint or check {} ({}-bit) checksums.\n\n  -b, --binary          read in binary mode\n  -c, --check           read checksums from the FILEs and check them\n      --tag             create a BSD-style checksum\n  -t, --text            read in text mode (default)\n  -z, --zero            end each output line with NUL, not newline\n\nWith --check:\n      --ignore-missing  don't fail or report status for missing files\n      --quiet           don't print OK for each successfully verified file\n      --status          don't output anything, status code shows success\n      --strict          exit non-zero for improperly formatted checksum lines\n  -w, --warn            warn about improperly formatted checksum lines\n", algo.tag(algo.bits()), algo.bits());
                    if algo == Algo::Blake2b {
                        print!("\n  -l, --length=BITS     digest length in bits; a multiple of 8, at most 512\n");
                    }
                    return 0;
                }
                "version" => {
                    println!("{name} (user_utils) 0.2.0");
                    return 0;
                }
                "binary" => o.binary = true,
                "text" => o.binary = false,
                "check" => o.check = true,
                "tag" => o.tag = true,
                "untagged" => o.tag = false,
                "zero" => o.zero = true,
                "ignore-missing" => o.ignore_missing = true,
                "quiet" => o.quiet = true,
                "status" => o.status = true,
                "strict" => o.strict = true,
                "warn" => o.warn = true,
                "length" if algo == Algo::Blake2b => {
                    let v = match val {
                        Some(v) => v,
                        None if i < args.len() => {
                            i += 1;
                            args[i - 1].clone()
                        }
                        None => return usage("option '--length' requires an argument"),
                    };
                    match parse_bits(&v) {
                        Some(b) => o.bits = Some(b),
                        None => return usage(&format!("invalid length: '{v}'")),
                    }
                }
                _ => return usage(&format!("unrecognized option '{a}'")),
            }
            continue;
        }
        let chars: Vec<char> = a[1..].chars().collect();
        let mut k = 0;
        while k < chars.len() {
            match chars[k] {
                'b' => o.binary = true,
                't' => o.binary = false,
                'c' => o.check = true,
                'z' => o.zero = true,
                'w' => o.warn = true,
                'l' if algo == Algo::Blake2b => {
                    let rest: String = chars[k + 1..].iter().collect();
                    let v = if !rest.is_empty() {
                        rest
                    } else {
                        match args.get(i) {
                            Some(v) => {
                                i += 1;
                                v.clone()
                            }
                            None => return usage("option requires an argument -- 'l'"),
                        }
                    };
                    match parse_bits(&v) {
                        Some(b) => o.bits = Some(b),
                        None => return usage(&format!("invalid length: '{v}'")),
                    }
                    break;
                }
                c => return usage(&format!("invalid option -- '{c}'")),
            }
            k += 1;
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }
    if o.check {
        return check(name, algo, &o, &files);
    }

    let bits = o.bits.unwrap_or(algo.bits());
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut status = 0;
    for f in &files {
        match algo.hash(f, bits) {
            Ok(h) => {
                let _ = write_line(&mut out, &o, algo, bits, &h, f);
            }
            Err(e) => {
                let _ = out.flush();
                eprintln!("{name}: {f}: {}", msg(&e));
                status = 1;
            }
        }
    }
    let _ = out.flush();
    status
}

fn parse_bits(v: &str) -> Option<usize> {
    let b: usize = v.parse().ok()?;
    match b {
        0 => Some(512),
        b if b % 8 == 0 && b <= 512 => Some(b),
        _ => None,
    }
}

/// One output line; names with `\`, newline or CR are escaped and the line starts with `\`.
fn write_line(out: &mut dyn Write, o: &Opts, algo: Algo, bits: usize, hash: &str, file: &str) -> io::Result<()> {
    let eol = if o.zero { '\0' } else { '\n' };
    let escape = !o.zero && file.contains(['\\', '\n', '\r']);
    let name = if escape {
        file.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "\\r")
    } else {
        file.to_string()
    };
    let lead = if escape { "\\" } else { "" };
    if o.tag {
        write!(out, "{lead}{} ({name}) = {hash}{eol}", algo.tag(bits))
    } else {
        write!(out, "{lead}{hash} {}{name}{eol}", if o.binary { '*' } else { ' ' })
    }
}

/// A checksum line: (hash, file, digest bits), from `HASH  FILE`, `HASH *FILE`,
/// `HASH FILE` or `ALGO (FILE) = HASH`.
fn parse_line(line: &str, algo: Algo) -> Option<(String, String, usize)> {
    let (escaped, line) = match line.strip_prefix('\\') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let tagged = line.split_once(" (").and_then(|(tag, rest)| {
        let (file, hash) = rest.rsplit_once(") = ")?;
        let bits = match (algo, tag) {
            (Algo::Blake2b, "BLAKE2b") => 512,
            (Algo::Blake2b, t) => t.strip_prefix("BLAKE2b-")?.parse().ok()?,
            (a, t) if t == a.tag(a.bits()) => a.bits(),
            _ => return None,
        };
        Some((hash.to_string(), file.to_string(), bits))
    });
    let (hash, file, bits) = match tagged {
        Some(t) => t,
        None => {
            let (hash, rest) = line.split_once(' ')?;
            let file = rest.strip_prefix(['*', ' ']).unwrap_or(rest);
            let bits = if algo == Algo::Blake2b { hash.len() * 4 } else { algo.bits() };
            (hash.to_string(), file.to_string(), bits)
        }
    };
    let hex_ok = hash.len() * 4 == bits && hash.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex_ok || file.is_empty() || bits == 0 || bits > 512 || bits % 8 != 0 {
        return None;
    }
    let file = if escaped { unescape(&file) } else { file };
    Some((hash, file, bits))
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn check(name: &str, algo: Algo, o: &Opts, lists: &[String]) -> i32 {
    let mut status = 0;
    for list in lists {
        let reader: Box<dyn BufRead> = if list == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            match std::fs::File::open(list) {
                Ok(f) => Box::new(BufReader::new(f)),
                Err(e) => {
                    eprintln!("{name}: {list}: {}", msg(&e));
                    status = 1;
                    continue;
                }
            }
        };
        let (mut bad_lines, mut failed, mut unreadable, mut verified, mut formatted) = (0, 0, 0, 0, 0);
        for (n, raw) in reader.split(b'\n').enumerate() {
            let raw = match raw {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{name}: {list}: {}", msg(&e));
                    status = 1;
                    break;
                }
            };
            let line = String::from_utf8_lossy(&raw);
            let line = line.strip_suffix('\r').unwrap_or(&line);
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((hash, file, bits)) = parse_line(line, algo) else {
                bad_lines += 1;
                if o.warn {
                    eprintln!("{name}: {list}: {}: improperly formatted {} checksum line", n + 1, algo.tag(algo.bits()));
                }
                continue;
            };
            formatted += 1;
            match algo.hash(&file, bits) {
                Ok(h) if h.eq_ignore_ascii_case(&hash) => {
                    verified += 1;
                    if !o.quiet && !o.status {
                        println!("{file}: OK");
                    }
                }
                Ok(_) => {
                    failed += 1;
                    if !o.status {
                        println!("{file}: FAILED");
                    }
                }
                Err(e) if o.ignore_missing && e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => {
                    unreadable += 1;
                    if !o.status {
                        eprintln!("{name}: {file}: {}", msg(&e));
                        println!("{file}: FAILED open or read");
                    }
                }
            }
        }
        if formatted == 0 {
            eprintln!("{name}: {list}: no properly formatted checksum lines found");
            status = 1;
            continue;
        }
        if !o.status {
            if bad_lines > 0 {
                eprintln!("{name}: WARNING: {}", plural(bad_lines, "line is improperly formatted", "lines are improperly formatted"));
            }
            if unreadable > 0 {
                eprintln!("{name}: WARNING: {}", plural(unreadable, "listed file could not be read", "listed files could not be read"));
            }
            if failed > 0 {
                eprintln!("{name}: WARNING: {}", plural(failed, "computed checksum did NOT match", "computed checksums did NOT match"));
            }
        }
        if o.ignore_missing && verified + failed + unreadable == 0 {
            if !o.status {
                eprintln!("{name}: {list}: no file was verified");
            }
            status = 1;
        }
        if failed > 0 || unreadable > 0 || (o.strict && bad_lines > 0) {
            status = 1;
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_formats() {
        let h = "d41d8cd98f00b204e9800998ecf8427e";
        assert_eq!(parse_line(&format!("{h}  a b"), Algo::Md5).unwrap().1, "a b");
        assert_eq!(parse_line(&format!("{h} *bin"), Algo::Md5).unwrap().1, "bin");
        assert_eq!(parse_line(&format!("{h} one"), Algo::Md5).unwrap().1, "one");
        assert_eq!(parse_line(&format!("MD5 (x (1).txt) = {h}"), Algo::Md5).unwrap().1, "x (1).txt");
        assert_eq!(parse_line(&format!("\\{h}  a\\nb"), Algo::Md5).unwrap().1, "a\nb");
        assert!(parse_line(&format!("SHA1 (x) = {h}"), Algo::Md5).is_none());
        assert!(parse_line("abc  x", Algo::Md5).is_none());
        let b = "0".repeat(32);
        assert_eq!(parse_line(&format!("BLAKE2b-128 (f) = {b}"), Algo::Blake2b).unwrap().2, 128);
        assert_eq!(parse_line(&format!("{b}  f"), Algo::Blake2b).unwrap().2, 128);
    }

    #[test]
    fn output_lines() {
        let mut o = Opts::default();
        let mut v = Vec::new();
        write_line(&mut v, &o, Algo::Md5, 128, "ab", "f").unwrap();
        o.binary = true;
        write_line(&mut v, &o, Algo::Md5, 128, "ab", "a\\b").unwrap();
        o.tag = true;
        write_line(&mut v, &o, Algo::Blake2b, 256, "ab", "f").unwrap();
        assert_eq!(String::from_utf8(v).unwrap(), "ab  f\n\\ab *a\\\\b\nBLAKE2b-256 (f) = ab\n");
    }
}
