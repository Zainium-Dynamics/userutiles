//! user grep — print lines matching patterns (POSIX BRE by default, -E ERE, -F fixed).
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use usercore::posix_regex::Regex;

#[derive(Default)]
struct Opts {
    extended: bool,
    fixed: bool,
    perl: bool,
    icase: bool,
    invert: bool,
    word: bool,
    line: bool,
    count: bool,
    files_with: bool,
    files_without: bool,
    number: bool,
    quiet: bool,
    silent_errors: bool,
    only: bool,
    with_name: Option<bool>,
    recursive: bool,
    max: Option<usize>,
    after: usize,
    before: usize,
    null: bool,
    byte_offset: bool,
    nul_data: bool,
    include: Vec<String>,
    exclude: Vec<String>,
    exclude_dir: Vec<String>,
}

impl Opts {
    fn set_kind(&mut self, c: char) {
        self.extended = c == 'E';
        self.fixed = c == 'F';
        self.perl = c == 'P';
    }
}

const HELP: &str = "Usage: grep [OPTION]... PATTERNS [FILE]...\n  -E, -F, -G, -P       extended / fixed-string / basic / Perl patterns (default -G)\n  -e PAT, -f FILE      use PAT / patterns from FILE (repeatable)\n  -i, -v, -w, -x       ignore case / invert / whole words / whole lines\n  -c, -l, -L, -q, -s   count / files with / files without / quiet / no file errors\n  -n, -b, -o           line numbers / byte offsets / only matching\n  -h, -H               no names / with names\n  -m NUM               stop after NUM matches\n  -A, -B, -C NUM       context after / before / around\n  -r, -R               recurse into directories (--include, --exclude, --exclude-dir)\n  -Z, --null           NUL after file names\n  -z, --null-data      lines end in NUL, not newline\n";

