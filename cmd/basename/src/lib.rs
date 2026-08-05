//! user basename — strip directory and suffix from filenames.
use std::path::Path;

use usercore::Ui;

/// Entry point for the `basename` utility. Parses `std::env::args()` and
/// prints the final path component of each `NAME` operand, optionally
/// stripping a trailing suffix (`-s`) and/or handling multiple operands
/// (`-a`, or implicitly when `-s` is given).
///
/// Returns 0 on success, 1 on a usage error (missing operand, unknown
/// option, or too many operands without `-a`/`-s`).
pub fn run() -> i32 {
    let ui = Ui::new("basename");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        if args.is_empty() {
            ui.err("missing operand");
            return 1;
        }
        print!("Usage: basename NAME [SUFFIX]\n basename OPTION... NAME...\n -a, --multiple support multiple arguments\n -s, --suffix=S remove trailing suffix S\n -z, --zero end output with NUL\n");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("basename (user_utils) 0.1.0");
        return 0;
    }

    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    let end = if parsed.zero { "\0" } else { "\n" };
    for n in &parsed.names {
        print!("{}{end}", strip_name(n, parsed.suffix.as_deref()));
    }
    0
}

/// Parsed command-line state for `basename` (everything but `--help`/
/// `--version`, which `run` handles before reaching here).
#[derive(Debug)]
struct Parsed {
    zero: bool,
    suffix: Option<String>,
    names: Vec<String>,
}

/// Parse operands and options out of `args` (already stripped of
/// `argv[0]`). Applies the historic two-operand form (`basename NAME
/// SUFFIX`) when exactly two bare names are given and neither `-a` nor
/// `-s` was used.
fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut multiple = false;
    let mut zero = false;
    let mut suffix: Option<String> = None;
    let mut names: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-a" | "--multiple" => multiple = true,
            "-z" | "--zero" => zero = true,
            "-s" | "--suffix" => {
                i += 1;
                let Some(s) = args.get(i) else {
                    return Err("option requires an argument -- 's'".into());
                };
                suffix = Some(s.clone());
                multiple = true;
            }
            s if s.starts_with("--suffix=") => {
                suffix = Some(s["--suffix=".len()..].to_string());
                multiple = true;
            }
            s if s.starts_with("-s") && s.len() > 2 => {
                suffix = Some(s[2..].to_string());
                multiple = true;
            }
            "--" => {
                names.extend(args[i + 1..].iter().cloned());
                break;
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("invalid option -- '{s}'"));
            }
            other => names.push(other.to_string()),
        }
        i += 1;
    }
    if names.is_empty() {
        return Err("missing operand".into());
    }
    // Historic form: `basename NAME SUFFIX`.
    if !multiple && names.len() == 2 && suffix.is_none() {
        suffix = names.pop();
    }
    if !multiple && names.len() > 1 {
        return Err(format!("extra operand '{}'", names[1]));
    }
    Ok(Parsed {
        zero,
        suffix,
        names,
    })
}

/// Compute the basename of `name`, stripping the trailing `suffix` (if
/// given and it's a proper, non-empty-result suffix of the basename).
fn strip_name(name: &str, suffix: Option<&str>) -> String {
    let base = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string());
    match suffix {
        Some(suf) if base.ends_with(suf) && base.len() > suf.len() => {
            base[..base.len() - suf.len()].to_string()
        }
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_name_plain_path() {
        assert_eq!(strip_name("/usr/bin/sort", None), "sort");
        assert_eq!(strip_name("sort", None), "sort");
    }

    #[test]
    fn strip_name_trailing_slash() {
        assert_eq!(strip_name("/usr/bin/", None), "bin");
    }

    #[test]
    fn strip_name_with_suffix() {
        assert_eq!(strip_name("include/stdio.h", Some(".h")), "stdio");
    }

    #[test]
    fn strip_name_suffix_equal_to_whole_basename_is_not_stripped() {
        // GNU basename: suffix must not consume the *entire* basename.
        assert_eq!(strip_name(".h", Some(".h")), ".h");
    }

    #[test]
    fn parse_args_single_name() {
        let p = parse_args(&["foo/bar".to_string()]).unwrap();
        assert_eq!(p.names, vec!["foo/bar".to_string()]);
        assert!(p.suffix.is_none());
        assert!(!p.zero);
    }

    #[test]
    fn parse_args_historic_two_operand_form() {
        let p = parse_args(&["stdio.h".to_string(), ".h".to_string()]).unwrap();
        assert_eq!(p.names, vec!["stdio.h".to_string()]);
        assert_eq!(p.suffix.as_deref(), Some(".h"));
    }

    #[test]
    fn parse_args_multiple_flag_allows_many_names() {
        let p = parse_args(&[
            "-a".to_string(),
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
        ])
        .unwrap();
        assert_eq!(p.names.len(), 3);
    }

    #[test]
    fn parse_args_too_many_operands_without_multiple_errors() {
        let err = parse_args(&["a".to_string(), "b".to_string(), "c".to_string()]).unwrap_err();
        assert!(err.contains("extra operand"));
    }

    #[test]
    fn parse_args_missing_operand_errors() {
        assert!(parse_args(&["-a".to_string()]).is_err());
    }

    #[test]
    fn parse_args_unknown_option_errors() {
        assert!(parse_args(&["--bogus".to_string(), "x".to_string()]).is_err());
    }

    #[test]
    fn parse_args_suffix_missing_argument_errors() {
        assert!(parse_args(&["x".to_string(), "-s".to_string()]).is_err());
    }

    #[test]
    fn parse_args_zero_flag() {
        let p = parse_args(&["-z".to_string(), "x".to_string()]).unwrap();
        assert!(p.zero);
    }
}
