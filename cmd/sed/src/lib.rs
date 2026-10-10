//! user sed — stream editor on POSIX regex (BRE by default, ERE with -E/-r).
//! Commands: s y d p q Q = a i c n { } with line, $, /re/ and range addresses, and `!`.
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use usercore::posix_regex::Regex;
use usercore::{protect, Ui};

/// Entry point: parse options and scripts, then stream every input through them.
pub fn run() -> i32 {
    // a closed pipe (`| head`) ends the program quietly instead of panicking
    // SAFETY: restoring the default SIGPIPE action has no preconditions.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let ui = Ui::new("sed");
    let mut scripts: Vec<String> = Vec::new();
    let mut in_place: Option<String> = None;
    let mut quiet = false;
    let mut extended = false;
    let mut separate = false;
    let mut files: Vec<String> = Vec::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut opts_done = false;
    while i < args.len() {
        let a = args[i].as_str();
        if opts_done || a == "-" || !a.starts_with('-') {
            files.push(a.to_string());
            i += 1;
            continue;
        }
        match a {
            "--" => opts_done = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("sed (user_utils) 0.2.0");
                return 0;
            }
            "-n" | "--quiet" | "--silent" => quiet = true,
            "-E" | "-r" | "--regexp-extended" => extended = true,
            "-s" | "--separate" => separate = true,
            "-u" | "--unbuffered" | "--posix" => {}
            "-i" | "--in-place" => in_place = Some(String::new()),
            "-e" | "--expression" | "-f" | "--file" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option requires an argument -- '{}'", &a[1..2]));
                    return 1;
                };
                if a.starts_with("-f") || a == "--file" {
                    match std::fs::read_to_string(v) {
                        Ok(s) => scripts.push(s),
                        Err(e) => {
                            ui.err(&format!("{v}: {e}"));
                            return 1;
                        }
                    }
                } else {
                    scripts.push(v.clone());
                }
            }
            s if s.starts_with("--in-place=") => in_place = Some(s["--in-place=".len()..].to_string()),
            s if s.starts_with("--expression=") => scripts.push(s["--expression=".len()..].to_string()),
            s if s.starts_with("-i") => in_place = Some(s[2..].to_string()),
            s if s.starts_with("-e") => scripts.push(s[2..].to_string()),
            s if s.starts_with("-f") => match std::fs::read_to_string(&s[2..]) {
                Ok(t) => scripts.push(t),
                Err(e) => {
                    ui.err(&format!("{}: {e}", &s[2..]));
                    return 1;
                }
            },
            // combined short flags such as -nE
            s if s.len() > 2 && s[1..].chars().all(|c| "nErsu".contains(c)) => {
                for c in s[1..].chars() {
                    match c {
                        'n' => quiet = true,
                        'E' | 'r' => extended = true,
                        's' => separate = true,
                        _ => {}
                    }
                }
            }
            s => {
                ui.err(&format!("invalid option -- '{}'", s.trim_start_matches('-')));
                return 1;
            }
        }
        i += 1;
    }
    if scripts.is_empty() {
        if files.is_empty() {
            ui.err("no input script");
            return 1;
        }
        scripts.push(files.remove(0));
    }
    let program = match Program::parse(&scripts.join("\n"), extended) {
        Ok(p) => p,
        Err(e) => {
            ui.err(&format!("-e expression: {e}"));
            return 1;
        }
    };
    if files.is_empty() {
        files.push("-".into());
    }

    let mut status = 0;
    if let Some(suffix) = in_place {
        for f in &files {
            if f == "-" {
                continue;
            }
            if let Err(e) = edit_in_place(f, &suffix, &program, quiet) {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
        return status;
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut state = State::new(program.cmds.len());
    if separate {
        for f in &files {
            match read_lines(f) {
                Ok(lines) => {
                    state.line_no = 0;
                    if let Ok(Flow::Quit(c)) = state.run(&program, lines, quiet, &mut out) {
                        let _ = out.flush();
                        return c;
                    }
                }
                Err(e) => {
                    ui.err(&format!("{f}: {e}"));
                    status = 2;
                }
            }
        }
    } else {
        let mut all = Vec::new();
        for f in &files {
            match read_lines(f) {
                Ok(lines) => all.extend(lines),
                Err(e) => {
                    ui.err(&format!("can't read {f}: {e}"));
                    status = 2;
                }
            }
        }
        match state.run(&program, all, quiet, &mut out) {
            Ok(Flow::Quit(c)) => {
                let _ = out.flush();
                return c;
            }
            Ok(_) => {}
            Err(e) => {
                ui.err(&e.to_string());
                status = 4;
            }
        }
    }
    let _ = out.flush();
    status
}

const HELP: &str = "Usage: sed [OPTION]... {script-only-if-no-other-script} [input-file]...\n  -n, --quiet          suppress automatic printing\n  -e, --expression=S   add script S\n  -f, --file=F         add the script in file F\n  -i[SUF], --in-place  edit files in place (backup with SUF)\n  -E, -r               extended regular expressions\n  -s, --separate       treat files as separate\nCommands: s y d p q Q = a i c n { } with N, $, /re/, ranges and !\n";

fn read_lines(path: &str) -> io::Result<Vec<String>> {
    let mut data = Vec::new();
    if path == "-" {
        io::stdin().read_to_end(&mut data)?;
    } else {
        File::open(path)?.read_to_end(&mut data)?;
    }
    let text = String::from_utf8_lossy(&data);
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    Ok(lines)
}

fn edit_in_place(path: &str, suffix: &str, program: &Program, quiet: bool) -> io::Result<()> {
    if let Some(reason) = protect::modification_denied(Path::new(path)) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason.message()));
    }
    let lines = read_lines(path)?;
    let mut buf: Vec<u8> = Vec::new();
    let mut state = State::new(program.cmds.len());
    state.run(program, lines, quiet, &mut buf)?;
    if !suffix.is_empty() {
        let backup = if suffix.contains('*') {
            let p = Path::new(path);
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let b = suffix.replace('*', &name);
            p.parent().map(|d| d.join(&b)).unwrap_or_else(|| b.into())
        } else {
            format!("{path}{suffix}").into()
        };
        std::fs::copy(path, backup)?;
    }
    let tmp = format!("{path}.sedtmp{}", std::process::id());
    std::fs::write(&tmp, &buf)?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path)
}

