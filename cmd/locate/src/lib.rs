//! user locate — list files in databases that match a pattern.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;

use usercore::Ui;

/// Entry point for the `locate` utility. Parses `std::env::args()` and
/// searches one or more `updatedb`-format databases (default resolved via
/// [`usercore::zainium::default_locate_db`]) for entries matching every
/// given PATTERN (glob if it contains `*`/`?`/`[`, substring otherwise).
///
/// Returns 0 if at least one match was found, 1 otherwise (including on a
/// usage error).
pub fn run() -> i32 {
    let ui = Ui::new("locate");
    let mut ignore_case = false;
    let mut basename = false;
    let mut existing = false;
    let mut limit: Option<usize> = None;
    let mut null = false;
    let mut count_only = false;
    let mut dbs: Vec<PathBuf> = Vec::new();
    let mut patterns: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!(
                    "Usage: locate [OPTION]... PATTERN...\n\
 Search a database built by updatedb.\n\n\
 -i, --ignore-case ignore case distinctions\n\
 -b, --basename match only the file name portion\n\
 -e, --existing only print entries for currently existing files\n\
 -l, --limit=N limit output (or counting) to N entries\n\
 -0, --null separate output with NUL\n\
 -c, --count only print number of found entries\n\
 -d, --database=DB database path (repeatable)\n\
 --help display this help\n\
 --version output version\n\n\
 Default DB from LOCATE_PATH / ZEX_LOCATEDB / $ZEX_PREFIX/var/lib/misc/locatedb\n"
                );
                return 0;
            }
            "--version" => {
                println!("locate (user_utils) 0.1.0");
                return 0;
            }
            "-i" | "--ignore-case" => ignore_case = true,
            "-b" | "--basename" => basename = true,
            "-e" | "--existing" => existing = true,
            "-0" | "--null" => null = true,
            "-c" | "--count" => count_only = true,
            "-l" | "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok());
            }
            s if s.starts_with("--limit=") => limit = s["--limit=".len()..].parse().ok(),
            s if s.starts_with("-l") && s.len() > 2 => limit = s[2..].parse().ok(),
            "-d" | "--database" => {
                i += 1;
                if let Some(d) = args.get(i) {
                    dbs.push(PathBuf::from(d));
                }
            }
            s if s.starts_with("--database=") => {
                dbs.push(PathBuf::from(&s["--database=".len()..]));
            }
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => patterns.push(other.to_string()),
        }
        i += 1;
    }

    if patterns.is_empty() {
        ui.err("no pattern given");
        return 1;
    }

    if dbs.is_empty() {
        // LOCATE_PATH can be multi
        if let Ok(lp) = std::env::var("LOCATE_PATH") {
            for p in lp.split(':').filter(|s| !s.is_empty()) {
                dbs.push(PathBuf::from(p));
            }
        }
        if dbs.is_empty() {
            dbs.push(usercore::zainium::default_locate_db());
        }
    }

    let mut found = 0usize;
    let mut out = io::stdout().lock();
    let end = if null { "\0" } else { "\n" };

    for db in &dbs {
        let file = match File::open(db) {
            Ok(f) => f,
            Err(e) => {
                ui.err(&format!("{}: {e}", db.display()));
                continue;
            }
        };
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let hay = if basename {
                std::path::Path::new(&line)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| line.clone())
            } else {
                line.clone()
            };
            let matched = patterns.iter().any(|pat| match_pat(&hay, pat, ignore_case));
            if !matched {
                continue;
            }
            if existing && !std::path::Path::new(&line).exists() {
                continue;
            }
            found += 1;
            if !count_only {
                let _ = write!(out, "{line}{end}");
            }
            if let Some(n) = limit {
                if found >= n {
                    break;
                }
            }
        }
        if let Some(n) = limit {
            if found >= n {
                break;
            }
        }
    }

    if count_only {
        println!("{found}");
    }
    if found == 0 {
        1
    } else {
        0
    }
}

/// Test whether `pat` matches `text`: glob matching (`*`/`?`) if `pat`
/// contains a glob metacharacter, substring containment otherwise (GNU
/// `locate`'s default). Note: `[...]` character classes are treated as a
/// glob trigger but are not actually special-cased by [`glob_match`], so
/// a literal `[` in the pattern must match a literal `[` in `text`.
fn match_pat(text: &str, pat: &str, ignore_case: bool) -> bool {
    // If pattern has glob metacharacters, use glob; else substring (GNU locate default).
    let is_glob = pat.contains('*') || pat.contains('?') || pat.contains('[');
    if is_glob {
        if ignore_case {
            glob_match(&pat.to_ascii_lowercase(), &text.to_ascii_lowercase())
        } else {
            glob_match(pat, text)
        }
    } else if ignore_case {
        text.to_ascii_lowercase()
            .contains(&pat.to_ascii_lowercase())
    } else {
        text.contains(pat)
    }
}

/// Shell-glob match `text` against `pat`, supporting `*` (any run of
/// characters, including empty) and `?` (any single character). Iterative
/// backtracking implementation (classic "star position" algorithm) —
/// linear space, no recursion blowup on pathological inputs.
fn glob_match(pat: &str, text: &str) -> bool {
    fn rec(p: &[u8], t: &[u8]) -> bool {
        let mut pi = 0;
        let mut ti = 0;
        let mut star_p = None;
        let mut star_t = 0;
        while ti < t.len() {
            if pi < p.len() && (p[pi] == t[ti] || p[pi] == b'?') {
                pi += 1;
                ti += 1;
            } else if pi < p.len() && p[pi] == b'*' {
                star_p = Some(pi);
                star_t = ti;
                pi += 1;
            } else if let Some(sp) = star_p {
                pi = sp + 1;
                star_t += 1;
                ti = star_t;
            } else {
                return false;
            }
        }
        while pi < p.len() && p[pi] == b'*' {
            pi += 1;
        }
        pi == p.len()
    }
    rec(pat.as_bytes(), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_exact_literal() {
        assert!(glob_match("abc", "abc"));
        assert!(!glob_match("abc", "abd"));
    }

    #[test]
    fn glob_match_star_wildcard() {
        assert!(glob_match("a*c", "abc"));
        assert!(glob_match("a*c", "ac"));
        assert!(glob_match("*.txt", "notes.txt"));
        assert!(!glob_match("*.txt", "notes.md"));
    }

    #[test]
    fn glob_match_question_wildcard() {
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
    }

    #[test]
    fn glob_match_empty_pattern_matches_empty_text_only() {
        assert!(glob_match("", ""));
        assert!(!glob_match("", "x"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn match_pat_substring_default() {
        assert!(match_pat("/home/alice/file.txt", "alice", false));
        assert!(!match_pat("/home/alice/file.txt", "bob", false));
    }

    #[test]
    fn match_pat_glob_triggered_by_metachar() {
        assert!(match_pat("/home/alice/file.txt", "*.txt", false));
        assert!(!match_pat("/home/alice/file.txt", "*.md", false));
    }

    #[test]
    fn match_pat_ignore_case() {
        assert!(match_pat("/Home/Alice", "alice", true));
        assert!(!match_pat("/Home/Alice", "alice", false));
    }
}
