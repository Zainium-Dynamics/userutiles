//! user xargs — build and execute command lines from standard input.
use std::io::{self, BufRead, Read};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut null = false;
    let mut max_args: Option<usize> = None;
    let mut max_lines: Option<usize> = None;
    let mut no_run_if_empty = false;
    let mut verbose = false;
    let mut replace: Option<String> = None;
    let mut delimiter: Option<u8> = None;
    let mut cmd: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: xargs [OPTION]... [COMMAND [INITIAL-ARGS]...]\n\
 Build and execute command lines from standard input.\n\n\
 -0, --null items are separated by NUL\n\
 -n, --max-args=MAX-ARGS use at most MAX-ARGS arguments per command line\n\
 -L, --max-lines=MAX-LINES use at most MAX-LINES nonblank input lines\n\
 -r, --no-run-if-empty if there are no arguments, do not run COMMAND\n\
 -I, --replace[=R] replace R (default {{}}) in INITIAL-ARGS\n\
 -d, --delimiter=DELIM input items are terminated by DELIM\n\
 -t, --verbose print commands before executing\n\
 --help display this help\n\
 --version output version\n"
                );
                return 0;
            }
            "--version" => {
                println!("xargs (user_utils) 0.1.0");
                return 0;
            }
            "-0" | "--null" => null = true,
            "-r" | "--no-run-if-empty" => no_run_if_empty = true,
            "-t" | "--verbose" => verbose = true,
            "-n" | "--max-args" => {
                i += 1;
                max_args = args.get(i).and_then(|s| s.parse().ok());
            }
            s if s.starts_with("-n") && s.len() > 2 => max_args = s[2..].parse().ok(),
            "-L" | "--max-lines" => {
                i += 1;
                max_lines = args.get(i).and_then(|s| s.parse().ok());
            }
            "-I" | "--replace" => {
                replace = Some("{}".into());
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    // optional R may be attached as -I{}
                }
            }
            s if s.starts_with("-I") && s.len() > 2 => replace = Some(s[2..].to_string()),
            s if s.starts_with("--replace=") => replace = Some(s["--replace=".len()..].to_string()),
            "-d" | "--delimiter" => {
                i += 1;
                delimiter = Some(delimiter_byte(args.get(i).map(|s| s.as_str()).unwrap_or("\\n")));
            }
            s if s.starts_with("--delimiter=") => delimiter = Some(delimiter_byte(&s["--delimiter=".len()..])),
            s if s.starts_with("-d") && s.len() > 2 => delimiter = Some(delimiter_byte(&s[2..])),
            s if s.starts_with("-L") && s.len() > 2 => max_lines = s[2..].parse().ok(),
            s if s.starts_with("--max-lines=") => max_lines = s["--max-lines=".len()..].parse().ok(),
            s if s.starts_with("--max-args=") => max_args = s["--max-args=".len()..].parse().ok(),
            // one command at a time here
            "-P" | "--max-procs" => i += 1,
            s if s.starts_with("-P") || s.starts_with("--max-procs=") => {}
            "--" => {
                cmd.extend(args[i + 1..].iter().cloned());
                break;
            }
            s if s.starts_with('-') => {
                eprintln!("xargs: invalid option -- '{s}'");
                return 1;
            }
            other => {
                cmd.push(other.to_string());
                cmd.extend(args[i + 1..].iter().cloned());
                break;
            }
        }
        i += 1;
    }
    if cmd.is_empty() {
        cmd.push("echo".into());
    }

    let lines = match read_items(null, delimiter, replace.is_some()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("xargs: {e}");
            return 1;
        }
    };

    let items: Vec<String> = lines.iter().flatten().cloned().collect();
    if items.is_empty() && no_run_if_empty {
        return 0;
    }

    // GNU's exit status: 123 when a command failed, 124/125/126/127 stop at once
    let mut status = 0;
    let mut run = |argv: &[String]| -> bool {
        let code = run_cmd(argv, verbose);
        status = status.max(code);
        code < 124
    };

    if let Some(ref token) = replace {
        // -I: one command per input line, the whole line in place of TOKEN
        for item in &items {
            let argv: Vec<String> = cmd.iter().map(|a| a.replace(token, item)).collect();
            if !run(&argv) {
                break;
            }
        }
        return status;
    }

    if items.is_empty() {
        run(&cmd);
    } else if let (None, Some(n)) = (max_args, max_lines) {
        // -L: N input lines per command
        for chunk in lines.chunks(n.max(1)) {
            let mut argv = cmd.clone();
            argv.extend(chunk.iter().flatten().cloned());
            if !run(&argv) {
                break;
            }
        }
    } else {
        for chunk in items.chunks(max_args.unwrap_or(usize::MAX).max(1)) {
            let mut argv = cmd.clone();
            argv.extend(chunk.iter().cloned());
            if !run(&argv) {
                break;
            }
        }
    }
    status
}