// ── script ────────────────────────────────────────────────────────────────────

enum Addr {
    Line(usize),
    Last,
    Re(Regex),
}

enum Op {
    Subst { re: Regex, repl: String, global: bool, nth: usize, print: bool },
    Translit(Vec<(char, char)>),
    Delete,
    Print,
    Quit { print: bool, code: i32 },
    LineNo,
    Append(String),
    Insert(String),
    Change(String),
    Next,
    Block(usize),
    EndBlock,
}

struct Cmd {
    a1: Option<Addr>,
    a2: Option<Addr>,
    negate: bool,
    op: Op,
}

pub struct Program {
    cmds: Vec<Cmd>,
}

impl Program {
    fn parse(src: &str, extended: bool) -> Result<Program, String> {
        let chars: Vec<char> = src.chars().collect();
        let mut p = Parser { s: chars, i: 0, extended };
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut opens: Vec<usize> = Vec::new();
        loop {
            p.skip(|c| c.is_whitespace() || c == ';');
            if p.done() {
                break;
            }
            if p.peek() == Some('}') {
                p.i += 1;
                let open = opens.pop().ok_or("unexpected `}'")?;
                let end = cmds.len();
                if let Op::Block(ref mut e) = cmds[open].op {
                    *e = end;
                }
                cmds.push(Cmd { a1: None, a2: None, negate: false, op: Op::EndBlock });
                continue;
            }
            if p.peek() == Some('#') {
                p.skip(|c| c != '\n');
                continue;
            }
            let a1 = p.addr()?;
            let mut a2 = None;
            if a1.is_some() && p.peek() == Some(',') {
                p.i += 1;
                a2 = Some(p.addr()?.ok_or("unexpected `,'")?);
            }
            p.skip(|c| c == ' ' || c == '\t');
            let mut negate = false;
            while p.peek() == Some('!') {
                negate = true;
                p.i += 1;
                p.skip(|c| c == ' ' || c == '\t');
            }
            let c = p.next().ok_or("missing command")?;
            let op = match c {
                '{' => {
                    opens.push(cmds.len());
                    Op::Block(0)
                }
                's' => p.subst()?,
                'y' => p.translit()?,
                'd' => Op::Delete,
                'p' => Op::Print,
                '=' => Op::LineNo,
                'n' => Op::Next,
                'q' | 'Q' => {
                    p.skip(|c| c == ' ' || c == '\t');
                    let n: String = p.take_while(|c| c.is_ascii_digit());
                    Op::Quit { print: c == 'q', code: n.parse().unwrap_or(0) }
                }
                'a' => Op::Append(p.text()),
                'i' => Op::Insert(p.text()),
                'c' => Op::Change(p.text()),
                other => return Err(format!("unknown command: `{other}'")),
            };
            let opens_block = matches!(op, Op::Block(_));
            cmds.push(Cmd { a1, a2, negate, op });
            if opens_block {
                continue;
            }
            p.skip(|c| c == ' ' || c == '\t');
            match p.peek() {
                None | Some(';') | Some('\n') | Some('}') | Some('#') => {}
                Some(c) => return Err(format!("extra characters after command: `{c}'")),
            }
        }
        if !opens.is_empty() {
            return Err("unmatched `{'".into());
        }
        Ok(Program { cmds })
    }
}

