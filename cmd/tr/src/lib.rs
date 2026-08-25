//! user tr — translate or delete characters.
use std::io::{self, Read, Write};

pub fn run() -> i32 {
    let mut delete = false;
    let mut squeeze = false;
    let mut complement = false;
    let mut sets: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: tr [OPTION]... SET1 [SET2]\n\
 Translate, squeeze, and/or delete characters from standard input.\n\n\
 -c, -C, --complement use the complement of SET1\n\
 -d, --delete delete characters in SET1\n\
 -s, --squeeze-repeats replace each sequence of a repeated char\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                );
                return 0;
            }
            "--version" => {
                println!("tr (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--delete" => delete = true,
            "-s" | "--squeeze-repeats" => squeeze = true,
            "-c" | "-C" | "--complement" => complement = true,
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for ch in s.chars().skip(1) {
                    match ch {
                        'd' => delete = true,
                        's' => squeeze = true,
                        'c' | 'C' => complement = true,
                        _ => {
                            eprintln!("tr: invalid option -- '{ch}'");
                            return 1;
                        }
                    }
                }
            }
            other => sets.push(other.to_string()),
        }
    }

    if sets.is_empty() {
        eprintln!("tr: missing operand");
        return 1;
    }

    let set1 = expand_set(&sets[0]);
    let set2 = if sets.len() > 1 {
        Some(expand_set(&sets[1]))
    } else {
        None
    };

    if !delete && set2.is_none() && !squeeze {
        eprintln!("tr: missing operand after '{}'", sets[0]);
        return 1;
    }

    let mut map = [None::<u8>; 256];
    let mut del = [false; 256];
    let mut sq = [false; 256];

    if delete {
        for &b in &set1 {
            if complement {
                // mark all then unmark set1
            } else {
                del[b as usize] = true;
            }
        }
        if complement {
            let mut in_set = [false; 256];
            for &b in &set1 {
                in_set[b as usize] = true;
            }
            for i in 0..256 {
                if !in_set[i] {
                    del[i] = true;
                }
            }
        }
    } else if let Some(ref s2) = set2 {
        // translation
        let mut s2 = s2.clone();
        if s2.is_empty() {
            eprintln!("tr: SET2 must be non-empty when translating");
            return 1;
        }
        // extend set2 by repeating last char if shorter
        if s2.len() < set1.len() {
            let last = *s2.last().unwrap();
            while s2.len() < set1.len() {
                s2.push(last);
            }
        }
        if complement {
            let mut in_set = [false; 256];
            for &b in &set1 {
                in_set[b as usize] = true;
            }
            let mut comp = Vec::new();
            for i in 0..256u16 {
                if !in_set[i as usize] {
                    comp.push(i as u8);
                }
            }
            for (i, &b) in comp.iter().enumerate() {
                let rep = s2[i.min(s2.len() - 1)];
                map[b as usize] = Some(rep);
            }
        } else {
            for (i, &b) in set1.iter().enumerate() {
                map[b as usize] = Some(s2[i]);
            }
        }
    }

    if squeeze {
        let sq_set = match &set2 {
            Some(s2) if !delete => s2,
            _ => &set1,
        };
        for &b in sq_set {
            sq[b as usize] = true;
        }
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut buf = [0u8; 64 * 1024];
    let mut last: Option<u8> = None;

    loop {
        let n = match stdin.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("tr: {e}");
                return 1;
            }
        };
        let mut out = Vec::with_capacity(n);
        for &b in &buf[..n] {
            if del[b as usize] {
                continue;
            }
            let b2 = map[b as usize].unwrap_or(b);
            if squeeze && sq[b2 as usize] && last == Some(b2) {
                continue;
            }
            last = Some(b2);
            out.push(b2);
        }
        if let Err(e) = stdout.write_all(&out) {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            eprintln!("tr: {e}");
            return 1;
        }
    }
    0
}

fn expand_set(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // ranges a-z
        if i + 2 < bytes.len() && bytes[i + 1] == b'-' && bytes[i] < bytes[i + 2] {
            for b in bytes[i]..=bytes[i + 2] {
                out.push(b);
            }
            i += 3;
            continue;
        }
        // \ escapes
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let e = match bytes[i + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'\\' => b'\\',
                b'a' => 0x07,
                b'b' => 0x08,
                b'f' => 0x0c,
                b'v' => 0x0b,
                other => other,
            };
            out.push(e);
            i += 2;
            continue;
        }
        // [:class:]
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b':' {
            if let Some(end) = find_class_end(bytes, i) {
                let name = &s[i + 2..end];
                push_class(&mut out, name);
                i = end + 2; // skip :]
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn find_class_end(bytes: &[u8], start: usize) -> Option<usize> {
    // start at [:
    let mut j = start + 2;
    while j + 1 < bytes.len() {
        if bytes[j] == b':' && bytes[j + 1] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
}

fn push_class(out: &mut Vec<u8>, name: &str) {
    match name {
        "alnum" => {
            for b in 0..=255u8 {
                if (b as char).is_ascii_alphanumeric() {
                    out.push(b);
                }
            }
        }
        "alpha" => {
            for b in b'A'..=b'Z' {
                out.push(b);
            }
            for b in b'a'..=b'z' {
                out.push(b);
            }
        }
        "digit" => {
            for b in b'0'..=b'9' {
                out.push(b);
            }
        }
        "lower" => {
            for b in b'a'..=b'z' {
                out.push(b);
            }
        }
        "upper" => {
            for b in b'A'..=b'Z' {
                out.push(b);
            }
        }
        "space" => out.extend_from_slice(b" \t\n\r\x0b\x0c"),
        "blank" => out.extend_from_slice(b" \t"),
        "punct" => {
            for b in 0..=255u8 {
                if (b as char).is_ascii_punctuation() {
                    out.push(b);
                }
            }
        }
        "print" => {
            for b in 0x20..=0x7e {
                out.push(b);
            }
        }
        "graph" => {
            for b in 0x21..=0x7e {
                out.push(b);
            }
        }
        "cntrl" => {
            for b in 0..=0x1f {
                out.push(b);
            }
            out.push(0x7f);
        }
        "xdigit" => {
            out.extend_from_slice(b"0123456789ABCDEFabcdef");
        }
        _ => {}
    }
}
