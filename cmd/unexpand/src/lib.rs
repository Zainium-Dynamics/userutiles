//! user unexpand — convert spaces to tabs.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

pub fn run() -> i32 {
    let mut tabstop = 8usize;
    let mut all = false;
    let mut files: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: unexpand [OPTION]... [FILE]...\nConvert blanks in each FILE to tabs.\n -a, --all convert all blanks\n -t, --tabs=N tab stops every N (default 8)\n");
                return 0;
            }
            "--version" => {
                println!("unexpand (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => all = true,
            s if s.starts_with("-t") => {
                tabstop = s
                    .trim_start_matches("-t")
                    .trim_start_matches("--tabs=")
                    .parse()
                    .unwrap_or(8)
                    .max(1);
            }
            s if s.starts_with('-') && s != "-" => {
                eprintln!("unexpand: invalid option -- '{s}'");
                return 1;
            }
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        files.push("-".into());
    }
    let mut out = io::stdout().lock();
    for f in files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            match File::open(&f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    eprintln!("unexpand: {f}: {e}");
                    return 1;
                }
            }
        };
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            let converted = if all {
                spaces_to_tabs(&line, tabstop)
            } else {
                // only leading
                let n = line.chars().take_while(|c| *c == ' ').count();
                let tabs = n / tabstop;
                let rem = n % tabstop;
                format!("{}{}{}", "\t".repeat(tabs), " ".repeat(rem), &line[n..])
            };
            let _ = writeln!(out, "{converted}");
        }
    }
    0
}

fn spaces_to_tabs(line: &str, tabstop: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    let mut space_run = 0usize;
    for ch in line.chars() {
        if ch == ' ' {
            space_run += 1;
            col += 1;
            if col % tabstop == 0 {
                out.push('\t');
                space_run = 0;
            }
        } else {
            out.push_str(&" ".repeat(space_run));
            space_run = 0;
            if ch == '\t' {
                col = (col / tabstop + 1) * tabstop;
            } else {
                col += 1;
            }
            out.push(ch);
        }
    }
    out.push_str(&" ".repeat(space_run));
    out
}