struct Parser {
    s: Vec<char>,
    i: usize,
    extended: bool,
}

impl Parser {
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        self.i += 1;
        c
    }
    fn skip(&mut self, f: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&f) {
            self.i += 1;
        }
    }
    fn take_while(&mut self, f: impl Fn(char) -> bool) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek().filter(|&c| f(c)) {
            out.push(c);
            self.i += 1;
        }
        out
    }

    fn addr(&mut self) -> Result<Option<Addr>, String> {
        match self.peek() {
            Some(c) if c.is_ascii_digit() => {
                let n = self.take_while(|c| c.is_ascii_digit());
                Ok(Some(Addr::Line(n.parse().map_err(|_| "bad line number")?)))
            }
            Some('$') => {
                self.i += 1;
                Ok(Some(Addr::Last))
            }
            Some('/') | Some('\\') => {
                let delim = if self.next() == Some('\\') { self.next().ok_or("bad address")? } else { '/' };
                let pat = self.delimited(delim, false)?;
                let icase = self.peek() == Some('I');
                if icase {
                    self.i += 1;
                }
                Ok(Some(Addr::Re(Regex::new(&pat, self.extended, icase)?)))
            }
            _ => Ok(None),
        }
    }

    /// Text up to an unescaped `delim`; `\delim` becomes `delim`, `\n` a newline in patterns.
    fn delimited(&mut self, delim: char, is_repl: bool) -> Result<String, String> {
        let mut out = String::new();
        loop {
            let c = self.next().ok_or_else(|| format!("unterminated `{delim}'"))?;
            if c == delim {
                return Ok(out);
            }
            if c == '\\' {
                let n = self.next().ok_or("trailing backslash")?;
                if n == delim && delim != '&' {
                    out.push(n);
                } else if n == 'n' && !is_repl {
                    out.push('\n');
                } else {
                    out.push('\\');
                    out.push(n);
                }
            } else {
                out.push(c);
            }
        }
    }

    fn subst(&mut self) -> Result<Op, String> {
        let delim = self.next().ok_or("unterminated `s' command")?;
        if delim == '\n' || delim == '\\' {
            return Err("bad delimiter".into());
        }
        let pat = self.delimited(delim, false)?;
        let repl = self.delimited(delim, true)?;
        let (mut global, mut nth, mut print, mut icase) = (false, 1usize, false, false);
        loop {
            match self.peek() {
                Some('g') => global = true,
                Some('p') => print = true,
                Some('i') | Some('I') => icase = true,
                Some('m') | Some('M') | Some('e') => {}
                Some(c) if c.is_ascii_digit() => {
                    let n = self.take_while(|c| c.is_ascii_digit());
                    nth = n.parse().map_err(|_| "bad occurrence")?;
                    continue;
                }
                Some('w') => return Err("`w' flag is not supported".into()),
                _ => break,
            }
            self.i += 1;
        }
        Ok(Op::Subst { re: Regex::new(&pat, self.extended, icase)?, repl, global, nth: nth.max(1), print })
    }

    fn translit(&mut self) -> Result<Op, String> {
        let delim = self.next().ok_or("unterminated `y' command")?;
        let unescape = |s: String| -> Vec<char> {
            let mut out = Vec::new();
            let mut it = s.chars();
            while let Some(c) = it.next() {
                if c == '\\' {
                    match it.next() {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some(o) => out.push(o),
                        None => out.push('\\'),
                    }
                } else {
                    out.push(c);
                }
            }
            out
        };
        let from = unescape(self.delimited(delim, true)?);
        let to = unescape(self.delimited(delim, true)?);
        if from.len() != to.len() {
            return Err("strings for `y' command are different lengths".into());
        }
        Ok(Op::Translit(from.into_iter().zip(to).collect()))
    }

    /// a/i/c text: GNU one-liner (`a text`) or POSIX (`a\` newline text, `\` continues lines).
    fn text(&mut self) -> String {
        self.skip(|c| c == ' ' || c == '\t');
        if self.peek() == Some('\\') {
            self.i += 1;
            if self.peek() == Some('\n') {
                self.i += 1;
            }
        }
        let mut out = String::new();
        while let Some(c) = self.next() {
            match c {
                '\\' => match self.next() {
                    Some('\n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => break,
                },
                '\n' => break,
                c => out.push(c),
            }
        }
        out
    }
}