pub fn run() -> i32 {
    // a closed pipe (`| head`) ends the program quietly instead of panicking
    // SAFETY: restoring the default SIGPIPE action has no preconditions.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let mut o = Opts::default();
    let mut patterns: Vec<String> = Vec::new();
    let mut have_pattern = false;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut opts_done = false;

    macro_rules! value {
        ($name:expr) => {{
            i += 1;
            match args.get(i) {
                Some(v) => v.clone(),
                None => {
                    eprintln!("grep: option requires an argument -- '{}'", $name);
                    return 2;
                }
            }
        }};
    }
    macro_rules! num {
        ($v:expr) => {
            match $v.parse::<usize>() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("grep: invalid number: '{}'", $v);
                    return 2;
                }
            }
        };
    }

    while i < args.len() {
        let a = args[i].clone();
        if opts_done || a == "-" || !a.starts_with('-') {
            if !have_pattern {
                patterns.extend(a.split('\n').map(str::to_string));
                have_pattern = true;
            } else {
                files.push(a);
            }
            i += 1;
            continue;
        }
        if let Some(long) = a.strip_prefix("--") {
            let (key, val) = match long.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (long, None),
            };
            match key {
                "" => opts_done = true,
                "help" => {
                    print!("{HELP}");
                    return 0;
                }
                "version" => {
                    println!("grep (user_utils) 0.2.0");
                    return 0;
                }
                "extended-regexp" => o.set_kind('E'),
                "fixed-strings" => o.set_kind('F'),
                "basic-regexp" => o.set_kind('G'),
                "perl-regexp" => o.set_kind('P'),
                "byte-offset" => o.byte_offset = true,
                "null-data" => o.nul_data = true,
                "ignore-case" => o.icase = true,
                "no-ignore-case" => o.icase = false,
                "invert-match" => o.invert = true,
                "word-regexp" => o.word = true,
                "line-regexp" => o.line = true,
                "count" => o.count = true,
                "files-with-matches" => o.files_with = true,
                "files-without-match" => o.files_without = true,
                "line-number" => o.number = true,
                "quiet" | "silent" => o.quiet = true,
                "no-messages" => o.silent_errors = true,
                "only-matching" => o.only = true,
                "with-filename" => o.with_name = Some(true),
                "no-filename" => o.with_name = Some(false),
                "recursive" | "dereference-recursive" => o.recursive = true,
                "null" => o.null = true,
                "color" | "colour" | "line-buffered" | "binary-files" | "text" => {}
                "regexp" | "file" | "max-count" | "after-context" | "before-context" | "context" | "include"
                | "exclude" | "exclude-dir" => {
                    let v = match val {
                        Some(v) => v,
                        None => value!(key),
                    };
                    match key {
                        "regexp" => {
                            patterns.push(v);
                            have_pattern = true;
                        }
                        "file" => match read_patterns(&v) {
                            Ok(p) => {
                                patterns.extend(p);
                                have_pattern = true;
                            }
                            Err(e) => {
                                eprintln!("grep: {v}: {e}");
                                return 2;
                            }
                        },
                        "max-count" => o.max = Some(num!(v)),
                        "after-context" => o.after = num!(v),
                        "before-context" => o.before = num!(v),
                        "context" => {
                            o.after = num!(v);
                            o.before = o.after;
                        }
                        "include" => o.include.push(v),
                        "exclude" => o.exclude.push(v),
                        _ => o.exclude_dir.push(v),
                    }
                }
                _ => {
                    eprintln!("grep: unrecognized option '{a}'");
                    return 2;
                }
            }
            i += 1;
            continue;
        }

        // short options, possibly combined (-inE, -A3, -e PAT, -ePAT)
        let chars: Vec<char> = a[1..].chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            let rest: String = chars[k + 1..].iter().collect();
            let takes_value = matches!(c, 'e' | 'f' | 'm' | 'A' | 'B' | 'C');
            if takes_value {
                let v = if rest.is_empty() { value!(c) } else { rest };
                match c {
                    'e' => {
                        patterns.push(v);
                        have_pattern = true;
                    }
                    'f' => match read_patterns(&v) {
                        Ok(p) => {
                            patterns.extend(p);
                            have_pattern = true;
                        }
                        Err(e) => {
                            eprintln!("grep: {v}: {e}");
                            return 2;
                        }
                    },
                    'm' => o.max = Some(num!(v)),
                    'A' => o.after = num!(v),
                    'B' => o.before = num!(v),
                    _ => {
                        o.after = num!(v);
                        o.before = o.after;
                    }
                }
                break;
            }
            match c {
                'E' | 'F' | 'G' | 'P' => o.set_kind(c),
                'b' => o.byte_offset = true,
                'z' => o.nul_data = true,
                'i' | 'y' => o.icase = true,
                'v' => o.invert = true,
                'w' => o.word = true,
                'x' => o.line = true,
                'c' => o.count = true,
                'l' => o.files_with = true,
                'L' => o.files_without = true,
                'n' => o.number = true,
                'q' => o.quiet = true,
                's' => o.silent_errors = true,
                'o' => o.only = true,
                'h' => o.with_name = Some(false),
                'H' => o.with_name = Some(true),
                'r' | 'R' => o.recursive = true,
                'Z' => o.null = true,
                'a' | 'I' | 'U' => {}
                c if c.is_ascii_digit() => {
                    let digits: String = chars[k..].iter().take_while(|c| c.is_ascii_digit()).collect();
                    o.after = num!(digits);
                    o.before = o.after;
                    k += digits.len();
                    continue;
                }
                _ => {
                    eprintln!("grep: invalid option -- '{c}'");
                    eprintln!("Usage: grep [OPTION]... PATTERNS [FILE]...");
                    return 2;
                }
            }
            k += 1;
        }
        i += 1;
    }

    if !have_pattern {
        eprintln!("Usage: grep [OPTION]... PATTERNS [FILE]...");
        return 2;
    }
    let matcher = match Matcher::new(&patterns, &o) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("grep: {e}");
            return 2;
        }
    };
    if files.is_empty() {
        files.push(if o.recursive { ".".into() } else { "-".into() });
    }

    let mut inputs: Vec<String> = Vec::new();
    let mut status = 1;
    for f in &files {
        if o.recursive && f != "-" && Path::new(f).is_dir() {
            walk(Path::new(f), &o, &mut inputs);
        } else if f != "-" && Path::new(f).is_dir() {
            if !o.silent_errors {
                eprintln!("grep: {f}: Is a directory");
            }
        } else {
            inputs.push(f.clone());
        }
    }
    let show_name = o.with_name.unwrap_or(inputs.len() > 1 || o.recursive);

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for f in &inputs {
        match grep_one(f, &matcher, &o, show_name, &mut out) {
            Ok(true) => {
                status = if status == 2 { 2 } else { 0 };
                if o.quiet {
                    let _ = out.flush();
                    return 0;
                }
            }
            Ok(false) => {}
            Err(e) => {
                if !o.silent_errors {
                    let _ = out.flush();
                    eprintln!("grep: {f}: {e}");
                }
                status = 2;
            }
        }
    }
    let _ = out.flush();
    status
}

