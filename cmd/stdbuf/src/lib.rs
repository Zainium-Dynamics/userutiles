//! user stdbuf — run COMMAND with modified I/O buffering preferences.
//!
//! GNU stdbuf uses `LD_PRELOAD` + `libstdbuf`. This ZEX build records the
//! requested modes in the environment (`_STDBUF_I` / `_STDBUF_O` / `_STDBUF_E`,
//! GNU-compatible names) and then `exec`s COMMAND. Full effect requires a
//! preload helper; without it, this still provides a compatible CLI entry
//! point for scripts and for future libstdbuf integration.
use std::os::unix::process::CommandExt;
use std::process::Command;

use usercore::Ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Default,
    Line,
    Full(usize),
    None,
}

/// Entry point for the `stdbuf` utility. Parses `std::env::args()`, applies
/// `-i`/`-o`/`-e` buffering-mode requests via environment variables, then
/// `exec`s the requested command in place of this process.
///
/// Returns 125 on a usage error, 126 if the command could not be executed,
/// 127 if the command was not found, and otherwise never returns (a
/// successful `exec` replaces this process).
pub fn run() -> i32 {
    let ui = Ui::new("stdbuf");
    let mut i_mode = Mode::Default;
    let mut o_mode = Mode::Default;
    let mut e_mode = Mode::Default;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut cmd: Vec<String> = Vec::new();
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("stdbuf (user_utils) 0.1.0");
                return 0;
            }
            "-i" => {
                i += 1;
                match parse_mode(args.get(i).map(|s| s.as_str()).unwrap_or("L")) {
                    Ok(m) => i_mode = m,
                    Err(e) => {
                        ui.err(&e);
                        return 125;
                    }
                }
            }
            "-o" => {
                i += 1;
                match parse_mode(args.get(i).map(|s| s.as_str()).unwrap_or("L")) {
                    Ok(m) => o_mode = m,
                    Err(e) => {
                        ui.err(&e);
                        return 125;
                    }
                }
            }
            "-e" => {
                i += 1;
                match parse_mode(args.get(i).map(|s| s.as_str()).unwrap_or("L")) {
                    Ok(m) => e_mode = m,
                    Err(e) => {
                        ui.err(&e);
                        return 125;
                    }
                }
            }
            s if s.starts_with("-i") && s.len() > 2 => match parse_mode(&s[2..]) {
                Ok(m) => i_mode = m,
                Err(e) => {
                    ui.err(&e);
                    return 125;
                }
            },
            s if s.starts_with("-o") && s.len() > 2 => match parse_mode(&s[2..]) {
                Ok(m) => o_mode = m,
                Err(e) => {
                    ui.err(&e);
                    return 125;
                }
            },
            s if s.starts_with("-e") && s.len() > 2 => match parse_mode(&s[2..]) {
                Ok(m) => e_mode = m,
                Err(e) => {
                    ui.err(&e);
                    return 125;
                }
            },
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 125;
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
        ui.err("missing operand");
        return 125;
    }
    let mut command = Command::new(&cmd[0]);
    command.args(&cmd[1..]);
    if let Some(s) = mode_env(i_mode) {
        command.env("_STDBUF_I", s);
    }
    if let Some(s) = mode_env(o_mode) {
        command.env("_STDBUF_O", s);
    }
    if let Some(s) = mode_env(e_mode) {
        command.env("_STDBUF_E", s);
    }
    let err = command.exec();
    ui.err(&format!("failed to run command '{}': {err}", cmd[0]));
    if err.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

fn print_help() {
    print!(
        "Usage: stdbuf OPTION... COMMAND\n\
Run COMMAND with modified buffering of standard streams.\n\n\
  -i MODE   stdin buffering\n\
  -o MODE   stdout buffering\n\
  -e MODE   stderr buffering\n\n\
MODE is 'L' (line buffered), '0' (unbuffered), or a positive integer byte size.\n"
    );
}

/// Parse a `stdbuf` buffering-mode string (`L`, `0`, or a byte-count) into a
/// [`Mode`]. Unlike GNU `stdbuf`'s looser handling, an unrecognized mode
/// string is a hard error rather than silently falling back to line
/// buffering, so a typo (e.g. `-o Lx`) is surfaced instead of masked.
fn parse_mode(s: &str) -> Result<Mode, String> {
    match s {
        "L" | "l" => Ok(Mode::Line),
        "0" => Ok(Mode::None),
        other => match other.parse::<usize>() {
            Ok(0) => Ok(Mode::None),
            Ok(n) => Ok(Mode::Full(n)),
            Err(_) => Err(format!("invalid mode '{other}'")),
        },
    }
}

fn mode_env(m: Mode) -> Option<String> {
    match m {
        Mode::Default => None,
        Mode::Line => Some("L".into()),
        Mode::None => Some("0".into()),
        Mode::Full(n) => Some(n.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_line() {
        assert_eq!(parse_mode("L"), Ok(Mode::Line));
        assert_eq!(parse_mode("l"), Ok(Mode::Line));
    }

    #[test]
    fn parse_mode_none() {
        assert_eq!(parse_mode("0"), Ok(Mode::None));
    }

    #[test]
    fn parse_mode_full_size() {
        assert_eq!(parse_mode("4096"), Ok(Mode::Full(4096)));
    }

    #[test]
    fn parse_mode_rejects_garbage_instead_of_defaulting() {
        // Regression: previously fell back silently to `Mode::Line`.
        assert!(parse_mode("bogus").is_err());
        assert!(parse_mode("-1").is_err());
        assert!(parse_mode("1.5").is_err());
    }

    #[test]
    fn mode_env_roundtrip() {
        assert_eq!(mode_env(Mode::Default), None);
        assert_eq!(mode_env(Mode::Line), Some("L".into()));
        assert_eq!(mode_env(Mode::None), Some("0".into()));
        assert_eq!(mode_env(Mode::Full(8192)), Some("8192".into()));
    }
}