// ── execution ─────────────────────────────────────────────────────────────────

pub enum Flow {
    Continue,
    Quit(i32),
}

struct State {
    line_no: usize,
    in_range: Vec<bool>,
}

impl State {
    fn new(n: usize) -> Self {
        State { line_no: 0, in_range: vec![false; n] }
    }

    fn matches(&mut self, idx: usize, cmd: &Cmd, line: &str, last: bool) -> bool {
        let hit = |a: &Addr, n: usize| match a {
            Addr::Line(l) => *l == n,
            Addr::Last => last,
            Addr::Re(r) => r.is_match(line),
        };
        let m = match (&cmd.a1, &cmd.a2) {
            (None, _) => true,
            (Some(a), None) => hit(a, self.line_no),
            (Some(a), Some(b)) => {
                if self.in_range[idx] {
                    let end = match b {
                        Addr::Line(l) => self.line_no >= *l,
                        other => hit(other, self.line_no),
                    };
                    if end {
                        self.in_range[idx] = false;
                    }
                    true
                } else if hit(a, self.line_no) {
                    let ends_now = matches!(b, Addr::Line(l) if *l <= self.line_no);
                    self.in_range[idx] = !ends_now;
                    true
                } else {
                    false
                }
            }
        };
        m != cmd.negate
    }

    fn run(&mut self, prog: &Program, lines: Vec<String>, quiet: bool, out: &mut dyn Write) -> io::Result<Flow> {
        let total = lines.len();
        let mut it = lines.into_iter().enumerate().peekable();
        while let Some((_, mut space)) = it.next() {
            self.line_no += 1;
            let mut last = it.peek().is_none() && total > 0;
            let mut append: Vec<String> = Vec::new();
            let mut deleted = false;
            let mut pc = 0;
            while pc < prog.cmds.len() {
                let cmd = &prog.cmds[pc];
                if matches!(cmd.op, Op::EndBlock) {
                    pc += 1;
                    continue;
                }
                if !self.matches(pc, cmd, &space, last) {
                    pc = match cmd.op {
                        Op::Block(end) => end + 1,
                        _ => pc + 1,
                    };
                    continue;
                }
                match &cmd.op {
                    Op::Block(_) | Op::EndBlock => {}
                    Op::Subst { re, repl, global, nth, print } => {
                        if let Some(new) = substitute(re, &space, repl, *global, *nth) {
                            space = new;
                            if *print {
                                writeln!(out, "{space}")?;
                            }
                        }
                    }
                    Op::Translit(map) => {
                        space = space
                            .chars()
                            .map(|c| map.iter().find(|(f, _)| *f == c).map(|(_, t)| *t).unwrap_or(c))
                            .collect();
                    }
                    Op::Delete => {
                        deleted = true;
                        break;
                    }
                    Op::Print => writeln!(out, "{space}")?,
                    Op::LineNo => writeln!(out, "{}", self.line_no)?,
                    Op::Append(t) => append.push(t.clone()),
                    Op::Insert(t) => writeln!(out, "{t}")?,
                    Op::Change(t) => {
                        let range_open = cmd.a2.is_some() && self.in_range[pc];
                        if !range_open {
                            writeln!(out, "{t}")?;
                        }
                        deleted = true;
                        break;
                    }
                    Op::Next => {
                        if !quiet {
                            writeln!(out, "{space}")?;
                        }
                        for a in append.drain(..) {
                            writeln!(out, "{a}")?;
                        }
                        match it.next() {
                            Some((_, l)) => {
                                space = l;
                                self.line_no += 1;
                                last = it.peek().is_none();
                            }
                            None => return Ok(Flow::Continue),
                        }
                    }
                    Op::Quit { print, code } => {
                        if *print && !quiet {
                            writeln!(out, "{space}")?;
                        }
                        for a in append.drain(..) {
                            writeln!(out, "{a}")?;
                        }
                        return Ok(Flow::Quit(*code));
                    }
                }
                pc += 1;
            }
            if !deleted && !quiet {
                writeln!(out, "{space}")?;
            }
            for a in append {
                writeln!(out, "{a}")?;
            }
        }
        Ok(Flow::Continue)
    }
}