fn read_patterns(path: &str) -> io::Result<Vec<String>> {
    let text = if path == "-" {
        let mut s = String::new();
        io::Read::read_to_string(&mut io::stdin(), &mut s)?;
        s
    } else {
        std::fs::read_to_string(path)?
    };
    Ok(text.lines().map(str::to_string).collect())
}

fn glob_match(pat: &str, name: &str) -> bool {
    fn m(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    m(&pat.chars().collect::<Vec<_>>(), &name.chars().collect::<Vec<_>>())
}

fn walk(dir: &Path, o: &Opts, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        if !o.silent_errors {
            eprintln!("grep: {}: Permission denied", dir.display());
        }
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !o.exclude_dir.iter().any(|g| glob_match(g, &name)) {
                walk(&p, o, out);
            }
        } else if ft.is_file() || (ft.is_symlink() && p.is_file()) {
            if !o.include.is_empty() && !o.include.iter().any(|g| glob_match(g, &name)) {
                continue;
            }
            if o.exclude.iter().any(|g| glob_match(g, &name)) {
                continue;
            }
            out.push(p.to_string_lossy().into_owned());
        }
    }
}

/// Search one input; Ok(true) when it had a selected line.
fn grep_one(path: &str, m: &Matcher, o: &Opts, show_name: bool, out: &mut dyn Write) -> io::Result<bool> {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    let label = if path == "-" { "(standard input)" } else { path };
    let eol = if o.nul_data { b'\0' } else { b'\n' };
    let name = |out: &mut dyn Write, sep: char| -> io::Result<()> {
        if show_name {
            write!(out, "{label}")?;
            if o.null {
                write!(out, "\0")?;
            } else {
                write!(out, "{sep}")?;
            }
        }
        Ok(())
    };
    // name, line number and byte offset; `sep` is ':' on selected lines, '-' on context lines
    let prefix = |out: &mut dyn Write, sep: char, lineno: usize, offset: usize| -> io::Result<()> {
        name(out, sep)?;
        if o.number {
            write!(out, "{lineno}{sep}")?;
        }
        if o.byte_offset {
            write!(out, "{offset}{sep}")?;
        }
        Ok(())
    };
    let emit = |out: &mut dyn Write, text: &str| -> io::Result<()> {
        out.write_all(text.as_bytes())?;
        out.write_all(&[eol])
    };

    let mut selected = 0usize;
    let mut before: std::collections::VecDeque<(usize, usize, String)> = std::collections::VecDeque::new();
    let mut after_left = 0usize;
    let mut last_printed: Option<usize> = None;
    let context = o.after > 0 || o.before > 0;
    let listing = o.quiet || o.files_with || o.files_without || o.count;
    let mut offset = 0usize;

    for (idx, raw) in reader.split(eol).enumerate() {
        let raw = raw?;
        let line_off = offset;
        offset += raw.len() + 1;
        let line = String::from_utf8_lossy(&raw).into_owned();
        let lineno = idx + 1;
        let hit = m.is_match(&line) != o.invert;
        if hit {
            selected += 1;
            if listing {
                if o.quiet || o.files_with || o.files_without {
                    break;
                }
            } else {
                if context {
                    if let Some(lp) = last_printed {
                        let first = before.front().map(|(n, _, _)| *n).unwrap_or(lineno);
                        if first > lp + 1 {
                            writeln!(out, "--")?;
                        }
                    } else if selected > 1 {
                        writeln!(out, "--")?;
                    }
                    for (n, off, l) in before.drain(..) {
                        prefix(out, '-', n, off)?;
                        emit(out, &l)?;
                    }
                }
                if o.only && !o.invert {
                    for (start, part) in m.find_all(&line) {
                        prefix(out, ':', lineno, line_off + start)?;
                        emit(out, part)?;
                    }
                } else if !o.only {
                    prefix(out, ':', lineno, line_off)?;
                    emit(out, &line)?;
                }
                last_printed = Some(lineno);
                after_left = o.after;
            }
            if o.max.is_some_and(|mx| selected >= mx) {
                break;
            }
        } else if !listing && context {
            if after_left > 0 {
                after_left -= 1;
                prefix(out, '-', lineno, line_off)?;
                emit(out, &line)?;
                last_printed = Some(lineno);
            } else if o.before > 0 {
                before.push_back((lineno, line_off, line));
                if before.len() > o.before {
                    before.pop_front();
                }
            }
        }
    }

    if o.count {
        name(out, ':')?;
        writeln!(out, "{selected}")?;
    }
    if (o.files_with && selected > 0) || (o.files_without && selected == 0) {
        write!(out, "{label}")?;
        if o.null { write!(out, "\0")? } else { writeln!(out)? }
    }
    Ok(selected > 0)
}

