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
                let d = args.get(i).map(|s| s.as_str()).unwrap_or("\n");
                delimiter = Some(if d == "\\0" {
                    0
                } else {
                    d.as_bytes().first().copied().unwrap_or(b'\n')
                });
            }
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

    let items = match read_items(null, delimiter) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("xargs: {e}");
            return 1;
        }
    };

    if items.is_empty() && no_run_if_empty {
        return 0;
    }

    if let Some(ref token) = replace {
        // one invocation per item
        let mut status = 0;
        for item in &items {
            let argv: Vec<String> = cmd.iter().map(|a| a.replace(token, item)).collect();
            status |= run_cmd(&argv, verbose);
        }
        return status;
    }

    let batch = max_args.or(max_lines).unwrap_or(usize::MAX).max(1);
    let mut status = 0;
    if items.is_empty() {
        status |= run_cmd(&cmd, verbose);
    } else {
        for chunk in items.chunks(batch) {
            let mut argv = cmd.clone();
            argv.extend(chunk.iter().cloned());
            status |= run_cmd(&argv, verbose);
        }
    }
    status
}

fn read_items(null: bool, delimiter: Option<u8>) -> io::Result<Vec<String>> {
    let mut stdin = io::stdin().lock();
    if null || delimiter == Some(0) {
        let mut data = Vec::new();
        stdin.read_to_end(&mut data)?;
        return Ok(data
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect());
    }
    if let Some(d) = delimiter {
        let mut data = Vec::new();
        stdin.read_to_end(&mut data)?;
        return Ok(data
            .split(|b| *b == d)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect());
    }
    // whitespace separated (default)
    let mut items = Vec::new();
    for line in stdin.lines() {
        let line = line?;
        for tok in line.split_whitespace() {
            items.push(tok.to_string());
        }
    }
    Ok(items)
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
        Ok(st) => {
            if st.success() {
                0
            } else {
                st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(1))
            }
        }
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
