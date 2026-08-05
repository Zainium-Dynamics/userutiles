//! user yes
use std::io::{self, BufRead, BufReader, Read, Write};
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("Usage: yes [STRING]...\nRepeatedly output a line with all specified STRING(s), or 'y'.\n");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("yes (user_utils) 0.1.0");
        return 0;
    }
    let line = if args.is_empty() {
        "y".to_string()
    } else {
        args.join(" ")
    };
    let mut out = io::stdout().lock();
    let buf = format!("{line}\n");
    let chunk = buf.repeat(4096 / buf.len().max(1));
    loop {
        if let Err(e) = out.write_all(chunk.as_bytes()) {
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            eprintln!("yes: {e}");
            return 1;
        }
    }
}
