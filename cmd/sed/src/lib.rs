//! user sed — stream editor (subset: s/// g i p d =).
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use std::path::Path;
use usercore::{protect, Ui};

/// Entry point for the `sed` utility. Parses `std::env::args()` for a
/// script (`-e SCRIPT` or the first bare operand) and zero or more input
/// files (stdin if none given), then applies the script line by line.
///
/// Supports a subset of sed: `s/pat/repl/[g]`, `d`, `p`, and `=`.
///
/// Returns 0 on success, 1 if the script fails to parse or any file
/// fails to process.
pub fn run() -> i32 {
    let ui = Ui::new("sed");
    let mut expr: Option<String> = None;
    let mut in_place = false;
    let mut quiet = false;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: sed [OPTION]... {{script-only-if-no-other-script}} [input-file]...\n -e script add script\n -i edit files in place\n -n suppress automatic printing\nSupported: s/pat/repl/[g] d p =\n");
                return 0;
            }
            "--version" => {
                println!("sed (user_utils) 0.1.0");
                return 0;
            }
            "-n" | "--quiet" | "--silent" => quiet = true,
            "-i" | "--in-place" => in_place = true,
            "-e" | "--expression" => {
                i += 1;
                let Some(e) = args.get(i) else {
                    ui.err("option requires an argument -- 'e'");
                    return 1;
                };
                expr = Some(e.clone());
            }
            s if s.starts_with("-e") && s.len() > 2 => expr = Some(s[2..].to_string()),
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => {
                if expr.is_none() {
                    expr = Some(other.to_string());
                } else {
                    files.push(other.to_string());
                }
            }
        }
        i += 1;
    }
    let Some(script) = expr else {
        ui.err("no input script");
        return 1;
    };
    let cmds = match parse_script(&script) {
        Ok(c) => c,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };
    if files.is_empty() {
        files.push("-".into());
    }
    let mut status = 0;
    for f in files {
        if let Err(e) = process_file(&f, &cmds, quiet, in_place) {
            ui.err(&format!("{f}: {e}"));
            status = 1;
        }
    }
    status
}

#[derive(Clone)]
enum Cmd {
    Subst {
        pat: String,
        repl: String,
        global: bool,
    },
    Delete,
    Print,
    Eq,
}

/// Parse a `;`-separated sequence of `sed` commands from `s`. Supports
/// `s<delim>PAT<delim>REPL<delim>[g]`, `d`, `p`, and `=`. The substitute
/// command's delimiter may be any character (traditionally `/`); a
/// backslash immediately before the delimiter escapes it inside the
/// pattern/replacement.
fn parse_script(s: &str) -> Result<Vec<Cmd>, String> {
    let mut cmds = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        if let Some(body) = rest.strip_prefix('s') {
            let delim = body.chars().next().ok_or("invalid s command")?;
            let body = &body[delim.len_utf8()..];
            let parts: Vec<&str> = split_delim(body, delim);
            if parts.len() < 2 {
                return Err("invalid substitute".into());
            }
            let pat = parts[0].to_string();
            let repl = parts[1].to_string();
            // Only characters immediately after the closing delimiter that
            // are recognized flags belong to this command — anything past
            // that (e.g. a `;`-separated next command) must be left in
            // `rest` for the next iteration, not swallowed as "flags".
            let flags_and_rest = if parts.len() > 2 { parts[2] } else { "" };
            let flag_len: usize = flags_and_rest
                .chars()
                .take_while(|c| {
                    matches!(c, 'g' | 'p' | 'i' | 'I' | 'm' | 'M') || c.is_ascii_digit()
                })
                .map(|c| c.len_utf8())
                .sum();
            let flags = &flags_and_rest[..flag_len];
            let global = flags.contains('g');
            let delim_count = if parts.len() > 2 { 3 } else { 2 };
            let consumed = 1 + delim.len_utf8() * delim_count + pat.len() + repl.len() + flag_len;
            cmds.push(Cmd::Subst { pat, repl, global });
            rest = rest
                .get(consumed..)
                .unwrap_or("")
                .trim_start_matches(';')
                .trim();
        } else if let Some(after) = rest.strip_prefix('d') {
            cmds.push(Cmd::Delete);
            rest = after.trim_start_matches(';').trim();
        } else if let Some(after) = rest.strip_prefix('p') {
            cmds.push(Cmd::Print);
            rest = after.trim_start_matches(';').trim();
        } else if let Some(after) = rest.strip_prefix('=') {
            cmds.push(Cmd::Eq);
            rest = after.trim_start_matches(';').trim();
        } else {
            return Err(format!("unsupported script near '{rest}'"));
        }
    }
    Ok(cmds)
}