/// Apply one s command; None when nothing matched.
fn substitute(re: &Regex, s: &str, repl: &str, global: bool, nth: usize) -> Option<String> {
    let mut out = String::new();
    let mut pos = 0;
    let mut count = 0;
    let mut changed = false;
    while pos <= s.len() {
        let Some(m) = re.find(s, pos) else { break };
        let (ms, me) = m[0]?;
        count += 1;
        if count >= nth {
            out.push_str(&s[pos..ms]);
            expand(repl, s, &m, &mut out);
            changed = true;
        } else {
            out.push_str(&s[pos..me]);
        }
        if me == ms {
            // empty match: copy one char and move on
            if let Some(c) = s[me..].chars().next() {
                out.push(c);
                pos = me + c.len_utf8();
            } else {
                pos = me + 1;
            }
        } else {
            pos = me;
        }
        if changed && !global {
            break;
        }
    }
    if !changed {
        return None;
    }
    if pos < s.len() {
        out.push_str(&s[pos..]);
    }
    Some(out)
}

fn expand(repl: &str, s: &str, m: &[Option<(usize, usize)>; 10], out: &mut String) {
    let mut it = repl.chars();
    while let Some(c) = it.next() {
        match c {
            '&' => {
                if let Some((a, b)) = m[0] {
                    out.push_str(&s[a..b]);
                }
            }
            '\\' => match it.next() {
                Some(d @ '0'..='9') => {
                    if let Some((a, b)) = m[d as usize - '0' as usize] {
                        out.push_str(&s[a..b]);
                    }
                }
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            },
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sed(script: &str, input: &str, quiet: bool, ext: bool) -> String {
        let prog = Program::parse(script, ext).unwrap();
        let lines: Vec<String> = input.lines().map(str::to_string).collect();
        let mut out = Vec::new();
        State::new(prog.cmds.len()).run(&prog, lines, quiet, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn anchored_regex_substitute() {
        assert_eq!(sed(r#"s/^build_version = .*/build_version = "X"/"#, "build_version = \"1\"\nname = a\n", false, false),
            "build_version = \"X\"\nname = a\n");
    }

    #[test]
    fn groups_ampersand_and_global() {
        assert_eq!(sed(r"s/\(a\)\(b\)/\2\1/g", "abab\n", false, false), "baba\n");
        assert_eq!(sed("s/[0-9]+/<&>/g", "a1b22\n", false, true), "a<1>b<22>\n");
        assert_eq!(sed("s/o/0/2", "foo\n", false, false), "fo0\n");
    }

    #[test]
    fn addresses_ranges_and_negation() {
        assert_eq!(sed("2d", "a\nb\nc\n", false, false), "a\nc\n");
        assert_eq!(sed("/b/,/c/d", "a\nb\nx\nc\nd\n", false, false), "a\nd\n");
        assert_eq!(sed("$!d", "a\nb\n", false, false), "b\n");
    }

    #[test]
    fn print_quiet_and_blocks() {
        assert_eq!(sed("/b/p", "a\nb\n", true, false), "b\n");
        assert_eq!(sed("/b/{s/b/B/;p}", "a\nb\n", true, false), "B\n");
        assert_eq!(sed("2q", "a\nb\nc\n", false, false), "a\nb\n");
    }

    #[test]
    fn text_commands_and_y() {
        assert_eq!(sed("1i top", "a\n", false, false), "top\na\n");
        assert_eq!(sed("$a end", "a\n", false, false), "a\nend\n");
        assert_eq!(sed("c new", "a\n", false, false), "new\n");
        assert_eq!(sed("y/abc/xyz/", "cab\n", false, false), "zxy\n");
    }

    #[test]
    fn custom_delimiter_and_escapes() {
        assert_eq!(sed("s#/usr#/overlayer/syshub#g", "/usr/bin:/usr/lib\n", false, false),
            "/overlayer/syshub/bin:/overlayer/syshub/lib\n");
        assert_eq!(sed(r"s/a\/b/c/", "a/b\n", false, false), "c\n");
    }

    #[test]
    fn rejects_bad_scripts() {
        assert!(Program::parse("s/foo", false).is_err());
        assert!(Program::parse("k", false).is_err());
        assert!(Program::parse("{p", false).is_err());
    }
}