/// `-d`'s argument: one character, or an escape (`\n`, `\t`, `\0`).
fn delimiter_byte(d: &str) -> u8 {
    match d {
        "\\n" => b'\n',
        "\\t" => b'\t',
        "\\0" => 0,
        _ => d.as_bytes().first().copied().unwrap_or(b'\n'),
    }
}

/// Default input: blank-separated items, with '...' and "..." grouping and a backslash
/// taking the next character as it is, one list per input line (for -L).
fn split_items(line: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut have = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                have = true;
            }
            (None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                have = true;
            }
            (None, c) if c.is_whitespace() => {
                if have {
                    items.push(std::mem::take(&mut cur));
                    have = false;
                }
            }
            (None, c) => {
                cur.push(c);
                have = true;
            }
        }
    }
    if have {
        items.push(cur);
    }
    items
}

/// The input items, grouped by input line (each -0/-d item is its own line). With -I a
/// whole line, leading blanks dropped, is one item.
fn read_items(null: bool, delimiter: Option<u8>, whole_lines: bool) -> io::Result<Vec<Vec<String>>> {
    let mut stdin = io::stdin().lock();
    if null || delimiter == Some(0) {
        let mut data = Vec::new();
        stdin.read_to_end(&mut data)?;
        return Ok(data
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| vec![String::from_utf8_lossy(s).into_owned()])
            .collect());
    }
    if let Some(d) = delimiter {
        let mut data = Vec::new();
        stdin.read_to_end(&mut data)?;
        return Ok(data
            .split(|b| *b == d)
            .filter(|s| !s.is_empty())
            .map(|s| vec![String::from_utf8_lossy(s).into_owned()])
            .collect());
    }
    // whitespace separated (default)
    let mut lines = Vec::new();
    for line in stdin.lines() {
        let line = line?;
        if whole_lines {
            let l = line.trim_start();
            if !l.is_empty() {
                lines.push(vec![l.to_string()]);
            }
        } else {
            let items = split_items(&line);
            if !items.is_empty() {
                lines.push(items);
            }
        }
    }
    Ok(lines)
}

fn run_cmd(argv: &[String], verbose: bool) -> i32 {
    if argv.is_empty() {
        return 0;
    }
    if verbose {
        eprintln!("{}", argv.join(" "));
    }
    match Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(st) => match st.code() {
            Some(0) => 0,
            Some(255) => {
                eprintln!("xargs: {}: exited with status 255; aborting", argv[0]);
                124
            }
            Some(_) => 123,
            None => {
                eprintln!("xargs: {}: terminated by signal {}", argv[0], st.signal().unwrap_or(0));
                125
            }
        },
        Err(e) => {
            eprintln!("xargs: {}: {e}", argv[0]);
            if e.kind() == io::ErrorKind::NotFound {
                127
            } else {
                126
            }
        }
    }
}
