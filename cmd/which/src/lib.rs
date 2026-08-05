//! user which — locate a command on PATH (Zainium: /overlayer/syshub/{bin,sbin}).
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn run() -> i32 {
    let mut all = false;
    let mut names: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: which [options] [--] COMMAND [...]\n\
 Write the full path of COMMAND(s) to standard output.\n\n\
 Search path is $PATH, or when unset:\n\
 {def}\n\n\
 -a, --all print all matching pathnames of each argument\n\
 --help display this help and exit\n\
 --version output version information and exit\n",
                    def = usercore::DEFAULT_PATH
                );
                return 0;
            }
            "--version" => {
                println!("which (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => all = true,
            "--" => {}
            s if s.starts_with('-') => {
                eprintln!("which: invalid option -- '{s}'");
                return 1;
            }
            other => names.push(other.to_string()),
        }
    }
    if names.is_empty() {
        eprintln!("which: missing operand");
        return 1;
    }

    let paths: Vec<PathBuf> = usercore::zainium::path_dirs();
    let mut status = 0;

    for name in &names {
        if name.contains('/') {
            let p = PathBuf::from(name);
            if is_executable(&p) {
                println!("{}", p.display());
            } else {
                status = 1;
            }
            continue;
        }
        let mut found = false;
        for dir in &paths {
            let cand = dir.join(name);
            if is_executable(&cand) {
                println!("{}", cand.display());
                found = true;
                if !all {
                    break;
                }
            }
        }
        if !found {
            status = 1;
        }
    }
    let _ = env::var_os("PATH"); // keep env used for docs clarity
    status
}

fn is_executable(path: &Path) -> bool {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if !meta.is_file() {
        return false;
    }
    meta.permissions().mode() & 0o111 != 0
}