enum Matcher {
    Fixed { needles: Vec<String>, icase: bool, word: bool, line: bool },
    Re { res: Vec<Regex>, word: bool },
    Perl { res: Vec<fancy_regex::Regex>, word: bool },
}

impl Matcher {
    fn new(patterns: &[String], o: &Opts) -> Result<Self, String> {
        if o.fixed {
            let needles = patterns.iter().map(|p| if o.icase { p.to_lowercase() } else { p.clone() }).collect();
            return Ok(Matcher::Fixed { needles, icase: o.icase, word: o.word, line: o.line });
        }
        if o.perl {
            let res = patterns
                .iter()
                .map(|p| {
                    let p = if o.line { format!("^(?:{p})$") } else { p.clone() };
                    fancy_regex::RegexBuilder::new(&p).case_insensitive(o.icase).build().map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Matcher::Perl { res, word: o.word });
        }
        let res = patterns
            .iter()
            .map(|p| {
                let p = if o.line {
                    if o.extended { format!("^({p})$") } else { format!("^\\({p}\\)$") }
                } else {
                    p.clone()
                };
                Regex::new(&p, o.extended, o.icase)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Matcher::Re { res, word: o.word })
    }

    fn is_match(&self, line: &str) -> bool {
        !self.spans(line, true).is_empty()
    }

    /// Non-empty matches with their byte offsets in the line.
    fn find_all<'a>(&self, line: &'a str) -> Vec<(usize, &'a str)> {
        self.spans(line, false).into_iter().filter(|(a, b)| b > a).map(|(a, b)| (a, &line[a..b])).collect()
    }

    /// Matched byte ranges, in order; with `first_only` it stops at the first one.
    fn spans(&self, line: &str, first_only: bool) -> Vec<(usize, usize)> {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let word_ok = |a: usize, b: usize| {
            let before = line[..a].chars().next_back().is_none_or(|c| !is_word(c));
            let after = line[b..].chars().next().is_none_or(|c| !is_word(c));
            before && after
        };
        let mut found: Vec<(usize, usize)> = Vec::new();
        match self {
            Matcher::Fixed { needles, icase, word, line: whole } => {
                let hay = if *icase { line.to_lowercase() } else { line.to_string() };
                for n in needles {
                    if *whole {
                        if hay == *n {
                            found.push((0, line.len()));
                        }
                        continue;
                    }
                    let mut from = 0;
                    while let Some(pos) = hay[from..].find(n.as_str()) {
                        let (a, b) = (from + pos, from + pos + n.len());
                        if !*word || word_ok(a, b) {
                            found.push((a, b));
                            if first_only {
                                return found;
                            }
                        }
                        from = if b > a { b } else { a + 1 };
                        if from > hay.len() {
                            break;
                        }
                    }
                }
            }
            Matcher::Re { res, word, .. } => {
                for re in res {
                    let mut from = 0;
                    while from <= line.len() {
                        let Some(m) = re.find(line, from) else { break };
                        let Some((a, b)) = m[0] else { break };
                        if !*word || word_ok(a, b) {
                            found.push((a, b));
                            if first_only {
                                return found;
                            }
                        }
                        from = if b > a { b } else { a + line[a..].chars().next().map_or(1, char::len_utf8) };
                    }
                }
            }
            Matcher::Perl { res, word } => {
                for re in res {
                    let mut from = 0;
                    while from <= line.len() {
                        let Ok(Some(m)) = re.find_from_pos(line, from) else { break };
                        let (a, b) = (m.start(), m.end());
                        if !*word || word_ok(a, b) {
                            found.push((a, b));
                            if first_only {
                                return found;
                            }
                        }
                        from = if b > a { b } else { a + line[a..].chars().next().map_or(1, char::len_utf8) };
                    }
                }
            }
        }
        found.sort();
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &[&str], f: impl Fn(&mut Opts)) -> Matcher {
        let mut o = Opts::default();
        f(&mut o);
        Matcher::new(&p.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &o).unwrap()
    }

    #[test]
    fn basic_vs_extended() {
        assert!(m(&["a+"], |_| {}).is_match("xa+y"));
        assert!(!m(&["a+"], |_| {}).is_match("aaa"));
        assert!(m(&["a+"], |o| o.extended = true).is_match("aaa"));
        assert!(m(&["foo|bar"], |o| o.extended = true).is_match("xbar"));
        assert!(m(&["^build_version = .*"], |_| {}).is_match("build_version = \"1\""));
    }

    #[test]
    fn multiple_patterns_word_line_icase() {
        let mm = m(&["foo", "bar"], |_| {});
        assert!(mm.is_match("bar") && mm.is_match("foo") && !mm.is_match("baz"));
        assert!(!m(&["foo"], |o| o.word = true).is_match("foobar"));
        assert!(m(&["foo"], |o| o.word = true).is_match("a foo b"));
        assert!(!m(&["foo"], |o| o.line = true).is_match("foo bar"));
        assert!(m(&["FOO"], |o| o.icase = true).is_match("foo"));
        assert!(m(&["a.b"], |o| o.fixed = true).is_match("xa.b"));
        assert!(!m(&["a.b"], |o| o.fixed = true).is_match("axb"));
    }

    #[test]
    fn only_matching_parts() {
        assert_eq!(m(&["[0-9][0-9]*"], |_| {}).find_all("a1b22c"), vec![(1, "1"), (3, "22")]);
        assert_eq!(m(&[r"\d+(?= bar)"], |o| o.perl = true).find_all("foo12 bar"), vec![(3, "12")]);
        assert!(m(&[r"(o)\1"], |o| o.perl = true).is_match("foo"));
    }

    #[test]
    fn globs() {
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(!glob_match("*.rs", "lib.rsx"));
    }
}
