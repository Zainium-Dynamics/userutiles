//! user sort — sort lines of text files.
//!
//! GNU's model: a line is compared key by key (`-k F[.C][OPTS][,F[.C][OPTS]]`, fields
//! split at `-t SEP` or where blanks begin, leading blanks belonging to the field unless
//! `b`), each key with its own options or the global ones; when all keys tie, the whole
//! lines are compared bytewise (unless `-s`). `-u` keeps the first of each run of equal
//! keys. In the C/UTF-8 locales collation is byte order.
use std::cmp::Ordering;
use std::fs::File;
use std::io::{self, Read, Write};

use usercore::Ui;

#[derive(Clone, Copy, PartialEq, Default, Debug)]
enum KeyType {
    #[default]
    Default,
    Numeric,
    General,
    Human,
    Month,
    Version,
    Random,
}

#[derive(Clone, Copy, Default, Debug)]
struct KeyOpts {
    kind: KeyType,
    reverse: bool,
    fold: bool,
    dictionary: bool,
    printable: bool,
    blanks: bool,
}

impl KeyOpts {
    /// Applies one option letter; false if it isn't one.
    fn set(&mut self, c: char) -> bool {
        match c {
            'b' => self.blanks = true,
            'd' => self.dictionary = true,
            'f' => self.fold = true,
            'i' => self.printable = true,
            'r' => self.reverse = true,
            'n' => self.kind = KeyType::Numeric,
            'g' => self.kind = KeyType::General,
            'h' => self.kind = KeyType::Human,
            'M' => self.kind = KeyType::Month,
            'V' => self.kind = KeyType::Version,
            'R' => self.kind = KeyType::Random,
            _ => return false,
        }
        true
    }

    fn any(&self) -> bool {
        self.kind != KeyType::Default || self.reverse || self.fold || self.dictionary || self.printable || self.blanks
    }
}

#[derive(Clone, Copy, Debug)]
struct Key {
    start_field: usize,
    start_char: usize,
    start_blanks: bool,
    /// 0: to the end of the line
    end_field: usize,
    /// 0: to the end of the field
    end_char: usize,
    end_blanks: bool,
    opts: KeyOpts,
}

struct Config {
    keys: Vec<Key>,
    global: KeyOpts,
    sep: Option<u8>,
    stable: bool,
    unique: bool,
    seed: u64,
}

fn parse_key(spec: &str) -> Result<Key, String> {
    let bad = || format!("invalid number at field start: invalid count at start of '{spec}'");
    let (a, b) = match spec.split_once(',') {
        Some((a, b)) => (a, Some(b)),
        None => (spec, None),
    };
    // F[.C]OPTS
    let split = |p: &str| -> Result<(usize, usize, KeyOpts, bool), String> {
        let digits_end = p.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(p.len());
        let (pos, opts) = p.split_at(digits_end);
        let (f, c) = match pos.split_once('.') {
            Some((f, c)) => (f, c),
            None => (pos, ""),
        };
        let f: usize = f.parse().map_err(|_| bad())?;
        let c: usize = if c.is_empty() { 0 } else { c.parse().map_err(|_| bad())? };
        let mut o = KeyOpts::default();
        let mut blanks = false;
        for ch in opts.chars() {
            if ch == 'b' {
                blanks = true;
            }
            if !o.set(ch) {
                return Err(format!("stray character in field spec: invalid field specification '{spec}'"));
            }
        }
        Ok((f, c, o, blanks))
    };
    let (sf, sc, mut opts, sb) = split(a)?;
    if sf == 0 {
        return Err(format!("field number is zero: invalid field specification '{spec}'"));
    }
    let (ef, ec, eb) = match b {
        Some(b) => {
            let (ef, ec, o2, eb) = split(b)?;
            let blanks = opts.blanks;
            // options after POS2 apply to the key as well
            if o2.any() {
                let merged = KeyOpts { blanks, ..o2 };
                opts = KeyOpts {
                    kind: if o2.kind != KeyType::Default { o2.kind } else { opts.kind },
                    reverse: opts.reverse || merged.reverse,
                    fold: opts.fold || merged.fold,
                    dictionary: opts.dictionary || merged.dictionary,
                    printable: opts.printable || merged.printable,
                    blanks,
                };
            }
            (ef, ec, eb)
        }
        None => (0, 0, false),
    };
    Ok(Key { start_field: sf, start_char: sc.max(1), start_blanks: sb, end_field: ef, end_char: ec, end_blanks: eb, opts })
}

fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// Byte offsets where each field begins (field 1 at 0): after each `sep`, or where a
/// run of blanks begins (the blanks belong to the next field).
fn field_starts(line: &[u8], sep: Option<u8>) -> Vec<usize> {
    let mut starts = vec![0];
    match sep {
        Some(t) => {
            for (i, c) in line.iter().enumerate() {
                if *c == t {
                    starts.push(i + 1);
                }
            }
        }
        None => {
            let mut i = 0;
            while i < line.len() {
                while i < line.len() && is_blank(line[i]) {
                    i += 1;
                }
                while i < line.len() && !is_blank(line[i]) {
                    i += 1;
                }
                if i < line.len() {
                    starts.push(i);
                }
            }
        }
    }
    starts
}

/// The part of `line` that `key` selects.
fn key_slice<'a>(line: &'a [u8], key: &Key, sep: Option<u8>, global_blanks: bool) -> &'a [u8] {
    let starts = field_starts(line, sep);
    let field_end = |f: usize| -> usize {
        match starts.get(f) {
            // a separator ends the field before it
            Some(next) => if sep.is_some() { next - 1 } else { *next },
            None => line.len(),
        }
    };
    let skip = |mut i: usize, end: usize, blanks: bool| {
        if blanks {
            while i < end && is_blank(line[i]) {
                i += 1;
            }
        }
        i
    };
    let begin = match starts.get(key.start_field - 1) {
        Some(&s) => {
            let fe = field_end(key.start_field);
            let s = skip(s, fe, key.start_blanks || global_blanks && !key.opts.any());
            (s + key.start_char - 1).min(fe)
        }
        None => line.len(),
    };
    let end = if key.end_field == 0 {
        line.len()
    } else {
        match starts.get(key.end_field - 1) {
            Some(&s) => {
                let fe = field_end(key.end_field);
                if key.end_char == 0 {
                    fe
                } else {
                    let s = skip(s, fe, key.end_blanks || global_blanks && !key.opts.any());
                    (s + key.end_char).min(fe)
                }
            }
            None => line.len(),
        }
    };
    if begin <= end { &line[begin..end] } else { &line[begin..begin] }
}

fn as_str(b: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(b)
}

/// -g: a C strtod prefix; lines without one sort first.
fn general_num(b: &[u8]) -> Option<f64> {
    let s = as_str(b);
    let t = s.trim_start();
    let mut end = 0;
    for i in (1..=t.len()).rev() {
        if t.is_char_boundary(i) && t[..i].parse::<f64>().is_ok() {
            end = i;
            break;
        }
    }
    (end > 0).then(|| t[..end].parse().unwrap_or(0.0))
}

