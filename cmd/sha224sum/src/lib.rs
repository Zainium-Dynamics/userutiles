//! user sha224sum — print or check 224-bit checksums.
use std::io;

pub fn run() -> i32 {
    let mut check = false;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
 "Usage: sha224sum [OPTION]... [FILE]...\n Print or check 224-bit checksums.\n -c, --check read checksums from the FILEs and check them\n With no FILE, or when FILE is -, read standard input.\n"
 );
                return 0;
            }
            "--version" => {
                println!("sha224sum (user_utils) 0.1.0");
                return 0;
            }
            "-c" | "--check" => check = true,
            s if s.starts_with('-') && s != "-" => {
                eprintln!("sha224sum: invalid option -- '{s}'");
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }
    if check {
        return check_mode(&files);
    }
    let mut status = 0;
    for f in &files {
        match hash_file(f) {
            Ok(h) => println!("{h} {f}"),
            Err(e) => {
                eprintln!("sha224sum: {f}: {e}");
                status = 1;
            }
        }
    }
    status
}

fn hash_file(path: &str) -> io::Result<String> {
    let mut h = usercore::digest::Sha224::new();
    usercore::digest::hash_path_update(path, |chunk| h.update(chunk))?;
    Ok(usercore::digest::hex_lower(&h.finalize()))
}

fn check_mode(files: &[String]) -> i32 {
    let mut status = 0;
    for list in files {
        let data = if list == "-" {
            let mut s = String::new();
            let _ = io::Read::read_to_string(&mut io::stdin(), &mut s);
            s
        } else {
            match std::fs::read_to_string(list) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("sha224sum: {list}: {e}");
                    status = 1;
                    continue;
                }
            }
        };
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (hash, file) = if let Some((h, f)) = line.split_once(" ") {
                (h, f)
            } else if let Some((h, f)) = line.split_once(" *") {
                (h, f)
            } else {
                eprintln!("sha224sum: invalid line");
                status = 1;
                continue;
            };
            match hash_file(file) {
                Ok(h) if h.eq_ignore_ascii_case(hash) => println!("{file}: OK"),
                Ok(_) => {
                    println!("{file}: FAILED");
                    status = 1;
                }
                Err(e) => {
                    eprintln!("sha224sum: {file}: {e}");
                    status = 1;
                }
            }
        }
    }
    status
}
