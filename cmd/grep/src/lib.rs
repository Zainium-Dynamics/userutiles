//! user grep — print lines matching a pattern (basic BRE/fixed subset).
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

pub fn run() -> i32 {
    let mut ignore_case = false;
    let mut invert = false;
    let mut count_only = false;
    let mut files_with = false;
    let mut line_number = false;
    let mut quiet = false;
    let mut fixed = false;
    let mut word = false;
    let mut pattern: Option<String> = None;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: grep [OPTION]... PATTERN [FILE]...\nSearch for PATTERN in each FILE.\n -i ignore case\n -v invert match\n -c count matches\n -n line number\n -l files with matches\n -q quiet\n -F fixed strings\n -w word match\n -e PATTERN\n");
                return 0;
            }
            "--version" => {
                println!("grep (user_utils) 0.1.0");
                return 0;
            }
            "-i" | "--ignore-case" => ignore_case = true,
            "-v" | "--invert-match" => invert = true,
            "-c" | "--count" => count_only = true,
            "-n" | "--line-number" => line_number = true,
            "-l" | "--files-with-matches" => files_with = true,
            "-q" | "--quiet" | "--silent" => quiet = true,
            "-F" | "--fixed-strings" => fixed = true,
            "-w" | "--word-regexp" => word = true,
            "-e" | "--regexp" => {
                i += 1;
                pattern = args.get(i).cloned();
            }
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'i' => ignore_case = true,
                        'v' => invert = true,
                        'c' => count_only = true,
                        'n' => line_number = true,
                        'l' => files_with = true,
                        'q' => quiet = true,
                        'F' => fixed = true,
                        'w' => word = true,
                        _ => {
                            eprintln!("grep: invalid option -- '{c}'");
                            return 2;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                eprintln!("grep: unrecognized option '{s}'");
                return 2;
            }
            other => {
                if pattern.is_none() {
                    pattern = Some(other.to_string());
                } else {
                    files.push(other.to_string());
                }
            }
        }
        i += 1;
    }
    let pattern = match pattern {
        Some(p) => p,
        None => {
            eprintln!("grep: missing pattern");
            return 2;
        }
    };
    let matcher = match Matcher::new(&pattern, ignore_case, fixed, word) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("grep: invalid pattern '{pattern}': {e}");
            return 2;
        }
    };
    if files.is_empty() {
        files.push("-".into());
    }
    let multi = files.len() > 1;
    let mut any = false;
    let mut status = 1; // 1 = no match
    let mut out = io::stdout().lock();
    for f in &files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    if !quiet {
                        eprintln!("grep: {f}: {e}");
                    }
                    status = 2;
                    continue;
                }
            }
        };
        let mut nmatch = 0usize;
        for (lineno, line) in reader.lines().enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    if !quiet {
                        eprintln!("grep: {e}");
                    }
                    status = 2;
                    break;
                }
            };
            let matched = matcher.is_match(&line);
            let show = if invert { !matched } else { matched };
            if show {
                nmatch += 1;
                any = true;
                if quiet {
                    return 0;
                }
                if files_with {
                    println!("{f}");
                    break;
                }
                if count_only {
                    continue;
                }
                if multi {
                    let _ = write!(out, "{f}:");
                }
                if line_number {
                    let _ = write!(out, "{}:", lineno + 1);
                }
                let _ = writeln!(out, "{line}");
            }
        }
        if count_only && !quiet {
            if multi {
                println!("{f}:{nmatch}");
            } else {
                println!("{nmatch}");
            }
        }
    }
    if status == 2 {
        2
    } else if any {
        0
    } else {
        1
    }
}

/// Compiled matcher for a single grep invocation — built once from CLI flags
/// rather than re-parsed per line, unlike the fixed-string / regex checks it
/// replaces.
///
/// `Fixed` is a fast substring/word check used for `-F` and for patterns with
/// no regex metacharacters. Anything containing metacharacters (`.`, `*`,
/// `+`, `?`, `[...]`, `(...)`, `|`, `^`, `$`, `\`) is compiled with the
/// `regex` crate instead of the previous hand-rolled engine, which silently
/// treated unsupported metacharacters (notably `[...]` character classes) as
/// literal text and produced wrong — not just unsupported — results.
enum Matcher {
    Fixed {
        needle: String,
        icase: bool,
        word: bool,
    },
    Regex(regex::Regex),
}

