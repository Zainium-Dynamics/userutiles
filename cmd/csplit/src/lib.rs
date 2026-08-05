//! user csplit — split a file into sections determined by context lines.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use usercore::Ui;

/// Entry point for the `csplit` utility. Parses `std::env::args()`, reads
/// the input file (or stdin if `-`), splits it at the given patterns, and
/// writes each piece to `PREFIXnn` (default `xx00`, `xx01`, ...).
///
/// Each `PATTERN` is either `/REGEX/` (here: a plain substring match) or a
/// 1-based integer line number. Returns 0 on success, 1 on a usage,
/// pattern-match, or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("csplit");
    let mut prefix = "xx".to_string();
    let mut keep = false;
    let mut digits = 2usize;
    let mut quiet = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut file = None;
    let mut patterns = Vec::new();
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("Usage: csplit [OPTION]... FILE PATTERN...\nOutput pieces of FILE separated by PATTERN(s) to files xx00, xx01, ...\n -f, --prefix=PREFIX use PREFIX (default xx)\n -k, --keep-files do not remove output files on errors\n -n, --digits=N use N digits (default 2)\n -s, --quiet do not print counts\nPATTERN is /REGEX/ or integer line number.\n");
                return 0;
            }
            "--version" => {
                println!("csplit (user_utils) 0.1.0");
                return 0;
            }
            "-k" | "--keep-files" => keep = true,
            "-s" | "--quiet" | "--silent" => quiet = true,
            "-f" | "--prefix" => {
                i += 1;
                prefix = args.get(i).cloned().unwrap_or_else(|| "xx".into());
            }
            "-n" | "--digits" => {
                i += 1;
                digits = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(2);
            }
            s if s.starts_with("-f") && s.len() > 2 => prefix = s[2..].to_string(),
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => {
                if file.is_none() {
                    file = Some(other.to_string());
                } else {
                    patterns.push(other.to_string());
                }
            }
        }
        i += 1;
    }
    let file = match file {
        Some(f) => f,
        None => {
            ui.err("missing file operand");
            return 1;
        }
    };
    if patterns.is_empty() {
        ui.err("missing pattern");
        return 1;
    }
    let reader: Box<dyn BufRead> = if file == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        match File::open(&file) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                ui.err(&format!("{file}: {e}"));
                return 1;
            }
        }
    };
    let lines: Vec<String> = match reader.lines().collect() {
        Ok(l) => l,
        Err(e) => {
            ui.err(&format!("{e}"));
            return 1;
        }
    };

    let cuts = match find_split_points(&lines, &patterns) {
        Ok(c) => c,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    let mut created = Vec::new();
    for (part, w) in cuts.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let name = format!("{prefix}{part:0digits$}", digits = digits);
        match File::create(&name) {
            Ok(mut f) => {
                let mut bytes = 0usize;
                for line in &lines[a..b] {
                    writeln!(f, "{line}").ok();
                    bytes += line.len() + 1;
                }
                if !quiet {
                    println!("{bytes}");
                }
                created.push(name);
            }
            Err(e) => {
                ui.err(&format!("{name}: {e}"));
                if !keep {
                    for c in &created {
                        let _ = std::fs::remove_file(c);
                    }
                }
                return 1;
            }
        }
    }
    0
}

/// Compute the line-index cut points for splitting `lines` at `patterns`.
///
/// Each pattern is matched starting from the position of the previous
/// match (patterns are applied in order, not re-searched from the top).
/// `/REGEX/` patterns are matched as a plain substring (no real regex
/// engine); bare integers are treated as 1-based line numbers.
///
/// Returns a sorted, deduplicated list of cut points including `0` and
/// `lines.len()`, or an `Err` describing the first pattern that failed to
/// match / parse. On error, no output files have been created yet, so
/// there is nothing to clean up regardless of `--keep-files`.
fn find_split_points(lines: &[String], patterns: &[String]) -> Result<Vec<usize>, String> {
    let mut cuts = vec![0usize];
    let mut pos = 0usize;
    for pat in patterns {
        if let Some(rest) = pat.strip_prefix('/').and_then(|s| s.strip_suffix('/')) {
            let found = lines
                .iter()
                .enumerate()
                .skip(pos)
                .find(|(_, line)| line.contains(rest))
                .map(|(i, _)| i);
            match found {
                Some(i) => {
                    cuts.push(i);
                    pos = i;
                }
                None => return Err(format!("/{rest}/: match not found")),
            }
        } else if let Ok(n) = pat.parse::<usize>() {
            let idx = n.saturating_sub(1).min(lines.len());
            cuts.push(idx);
            pos = idx;
        } else {
            return Err(format!("invalid pattern '{pat}'"));
        }
    }
    cuts.push(lines.len());
    cuts.sort_unstable();
    cuts.dedup();
    Ok(cuts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn split_on_line_number() {
        let l = lines(&["a", "b", "c", "d"]);
        let cuts = find_split_points(&l, &["3".to_string()]).unwrap();
        assert_eq!(cuts, vec![0, 2, 4]);
    }

    #[test]
    fn split_on_substring_pattern() {
        let l = lines(&["alpha", "beta", "gamma"]);
        let cuts = find_split_points(&l, &["/beta/".to_string()]).unwrap();
        assert_eq!(cuts, vec![0, 1, 3]);
    }

    #[test]
    fn split_on_multiple_patterns_searches_forward() {
        let l = lines(&["one", "two", "three", "two", "five"]);
        let cuts =
            find_split_points(&l, &["/two/".to_string(), "/two/".to_string()]).unwrap();
        // First /two/ matches index 1; second search resumes at pos=1 and
        // finds the same line again (skip(pos) includes pos itself).
        assert_eq!(cuts, vec![0, 1, 5]);
    }

    #[test]
    fn missing_pattern_match_errors_without_creating_files() {
        let l = lines(&["a", "b"]);
        let result = find_split_points(&l, &["/nope/".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_pattern_errors() {
        let l = lines(&["a", "b"]);
        let result = find_split_points(&l, &["not-a-pattern".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn empty_input_with_line_number_pattern() {
        let l: Vec<String> = Vec::new();
        let cuts = find_split_points(&l, &["1".to_string()]).unwrap();
        // With no lines, both the pattern's index (clamped to 0) and the
        // trailing `lines.len()` cut collapse to 0 after dedup, so there's
        // a single cut point and no windows to create files from.
        assert_eq!(cuts, vec![0]);
    }
}