fn hash(seed: u64, b: &[u8]) -> u64 {
    let mut h = seed ^ 0xcbf2_9ce4_8422_2325;
    for c in b {
        h ^= *c as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn cmp_key(a: &[u8], b: &[u8], o: &KeyOpts, seed: u64) -> Ordering {
    let filt = |x: &[u8]| -> Vec<u8> {
        x.iter()
            .copied()
            .filter(|c| !o.dictionary || is_blank(*c) || c.is_ascii_alphanumeric())
            .filter(|c| !o.printable || (0x20..0x7f).contains(c))
            .map(|c| if o.fold { c.to_ascii_uppercase() } else { c })
            .collect()
    };
    let ord = match o.kind {
        KeyType::Numeric => parse_num(&as_str(a)).partial_cmp(&parse_num(&as_str(b))).unwrap_or(Ordering::Equal),
        KeyType::General => match (general_num(a), general_num(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        KeyType::Human => parse_human(&as_str(a)).partial_cmp(&parse_human(&as_str(b))).unwrap_or(Ordering::Equal),
        KeyType::Month => month_key(&as_str(a)).cmp(&month_key(&as_str(b))),
        KeyType::Version => ver_cmp(&as_str(a), &as_str(b)),
        KeyType::Random => hash(seed, &filt(a)).cmp(&hash(seed, &filt(b))),
        KeyType::Default => {
            if o.dictionary || o.printable || o.fold {
                filt(a).cmp(&filt(b))
            } else {
                a.cmp(b)
            }
        }
    };
    if o.reverse { ord.reverse() } else { ord }
}

/// Keys only (what -u calls equal).
fn cmp_keys(a: &[u8], b: &[u8], c: &Config) -> Ordering {
    if c.keys.is_empty() {
        let (a, b) = if c.global.blanks {
            (trim_blanks(a), trim_blanks(b))
        } else {
            (a, b)
        };
        return cmp_key(a, b, &c.global, c.seed);
    }
    for k in &c.keys {
        let o = if k.opts.any() { KeyOpts { blanks: k.opts.blanks, ..k.opts } } else { c.global };
        let ka = key_slice(a, k, c.sep, c.global.blanks);
        let kb = key_slice(b, k, c.sep, c.global.blanks);
        match cmp_key(ka, kb, &o, c.seed) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    Ordering::Equal
}

fn trim_blanks(x: &[u8]) -> &[u8] {
    let i = x.iter().position(|c| !is_blank(*c)).unwrap_or(x.len());
    &x[i..]
}

/// Keys, then (unless -s or -u) the whole lines as the last resort.
fn cmp_lines(a: &[u8], b: &[u8], c: &Config) -> Ordering {
    match cmp_keys(a, b, c) {
        Ordering::Equal if !c.stable && !c.unique => {
            let ord = a.cmp(b);
            if c.global.reverse { ord.reverse() } else { ord }
        }
        ord => ord,
    }
}

pub fn run() -> i32 {
    let ui = Ui::new("sort");
    let mut cfg = Config { keys: Vec::new(), global: KeyOpts::default(), sep: None, stable: false, unique: false, seed: 0 };
    let mut check: Option<bool> = None; // Some(quiet)
    let mut merge = false;
    let mut zero = false;
    let mut output: Option<String> = None;
    let mut files: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let need = |i: &mut usize, opt: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or(format!("option requires an argument -- '{opt}'"))
    };
    while i < args.len() {
        let a = args[i].clone();
        let result: Result<(), String> = (|| {
            match a.as_str() {
                "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "--version" => {
                    println!("sort (user_utils) 0.2.0");
                    std::process::exit(0);
                }
                "--" => {
                    files.extend(args[i + 1..].iter().cloned());
                    i = args.len();
                }
                "-" => files.push(a.clone()),
                "-k" | "--key" => cfg.keys.push(parse_key(&need(&mut i, "k")?)?),
                "-t" | "--field-separator" => cfg.sep = Some(sep_byte(&need(&mut i, "t")?)?),
                "-o" | "--output" => output = Some(need(&mut i, "o")?),
                "-S" | "--buffer-size" | "-T" | "--temporary-directory" | "--parallel" | "--batch-size" => {
                    need(&mut i, "S")?;
                }
                "--random-source" => {
                    need(&mut i, "random-source")?;
                }
                s if s.starts_with("--key=") => cfg.keys.push(parse_key(&s[6..])?),
                s if s.starts_with("--field-separator=") => cfg.sep = Some(sep_byte(&s[18..])?),
                s if s.starts_with("--output=") => output = Some(s[9..].to_string()),
                s if s.starts_with("--buffer-size=") || s.starts_with("--parallel=") || s.starts_with("--temporary-directory=") || s.starts_with("--random-source=") || s.starts_with("--batch-size=") => {}
                "--numeric-sort" => cfg.global.kind = KeyType::Numeric,
                "--general-numeric-sort" => cfg.global.kind = KeyType::General,
                "--human-numeric-sort" => cfg.global.kind = KeyType::Human,
                "--month-sort" => cfg.global.kind = KeyType::Month,
                "--version-sort" => cfg.global.kind = KeyType::Version,
                "--random-sort" => cfg.global.kind = KeyType::Random,
                "--reverse" => cfg.global.reverse = true,
                "--ignore-case" => cfg.global.fold = true,
                "--dictionary-order" => cfg.global.dictionary = true,
                "--ignore-nonprinting" => cfg.global.printable = true,
                "--ignore-leading-blanks" => cfg.global.blanks = true,
                "--unique" => cfg.unique = true,
                "--stable" => cfg.stable = true,
                "--merge" => merge = true,
                "--zero-terminated" => zero = true,
                "--check" | "--check=diagnose-first" => check = Some(false),
                "--check=quiet" | "--check=silent" => check = Some(true),
                s if s.starts_with("--sort=") => {
                    let c = match &s[7..] {
                        "numeric" => 'n',
                        "general-numeric" => 'g',
                        "human-numeric" => 'h',
                        "month" => 'M',
                        "version" => 'V',
                        "random" => 'R',
                        other => return Err(format!("invalid argument '{other}' for '--sort'")),
                    };
                    cfg.global.set(c);
                }
                s if s.starts_with("--") => return Err(format!("unrecognized option '{s}'")),
                s if s.starts_with('-') && s.len() > 1 => {
                    let chars: Vec<char> = s[1..].chars().collect();
                    let mut k = 0;
                    while k < chars.len() {
                        let c = chars[k];
                        let inline: String = chars[k + 1..].iter().collect();
                        match c {
                            'k' | 't' | 'o' | 'S' | 'T' => {
                                let v = if inline.is_empty() { need(&mut i, &c.to_string())? } else { inline };
                                match c {
                                    'k' => cfg.keys.push(parse_key(&v)?),
                                    't' => cfg.sep = Some(sep_byte(&v)?),
                                    'o' => output = Some(v),
                                    _ => {}
                                }
                                break;
                            }
                            'c' => check = Some(false),
                            'C' => check = Some(true),
                            'm' => merge = true,
                            's' => cfg.stable = true,
                            'u' => cfg.unique = true,
                            'z' => zero = true,
                            c if cfg.global.set(c) => {}
                            c => return Err(format!("invalid option -- '{c}'")),
                        }
                        k += 1;
                    }
                }
                _ => files.push(a.clone()),
            }
            Ok(())
        })();
        if let Err(e) = result {
            ui.err(&e);
            return 2;
        }
        i += 1;
    }
    if files.is_empty() {
        files.push("-".into());
    }
    if cfg.global.kind == KeyType::Random {
        cfg.seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
    }
    let term = if zero { 0u8 } else { b'\n' };

    let mut inputs: Vec<Vec<Vec<u8>>> = Vec::new();
    for f in &files {
        let mut v = Vec::new();
        if let Err(e) = read_records(f, term, &mut v) {
            ui.err(&format!("cannot read: {f}: {e}"));
            return 2;
        }
        inputs.push(v);
    }

    if let Some(quiet) = check {
        let lines = &inputs[0];
        for w in 1..lines.len() {
            let ord = cmp_lines(&lines[w - 1], &lines[w], &cfg);
            if ord == Ordering::Greater || (cfg.unique && cmp_keys(&lines[w - 1], &lines[w], &cfg) == Ordering::Equal) {
                if !quiet {
                    eprintln!("sort: {}:{}: disorder: {}", files[0], w + 1, as_str(&lines[w]));
                }
                return 1;
            }
        }
        return 0;
    }

    let mut lines: Vec<Vec<u8>> = if merge {
        merge_inputs(inputs, &cfg)
    } else {
        let mut all: Vec<Vec<u8>> = inputs.into_iter().flatten().collect();
        all.sort_by(|a, b| cmp_lines(a, b, &cfg));
        all
    };
    if cfg.unique {
        lines.dedup_by(|b, a| cmp_keys(a, b, &cfg) == Ordering::Equal);
    }

    let mut buf = Vec::new();
    for l in &lines {
        buf.extend_from_slice(l);
        buf.push(term);
    }
    let written = match &output {
        Some(path) => std::fs::write(path, &buf),
        None => io::stdout().lock().write_all(&buf),
    };
    match written {
        Ok(()) => 0,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            ui.err(&format!("write failed: {}: {e}", output.as_deref().unwrap_or("standard output")));
            2
        }
    }
}

/// `-t`'s argument: one character (`\0` for NUL).
fn sep_byte(s: &str) -> Result<u8, String> {
    match s {
        "\\0" => Ok(0),
        _ if s.len() == 1 => Ok(s.as_bytes()[0]),
        _ => Err(format!("multi-character tab '{s}'")),
    }
}

/// -m: merge inputs that are each already sorted, by repeatedly taking the least head.
fn merge_inputs(inputs: Vec<Vec<Vec<u8>>>, cfg: &Config) -> Vec<Vec<u8>> {
    let mut pos = vec![0usize; inputs.len()];
    let mut out = Vec::new();
    loop {
        let mut best: Option<usize> = None;
        for k in 0..inputs.len() {
            if pos[k] >= inputs[k].len() {
                continue;
            }
            best = match best {
                Some(b) if cmp_lines(&inputs[k][pos[k]], &inputs[b][pos[b]], cfg) != Ordering::Less => Some(b),
                _ => Some(k),
            };
        }
        match best {
            Some(b) => {
                out.push(inputs[b][pos[b]].clone());
                pos[b] += 1;
            }
            None => return out,
        }
    }
}

fn print_help() {
    print!(
        "Usage: sort [OPTION]... [FILE]...\n\
 Write sorted concatenation of all FILE(s) to standard output.\n\n\
 -b, --ignore-leading-blanks  ignore leading blanks\n\
 -d, --dictionary-order       consider only blanks and alphanumeric characters\n\
 -f, --ignore-case            fold lower case to upper case characters\n\
 -g, --general-numeric-sort   compare according to general numerical value\n\
 -i, --ignore-nonprinting     consider only printable characters\n\
 -M, --month-sort             compare (unknown) < 'JAN' < ... < 'DEC'\n\
 -h, --human-numeric-sort     compare human readable numbers (e.g., 2K 1G)\n\
 -n, --numeric-sort           compare according to string numerical value\n\
 -R, --random-sort            shuffle, but group identical keys\n\
 -r, --reverse                reverse the result of comparisons\n\
 -V, --version-sort           natural sort of (version) numbers within text\n\
 -c, --check / -C             check for sorted input; do not sort\n\
 -k, --key=KEYDEF             sort via a key; KEYDEF gives location and type\n\
 -m, --merge                  merge already sorted files; do not sort\n\
 -o, --output=FILE            write result to FILE instead of standard output\n\
 -s, --stable                 stabilize sort by disabling last-resort comparison\n\
 -t, --field-separator=SEP    use SEP instead of non-blank to blank transition\n\
 -u, --unique                 output only the first of an equal run\n\
 -z, --zero-terminated        line delimiter is NUL, not newline\n"
    );
}

/// The records of `path` (or stdin), split at `term`; a last record without `term`
/// counts too.
fn read_records(path: &str, term: u8, out: &mut Vec<Vec<u8>>) -> io::Result<()> {
    let mut data = Vec::new();
    if path == "-" {
        io::stdin().lock().read_to_end(&mut data)?;
    } else {
        File::open(path)?.read_to_end(&mut data)?;
    }
    if data.is_empty() {
        return Ok(());
    }
    if data.last() == Some(&term) {
        data.pop();
    }
    out.extend(data.split(|c| *c == term).map(|l| l.to_vec()));
    Ok(())
}

/// For the tests: the lines of a file.
#[cfg(test)]
fn read_lines(path: &str, out: &mut Vec<String>) -> io::Result<()> {
    use std::io::BufRead;
    for line in io::BufReader::new(File::open(path)?).lines() {
        out.push(line?);
    }
    Ok(())
}

fn parse_num(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    if !bytes.is_empty() && (bytes[0] == b'-' || bytes[0] == b'+') {
        end = 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    s[..end].parse().unwrap_or(0.0)
}

/// Like [`parse_num`], but also recognizes a trailing `K`/`M`/`G`/`T`/`P`
/// (powers of 1024) suffix for `-h`/`--human-numeric-sort`.
fn parse_human(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    if !bytes.is_empty() && (bytes[0] == b'-' || bytes[0] == b'+') {
        end = 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    let num: f64 = s[..end].parse().unwrap_or(0.0);
    let mult = match bytes.get(end).map(|c| c.to_ascii_uppercase()) {
        Some(b'K') => 1024.0,
        Some(b'M') => 1024.0f64.powi(2),
        Some(b'G') => 1024.0f64.powi(3),
        Some(b'T') => 1024.0f64.powi(4),
        Some(b'P') => 1024.0f64.powi(5),
        _ => 1.0,
    };
    num * mult
}

fn month_key(s: &str) -> u8 {
    let s = s.trim_start();
    let m = s.get(..3).unwrap_or("").to_ascii_uppercase();
    match m.as_str() {
        "JAN" => 1,
        "FEB" => 2,
        "MAR" => 3,
        "APR" => 4,
        "MAY" => 5,
        "JUN" => 6,
        "JUL" => 7,
        "AUG" => 8,
        "SEP" => 9,
        "OCT" => 10,
        "NOV" => 11,
        "DEC" => 12,
        _ => 0,
    }
}

/// `-V`/`--version-sort` comparison: runs of ASCII digits compare
/// numerically (as `u64`, saturating rather than overflowing on
/// pathologically long digit runs), everything else compares
/// char-by-char.
fn ver_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let mut an = 0u64;
                while let Some(c) = ai.peek().copied() {
                    if c.is_ascii_digit() {
                        an = an
                            .saturating_mul(10)
                            .saturating_add((c as u8 - b'0') as u64);
                        ai.next();
                    } else {
                        break;
                    }
                }
                let mut bn = 0u64;
                while let Some(c) = bi.peek().copied() {
                    if c.is_ascii_digit() {
                        bn = bn
                            .saturating_mul(10)
                            .saturating_add((c as u8 - b'0') as u64);
                        bi.next();
                    } else {
                        break;
                    }
                }
                match an.cmp(&bn) {
                    Ordering::Equal => {}
                    o => return o,
                }
            }
            (Some(ac), Some(bc)) => match ac.cmp(&bc) {
                Ordering::Equal => {
                    ai.next();
                    bi.next();
                }
                o => return o,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_num_basic() {
        assert_eq!(parse_num("42"), 42.0);
        assert_eq!(parse_num("-3.5"), -3.5);
    }

    #[test]
    fn parse_num_non_numeric_defaults_to_zero() {
        assert_eq!(parse_num("abc"), 0.0);
    }

    #[test]
    fn parse_num_leading_whitespace_and_trailing_text() {
        assert_eq!(parse_num("  10abc"), 10.0);
    }

    #[test]
    fn parse_human_suffixes() {
        assert_eq!(parse_human("2K"), 2048.0);
        assert_eq!(parse_human("1M"), 1024.0 * 1024.0);
        assert_eq!(parse_human("3"), 3.0);
    }

    #[test]
    fn month_key_known_and_unknown() {
        assert_eq!(month_key("Jan 1"), 1);
        assert_eq!(month_key("Dec 31"), 12);
        assert_eq!(month_key("nope"), 0);
    }

    #[test]
    fn ver_cmp_numeric_runs() {
        assert_eq!(ver_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(ver_cmp("file10", "file2"), Ordering::Greater);
        assert_eq!(ver_cmp("file1", "file1"), Ordering::Equal);
    }

    #[test]
    fn ver_cmp_does_not_panic_on_huge_digit_run() {
        // 40 digits overflows u64; saturating arithmetic must not panic.
        let a = "9999999999999999999999999999999999999999";
        let b = "1";
        // Should not panic regardless of the resulting ordering.
        let _ = ver_cmp(a, b);
    }

    #[test]
    fn cmp_lines_numeric_ties_break_on_text() {
        // "1" and "01" both parse to 1.0, so the string comparison
        // decides the tie.
        let mut c = Config { keys: Vec::new(), global: KeyOpts::default(), sep: None, stable: false, unique: false, seed: 0 };
        c.global.kind = KeyType::Numeric;
        let ord = cmp_lines(b"01", b"1", &c);
        assert_eq!(ord, "01".cmp("1"));
    }

    #[test]
    fn keys_and_fields() {
        let k = parse_key("2,2n").unwrap();
        assert_eq!(key_slice(b"x:22:b", &k, Some(b':'), false), b"22");
        let k = parse_key("2").unwrap();
        assert_eq!(key_slice(b"apple 10", &k, None, false), b" 10");
        let k = parse_key("1.2,1.3").unwrap();
        assert_eq!(key_slice(b"abcd", &k, None, false), b"bc");
    }

    #[test]
    fn read_lines_missing_file_errors() {
        let mut out = Vec::new();
        assert!(read_lines("/nonexistent/user-sort-missing", &mut out).is_err());
    }

    #[test]
    fn read_lines_reads_all_lines_from_a_real_file() {
        let dir = std::env::temp_dir().join(format!("user_sort_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("in.txt");
        std::fs::write(&path, "b\na\nc\n").unwrap();
        let mut out = Vec::new();
        read_lines(path.to_str().unwrap(), &mut out).unwrap();
        assert_eq!(out, vec!["b", "a", "c"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