impl Matcher {
    fn new(
        pattern: &str,
        ignore_case: bool,
        fixed: bool,
        word: bool,
    ) -> Result<Self, regex::Error> {
        if fixed || !has_meta(pattern) {
            let needle = if ignore_case {
                pattern.to_ascii_lowercase()
            } else {
                pattern.to_string()
            };
            return Ok(Matcher::Fixed {
                needle,
                icase: ignore_case,
                word,
            });
        }
        let body = if word {
            format!(r"\b(?:{pattern})\b")
        } else {
            pattern.to_string()
        };
        let re = regex::RegexBuilder::new(&body)
            .case_insensitive(ignore_case)
            .build()?;
        Ok(Matcher::Regex(re))
    }

    fn is_match(&self, line: &str) -> bool {
        match self {
            Matcher::Fixed {
                needle,
                icase,
                word,
            } => {
                let owned;
                let l: &str = if *icase {
                    owned = line.to_ascii_lowercase();
                    &owned
                } else {
                    line
                };
                if *word {
                    l.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|w| w == needle)
                } else {
                    l.contains(needle.as_str())
                }
            }
            Matcher::Regex(re) => re.is_match(line),
        }
    }
}

fn has_meta(p: &str) -> bool {
    p.chars().any(|c| {
        matches!(
            c,
            '.' | '*' | '+' | '?' | '[' | '(' | '|' | '^' | '$' | '\\'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_class_matches_any_member() {
        // Regression: the old hand-rolled engine treated `[...]` as literal
        // characters, so "c[au]t" only matched the literal string "c[au]t".
        let m = Matcher::new("c[au]t", false, false, false).unwrap();
        assert!(m.is_match("cat"));
        assert!(m.is_match("cut"));
        assert!(!m.is_match("cot"));
        assert!(!m.is_match("c[au]t"));
    }

    #[test]
    fn plus_quantifier_means_one_or_more() {
        // Regression: the old engine had no `+` support and matched it as a
        // literal plus sign.
        let m = Matcher::new("ab+c", false, false, false).unwrap();
        assert!(m.is_match("abc"));
        assert!(m.is_match("abbbc"));
        assert!(!m.is_match("ac"));
    }

    #[test]
    fn question_quantifier_means_optional() {
        let m = Matcher::new("colou?r", false, false, false).unwrap();
        assert!(m.is_match("color"));
        assert!(m.is_match("colour"));
        assert!(!m.is_match("colouur"));
    }

    #[test]
    fn alternation_matches_either_branch() {
        let m = Matcher::new("foo|bar", false, false, false).unwrap();
        assert!(m.is_match("a foo b"));
        assert!(m.is_match("a bar b"));
        assert!(!m.is_match("a baz b"));
    }

    #[test]
    fn fixed_strings_flag_disables_regex_interpretation() {
        let m = Matcher::new("a.b", false, true, false).unwrap();
        assert!(m.is_match("xa.by"));
        assert!(!m.is_match("xaxby"), "with -F, '.' must be literal");
    }

    #[test]
    fn word_match_respects_boundaries_for_regex_path() {
        let m = Matcher::new("c[au]t", false, false, true).unwrap();
        assert!(m.is_match("a cat sat"));
        assert!(!m.is_match("concatenate"));
    }

    #[test]
    fn ignore_case_applies_to_both_matcher_kinds() {
        let fixed = Matcher::new("hello", true, false, false).unwrap();
        assert!(fixed.is_match("HELLO world"));

        let re = Matcher::new("h[ei]llo", true, false, false).unwrap();
        assert!(re.is_match("HELLO world"));
    }

    #[test]
    fn invalid_pattern_is_rejected_not_panicking() {
        assert!(Matcher::new("a(b", false, false, false).is_err());
    }
}
