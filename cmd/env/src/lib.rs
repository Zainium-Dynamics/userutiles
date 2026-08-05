//! user env — set environment and run command, or print environment.
use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

use usercore::Ui;

/// Entry point for the `env` utility. Parses `std::env::args()`; with no
/// `COMMAND` it prints the resulting environment (one `NAME=VALUE` per
/// line, or NUL-separated with `-0`), otherwise it `exec`s `COMMAND` with
/// that environment (never returning on success).
///
/// Returns 0 when printing the environment, 126/127 if `exec` fails
/// (matching the `env`(1) convention: 127 for "command not found", 126 for
/// other exec failures), or 1 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("env");
    let mut ignore_env = false;
    let mut null = false;
    let mut sets: Vec<(String, String)> = Vec::new();
    let mut unsets: Vec<String> = Vec::new();
    let mut cmd: Vec<String> = Vec::new();
    let mut parsing_opts = true;

    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if parsing_opts {
            match a.as_str() {
                "-h" | "--help" => {
                    print!(
                        "Usage: env [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]\n\
 Set each NAME to VALUE in the environment and run COMMAND.\n\n\
 -i, --ignore-environment start with an empty environment\n\
 -0, --null end each output line with NUL, not newline\n\
 -u, --unset=NAME remove variable from the environment\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
                    );
                    return 0;
                }
                "--version" => {
                    println!("env (user_utils) 0.1.0");
                    return 0;
                }
                "-" | "-i" | "--ignore-environment" => {
                    ignore_env = true;
                    i += 1;
                    continue;
                }
                "-0" | "--null" => {
                    null = true;
                    i += 1;
                    continue;
                }
                "-u" | "--unset" => {
                    i += 1;
                    let Some(name) = args.get(i) else {
                        ui.err("option requires an argument -- 'u'");
                        return 1;
                    };
                    unsets.push(name.clone());
                    i += 1;
                    continue;
                }
                s if s.starts_with("-u") && s.len() > 2 => {
                    unsets.push(s[2..].to_string());
                    i += 1;
                    continue;
                }
                s if s.contains('=') => {
                    if let Some((k, v)) = s.split_once('=') {
                        sets.push((k.to_string(), v.to_string()));
                    }
                    i += 1;
                    continue;
                }
                s if s.starts_with('-') => {
                    ui.err(&format!("invalid option -- '{s}'"));
                    return 1;
                }
                _ => {
                    parsing_opts = false;
                    // fall through to treat as command
                }
            }
        }
        if !parsing_opts || !a.starts_with('-') {
            if a.contains('=') && cmd.is_empty() {
                if let Some((k, v)) = a.split_once('=') {
                    sets.push((k.to_string(), v.to_string()));
                }
            } else {
                cmd.push(a.clone());
                // rest are args
                cmd.extend(args[i + 1..].iter().cloned());
                break;
            }
        }
        i += 1;
    }

    let base = if ignore_env { Vec::new() } else { env::vars().collect() };
    let env_map = build_env(base, &unsets, &sets);

    if cmd.is_empty() {
        print_env(&env_map, null);
        return 0;
    }

    let mut command = Command::new(&cmd[0]);
    if cmd.len() > 1 {
        command.args(&cmd[1..]);
    }
    command.env_clear();
    for (k, v) in &env_map {
        command.env(k, v);
    }
    let err = command.exec();
    ui.err(&format!("'{}': {err}", cmd[0]));
    if err.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// Build the final environment from a `base` variable list (pass an empty
/// `base` for `-i`/`--ignore-environment`), applying `-u`/`--unset`
/// removals and then `NAME=VALUE` sets (sets override an existing entry in
/// place, preserving its original position, or are appended).
fn build_env(
    base: Vec<(String, String)>,
    unsets: &[String],
    sets: &[(String, String)],
) -> Vec<(String, String)> {
    let mut env_map = base;
    env_map.retain(|(k, _)| !unsets.contains(k));
    for (k, v) in sets {
        if let Some(slot) = env_map.iter_mut().find(|(ek, _)| ek == k) {
            slot.1 = v.clone();
        } else {
            env_map.push((k.clone(), v.clone()));
        }
    }
    env_map
}

/// Print `env_map` as `NAME=VALUE` lines, NUL-terminated instead of
/// newline-terminated when `null` is set.
fn print_env(env_map: &[(String, String)], null: bool) {
    for (k, v) in env_map {
        if null {
            print!("{k}={v}\0");
        } else {
            println!("{k}={v}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_env_applies_sets_over_base() {
        let base = vec![("A".to_string(), "1".to_string())];
        let sets = vec![("A".to_string(), "2".to_string())];
        let result = build_env(base, &[], &sets);
        assert_eq!(result, vec![("A".to_string(), "2".to_string())]);
    }

    #[test]
    fn build_env_appends_new_vars() {
        let base = vec![("A".to_string(), "1".to_string())];
        let sets = vec![("B".to_string(), "2".to_string())];
        let result = build_env(base, &[], &sets);
        assert_eq!(
            result,
            vec![
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "2".to_string())
            ]
        );
    }

    #[test]
    fn build_env_unset_removes_var() {
        let base = vec![
            ("A".to_string(), "1".to_string()),
            ("B".to_string(), "2".to_string()),
        ];
        let result = build_env(base, &["A".to_string()], &[]);
        assert_eq!(result, vec![("B".to_string(), "2".to_string())]);
    }

    #[test]
    fn build_env_ignore_env_starts_empty() {
        let sets = vec![("A".to_string(), "1".to_string())];
        let result = build_env(Vec::new(), &[], &sets);
        assert_eq!(result, vec![("A".to_string(), "1".to_string())]);
    }

    #[test]
    fn build_env_empty_everything() {
        let result = build_env(Vec::new(), &[], &[]);
        assert!(result.is_empty());
    }
}