/// Split `s` on unescaped occurrences of delimiter `d` (a `\` immediately
/// before `d` escapes it and is skipped rather than splitting), returning
/// each segment between delimiters plus the trailing remainder.
fn split_delim(s: &str, d: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (idx, c) = chars[i];
        if c == '\\' {
            i += 2;
            continue;
        }
        if c == d {
            parts.push(&s[start..idx]);
            start = idx + c.len_utf8();
        }
        i += 1;
    }
    parts.push(&s[start..]);
    parts
}

/// Run `cmds` over each line of `path` (`-` for stdin), writing to
/// stdout. `p` and `=` always print (matching GNU `sed`, where `-n` only
/// suppresses the automatic end-of-cycle print, not explicit commands);
/// the automatic print itself is skipped when `quiet` is set. When
/// `in_place` is set and `path` isn't `-`, the transformed lines are
/// written back to `path` after processing.
fn process_file(path: &str, cmds: &[Cmd], quiet: bool, in_place: bool) -> io::Result<()> {
    if in_place && path != "-" {
        if let Some(reason) = protect::modification_denied(Path::new(path)) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason.message()));
        }
    }
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    let mut out_lines = Vec::new();
    let mut stdout = io::stdout().lock();
    for (idx, line) in reader.lines().enumerate() {
        let mut line = line?;
        let mut deleted = false;
        for cmd in cmds {
            match cmd {
                Cmd::Subst { pat, repl, global } => {
                    if *global {
                        line = line.replace(pat, repl);
                    } else {
                        line = line.replacen(pat, repl, 1);
                    }
                }
                Cmd::Delete => {
                    deleted = true;
                    break;
                }
                Cmd::Print => {
                    writeln!(stdout, "{line}")?;
                }
                Cmd::Eq => {
                    writeln!(stdout, "{}", idx + 1)?;
                }
            }
        }
        if deleted {
            continue;
        }
        if !quiet {
            writeln!(stdout, "{line}")?;
        }
        if in_place {
            out_lines.push(line);
        }
    }
    if in_place && path != "-" {
        let mut f = File::create(path)?;
        for l in out_lines {
            writeln!(f, "{l}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_script_single_substitute() {
        let cmds = parse_script("s/foo/bar/").unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            &cmds[0],
            Cmd::Subst { pat, repl, global } if pat == "foo" && repl == "bar" && !global
        ));
    }

    #[test]
    fn parse_script_global_flag() {
        let cmds = parse_script("s/a/b/g").unwrap();
        assert!(matches!(&cmds[0], Cmd::Subst { global: true, .. }));
    }

    #[test]
    fn parse_script_custom_delimiter() {
        let cmds = parse_script("s#foo#bar#").unwrap();
        assert!(matches!(
            &cmds[0],
            Cmd::Subst { pat, repl, .. } if pat == "foo" && repl == "bar"
        ));
    }

    #[test]
    fn parse_script_chained_commands() {
        let cmds = parse_script("s/a/b/;d").unwrap();
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[1], Cmd::Delete));
    }

    #[test]
    fn parse_script_print_and_eq() {
        let cmds = parse_script("p;=").unwrap();
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], Cmd::Print));
        assert!(matches!(cmds[1], Cmd::Eq));
    }

    #[test]
    fn parse_script_rejects_unsupported_command() {
        assert!(parse_script("y/a/b/").is_err());
    }

    #[test]
    fn parse_script_rejects_incomplete_substitute() {
        assert!(parse_script("s/foo").is_err());
    }

    #[test]
    fn parse_script_rejects_bare_s() {
        assert!(parse_script("s").is_err());
    }

    #[test]
    fn split_delim_handles_escaped_delimiter() {
        let parts = split_delim(r"a\/b/c", '/');
        assert_eq!(parts, vec![r"a\/b", "c"]);
    }

    #[test]
    fn split_delim_no_delimiter_returns_whole_string() {
        let parts = split_delim("abc", '/');
        assert_eq!(parts, vec!["abc"]);
    }

    #[test]
    fn process_file_substitute_roundtrip() -> io::Result<()> {
        let dir = std::env::temp_dir().join(format!("user_sed_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("in.txt");
        std::fs::write(&path, "foo\nbar\n")?;
        let cmds = parse_script("s/foo/baz/").unwrap();
        // Just verify no error is returned; stdout content isn't captured
        // here, so this exercises the read + substitute + write path.
        let result = process_file(path.to_str().unwrap(), &cmds, false, false);
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn process_file_missing_file_errors() {
        let result = process_file("/nonexistent/user-sed-missing", &[], false, false);
        assert!(result.is_err());
    }
}
