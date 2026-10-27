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
    let mut zero = false;
    let mut line_len = 70usize;
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
            "-z" | "--null-data" | "--zero-terminated" => zero = true,
            "-l" | "--line-length" => {
                i += 1;
                line_len = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(70);
            }
            s if s.starts_with("--line-length=") => line_len = s[14..].parse().unwrap_or(70),
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
            s if s.len() > 2 && s[1..].chars().all(|c| "nErsuz".contains(c)) => {
                for c in s[1..].chars() {
                    match c {
                        'n' => quiet = true,
                        'E' | 'r' => extended = true,
                        's' => separate = true,
                        'z' => zero = true,
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

    let term = if zero { b'\0' } else { b'\n' };
    let mut status = 0;
    if let Some(suffix) = in_place {
        for f in &files {
            if f == "-" {
                continue;
            }
            if let Err(e) = edit_in_place(f, &suffix, &program, quiet, term) {
                ui.err(&format!("{f}: {e}"));
                status = 1;
            }
        }
        return status;
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut state = State::new(&program, term, line_len);
    if separate {
        for f in &files {
            match read_lines(f, term) {
                Ok((lines, unterminated)) => {
                    state.line_no = 0;
                    state.file = f.clone();
                    state.unterminated = unterminated;
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
            match read_lines(f, term) {
                Ok((lines, unterminated)) => {
                    state.file = f.clone();
                    state.unterminated = unterminated;
                    all.extend(lines)
                }
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

const HELP: &str = "Usage: sed [OPTION]... {script-only-if-no-other-script} [input-file]...\n  -n, --quiet          suppress automatic printing\n  -e, --expression=S   add script S\n  -f, --file=F         add the script in file F\n  -i[SUF], --in-place  edit files in place (backup with SUF)\n  -E, -r               extended regular expressions\n  -s, --separate       treat files as separate\n  -z, --null-data      lines are separated by NUL characters\n  -l N                 line-wrap length for the l command\nCommands: s y d D p P n N g G h H x l q Q = a i c r R w W b t T : z F { }\nAddresses: N, $, /re/, \\cREc, first~step, addr1,addr2, addr1,+N, addr1,~N, 0,/re/, !\n";

/// The records of `path`, and whether the last one lacks its terminator (GNU sed then
/// writes the last line without one too).
fn read_lines(path: &str, term: u8) -> io::Result<(Vec<String>, bool)> {
    let mut data = Vec::new();
    if path == "-" {
        io::stdin().read_to_end(&mut data)?;
    } else {
        File::open(path)?.read_to_end(&mut data)?;
    }
    let unterminated = data.last().is_some_and(|c| *c != term);
    let text = String::from_utf8_lossy(&data);
    let mut lines: Vec<String> = text.split(term as char).map(str::to_string).collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    Ok((lines, unterminated))
}

fn edit_in_place(path: &str, suffix: &str, program: &Program, quiet: bool, term: u8) -> io::Result<()> {
    if let Some(reason) = protect::modification_denied(Path::new(path)) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason.message()));
    }
    let (lines, unterminated) = read_lines(path, term)?;
    let mut buf: Vec<u8> = Vec::new();
    let mut state = State::new(program, term, 70);
    state.unterminated = unterminated;
    state.file = path.to_string();
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
    /// first~step
    Step(usize, usize),
    /// 0 in `0,/re/`: the range is open before line 1
    Zero,
}

/// The end of a range.
enum Addr2 {
    At(Addr),
    /// addr1,+N
    Plus(usize),
    /// addr1,~N
    Multiple(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum Jump {
    Always,
    IfSubst,
    UnlessSubst,
}

enum Op {
    Subst { re: Regex, repl: String, global: bool, nth: usize, print: bool, write: Option<usize> },
    Translit(Vec<(char, char)>),
    Delete,
    DeleteFirst,
    Print,
    PrintFirst,
    Quit { print: bool, code: i32 },
    LineNo,
    Append(String),
    Insert(String),
    Change(String),
    Next,
    NextAppend,
    Hold,
    HoldAppend,
    Get,
    GetAppend,
    Exchange,
    List(Option<usize>),
    Branch(Jump, String),
    Label,
    ReadFile(String),
    ReadLine(String),
    Write(usize),
    WriteFirst(usize),
    Zap,
    FileName,
    Block(usize),
    EndBlock,
}

struct Cmd {
    a1: Option<Addr>,
    a2: Option<Addr2>,
    negate: bool,
    op: Op,
}

pub struct Program {
    cmds: Vec<Cmd>,
    /// label name → index of its `:` command
    labels: std::collections::HashMap<String, usize>,
    /// files named by `w`, `W` and the `s///w` flag, in order
    wfiles: Vec<String>,
}

impl Program {
    fn parse(src: &str, extended: bool) -> Result<Program, String> {
        let chars: Vec<char> = src.chars().collect();
        let mut p = Parser { s: chars, i: 0, extended, wfiles: Vec::new(), last_re: None };
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut opens: Vec<usize> = Vec::new();
        let mut labels = std::collections::HashMap::new();
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
                p.skip(|c| c == ' ' || c == '\t');
                a2 = Some(match p.peek() {
                    Some('+') | Some('~') => {
                        let plus = p.next() == Some('+');
                        let n: usize = p.take_while(|c| c.is_ascii_digit()).parse().map_err(|_| "expected a number")?;
                        if plus { Addr2::Plus(n) } else { Addr2::Multiple(n) }
                    }
                    _ => Addr2::At(p.addr()?.ok_or("unexpected `,'")?),
                });
            }
            if matches!(a1, Some(Addr::Zero)) && !matches!(a2, Some(Addr2::At(Addr::Re(_)))) {
                return Err("invalid usage of line address 0".into());
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
                'D' => Op::DeleteFirst,
                'p' => Op::Print,
                'P' => Op::PrintFirst,
                '=' => Op::LineNo,
                'n' => Op::Next,
                'N' => Op::NextAppend,
                'h' => Op::Hold,
                'H' => Op::HoldAppend,
                'g' => Op::Get,
                'G' => Op::GetAppend,
                'x' => Op::Exchange,
                'z' => Op::Zap,
                'F' => Op::FileName,
                'l' => {
                    p.skip(|c| c == ' ' || c == '\t');
                    let n = p.take_while(|c| c.is_ascii_digit());
                    Op::List(n.parse().ok())
                }
                ':' => {
                    if a1.is_some() {
                        return Err(": doesn't want any addresses".into());
                    }
                    p.skip(|c| c == ' ' || c == '\t');
                    let name = p.take_while(|c| c != '\n' && c != ';');
                    let name = name.trim_end().to_string();
                    if name.is_empty() {
                        return Err("\":\" lacks a label".into());
                    }
                    labels.insert(name, cmds.len());
                    Op::Label
                }
                'b' | 't' | 'T' => {
                    p.skip(|c| c == ' ' || c == '\t');
                    let name = p.take_while(|c| c != '\n' && c != ';' && c != '}');
                    let kind = match c {
                        'b' => Jump::Always,
                        't' => Jump::IfSubst,
                        _ => Jump::UnlessSubst,
                    };
                    Op::Branch(kind, name.trim_end().to_string())
                }
                'r' | 'R' | 'w' | 'W' => {
                    p.skip(|c| c == ' ' || c == '\t');
                    let name = p.take_while(|c| c != '\n');
                    match c {
                        'r' => Op::ReadFile(name),
                        'R' => Op::ReadLine(name),
                        'w' => Op::Write(p.wfile(name)),
                        _ => Op::WriteFirst(p.wfile(name)),
                    }
                }
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
        for c in &cmds {
            if let Op::Branch(_, l) = &c.op {
                if !l.is_empty() && !labels.contains_key(l) {
                    return Err(format!("can't find label for jump to `{l}'"));
                }
            }
        }
        Ok(Program { cmds, labels, wfiles: p.wfiles })
    }
}

struct Parser {
    s: Vec<char>,
    i: usize,
    extended: bool,
    wfiles: Vec<String>,
    /// an empty regex (`s//X/`, `//`) means the last one given
    last_re: Option<String>,
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

    /// `pat`, or the last regex when it is empty.
    fn regex(&mut self, pat: String) -> Result<String, String> {
        if pat.is_empty() {
            return self.last_re.clone().ok_or_else(|| "no previous regular expression".to_string());
        }
        self.last_re = Some(pat.clone());
        Ok(pat)
    }

    /// The index of `name` among the files written to (one handle per name).
    fn wfile(&mut self, name: String) -> usize {
        match self.wfiles.iter().position(|w| *w == name) {
            Some(i) => i,
            None => {
                self.wfiles.push(name);
                self.wfiles.len() - 1
            }
        }
    }

    fn addr(&mut self) -> Result<Option<Addr>, String> {
        match self.peek() {
            Some(c) if c.is_ascii_digit() => {
                let n: usize = self.take_while(|c| c.is_ascii_digit()).parse().map_err(|_| "bad line number")?;
                if self.peek() == Some('~') {
                    self.i += 1;
                    let step: usize = self.take_while(|c| c.is_ascii_digit()).parse().unwrap_or(0);
                    return Ok(Some(Addr::Step(n, step)));
                }
                Ok(Some(if n == 0 { Addr::Zero } else { Addr::Line(n) }))
            }
            Some('$') => {
                self.i += 1;
                Ok(Some(Addr::Last))
            }
            Some('/') | Some('\\') => {
                let delim = if self.next() == Some('\\') { self.next().ok_or("bad address")? } else { '/' };
                let pat = self.delimited(delim, false)?;
                let pat = self.regex(pat)?;
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
        let pat = self.regex(pat)?;
        let repl = self.delimited(delim, true)?;
        let (mut global, mut nth, mut print, mut icase) = (false, 1usize, false, false);
        let mut write = None;
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
                Some('w') => {
                    self.i += 1;
                    self.skip(|c| c == ' ' || c == '\t');
                    let name = self.take_while(|c| c != '\n');
                    write = Some(self.wfile(name));
                    break;
                }
                _ => break,
            }
            self.i += 1;
        }
        Ok(Op::Subst { re: Regex::new(&pat, self.extended, icase)?, repl, global, nth: nth.max(1), print, write })
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
        // `a\` is followed by the text, on the next line or (GNU) the same one
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
                    Some('a') => out.push('\u{7}'),
                    Some('f') => out.push('\u{c}'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('v') => out.push('\u{b}'),
                    // \cX: control-X
                    Some('c') => match self.next() {
                        Some(x) => out.push(((x.to_ascii_uppercase() as u8) ^ 0x40) as char),
                        None => break,
                    },
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
    /// for addr1,+N / addr1,~N: the line the range ends on
    range_end: Vec<usize>,
    hold: String,
    term: u8,
    line_len: usize,
    file: String,
    wfiles: Vec<Option<std::fs::File>>,
    rfiles: std::collections::HashMap<String, std::io::Lines<io::BufReader<File>>>,
    /// the input's last line had no terminator
    unterminated: bool,
}

impl State {
    fn new(prog: &Program, term: u8, line_len: usize) -> Self {
        let n = prog.cmds.len();
        // every w file is created (emptied) before the first line, as GNU sed does
        let wfiles = prog
            .wfiles
            .iter()
            .map(|w| if w == "/dev/stdout" || w == "/dev/stderr" { None } else { File::create(w).ok() })
            .collect();
        State {
            line_no: 0,
            in_range: vec![false; n],
            range_end: vec![0; n],
            hold: String::new(),
            term,
            line_len,
            file: "-".into(),
            wfiles,
            rfiles: std::collections::HashMap::new(),
            unterminated: false,
        }
    }

    fn matches(&mut self, idx: usize, cmd: &Cmd, line: &str, last: bool) -> bool {
        let n = self.line_no;
        let hit = |a: &Addr| match a {
            Addr::Line(l) => *l == n,
            Addr::Last => last,
            Addr::Re(r) => r.is_match(line),
            Addr::Step(first, step) => if *step == 0 { n == *first } else { n >= *first && (n - first) % step == 0 },
            Addr::Zero => false,
        };
        let m = match (&cmd.a1, &cmd.a2) {
            (None, _) => true,
            (Some(a), None) => hit(a),
            (Some(a), Some(b)) => {
                if self.in_range[idx] {
                    let end = match b {
                        Addr2::At(Addr::Line(l)) => n >= *l,
                        Addr2::At(other) => hit(other),
                        Addr2::Plus(_) | Addr2::Multiple(_) => n >= self.range_end[idx],
                    };
                    if end {
                        self.in_range[idx] = false;
                    }
                    true
                } else if hit(a) || (matches!(a, Addr::Zero) && n == 1 && !self.in_range[idx] && self.range_end[idx] == 0) {
                    // 0,/re/: the range is already open on line 1, so /re/ can end it there
                    let zero = matches!(a, Addr::Zero);
                    if zero {
                        self.range_end[idx] = usize::MAX;
                    }
                    let ends_now = match b {
                        Addr2::At(Addr::Line(l)) => *l <= n,
                        Addr2::At(other) if zero => hit(other),
                        Addr2::Plus(k) => {
                            self.range_end[idx] = n + k;
                            *k == 0
                        }
                        Addr2::Multiple(k) => {
                            let end = if *k == 0 { n } else { n.div_ceil(*k) * k };
                            self.range_end[idx] = end;
                            end <= n
                        }
                        Addr2::At(_) => false,
                    };
                    self.in_range[idx] = !ends_now;
                    true
                } else {
                    false
                }
            }
        };
        m != cmd.negate
    }

    fn put(&self, out: &mut dyn Write, s: &str) -> io::Result<()> {
        out.write_all(s.as_bytes())?;
        out.write_all(&[self.term])
    }

    fn write_to(&mut self, k: usize, name: &str, s: &str, out: &mut dyn Write) -> io::Result<()> {
        let term = self.term;
        match name {
            "/dev/stdout" => self.put(out, s),
            "/dev/stderr" => {
                let mut e = io::stderr();
                e.write_all(s.as_bytes())?;
                e.write_all(&[term])
            }
            _ => match &mut self.wfiles[k] {
                Some(f) => {
                    f.write_all(s.as_bytes())?;
                    f.write_all(&[term])
                }
                None => Ok(()),
            },
        }
    }

    /// `l`: the pattern space with escapes, wrapped, ending in `$`.
    fn list(&self, s: &str, width: Option<usize>, out: &mut dyn Write) -> io::Result<()> {
        let width = width.unwrap_or(self.line_len);
        let mut line = String::new();
        let mut col = 0;
        for b in s.bytes() {
            let piece = match b {
                b'\\' => "\\\\".to_string(),
                7 => "\\a".into(),
                8 => "\\b".into(),
                12 => "\\f".into(),
                b'\n' => "\\n".into(),
                b'\r' => "\\r".into(),
                b'\t' => "\\t".into(),
                11 => "\\v".into(),
                c if (0x20..0x7f).contains(&c) => (c as char).to_string(),
                c => format!("\\{c:03o}"),
            };
            if width > 1 && col + piece.len() > width - 1 {
                line.push_str("\\\n");
                col = 0;
            }
            col += piece.len();
            line.push_str(&piece);
        }
        line.push('$');
        writeln!(out, "{line}")
    }

    fn run(&mut self, prog: &Program, lines: Vec<String>, quiet: bool, out: &mut dyn Write) -> io::Result<Flow> {
        let mut buf: Vec<u8> = Vec::new();
        let mut ended = false;
        let flow = self.cycles(prog, lines, quiet, &mut buf, &mut ended);
        // input without a final newline: neither has the output
        if ended && self.unterminated && buf.last() == Some(&self.term) {
            buf.pop();
        }
        out.write_all(&buf)?;
        flow
    }

    fn cycles(&mut self, prog: &Program, lines: Vec<String>, quiet: bool, out: &mut dyn Write, ended: &mut bool) -> io::Result<Flow> {
        let mut it = lines.into_iter().peekable();
        let mut space = String::new();
        // D restarts the cycle with what is left instead of reading a line
        let mut reuse = false;
        loop {
            if !reuse {
                match it.next() {
                    Some(l) => space = l,
                    None => return Ok(Flow::Continue),
                }
                self.line_no += 1;
            }
            reuse = false;
            let mut last = it.peek().is_none();
            *ended = last;
            let mut append: Vec<String> = Vec::new();
            let mut substituted = false;
            let mut autoprint = !quiet;
            let mut pc = 0;
            while pc < prog.cmds.len() {
                let cmd = &prog.cmds[pc];
                if matches!(cmd.op, Op::EndBlock | Op::Label) {
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
                    Op::Block(_) | Op::EndBlock | Op::Label => {}
                    Op::Subst { re, repl, global, nth, print, write } => {
                        if let Some(new) = substitute(re, &space, repl, *global, *nth) {
                            space = new;
                            substituted = true;
                            if *print {
                                self.put(out, &space)?;
                            }
                            if let Some(k) = write {
                                let name = prog.wfiles[*k].clone();
                                self.write_to(*k, &name, &space.clone(), out)?;
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
                        autoprint = false;
                        break;
                    }
                    Op::DeleteFirst => {
                        autoprint = false;
                        if let Some(i) = space.find('\n') {
                            space = space[i + 1..].to_string();
                            reuse = true;
                        }
                        break;
                    }
                    Op::Print => self.put(out, &space)?,
                    Op::PrintFirst => {
                        let first = space.split('\n').next().unwrap_or("").to_string();
                        self.put(out, &first)?;
                    }
                    Op::LineNo => writeln!(out, "{}", self.line_no)?,
                    Op::Append(t) => append.push(t.clone()),
                    Op::Insert(t) => writeln!(out, "{t}")?,
                    Op::Change(t) => {
                        let range_open = cmd.a2.is_some() && self.in_range[pc];
                        if !range_open || cmd.negate {
                            writeln!(out, "{t}")?;
                        }
                        autoprint = false;
                        break;
                    }
                    Op::Next => {
                        if it.peek().is_none() {
                            // no next line: GNU sed ends here, printing as usual
                            break;
                        }
                        if !quiet {
                            self.put(out, &space)?;
                        }
                        for a in append.drain(..) {
                            writeln!(out, "{a}")?;
                        }
                        space = it.next().unwrap_or_default();
                        self.line_no += 1;
                        last = it.peek().is_none();
                        *ended = last;
                    }
                    Op::NextAppend => match it.next() {
                        Some(l) => {
                            space.push('\n');
                            space.push_str(&l);
                            self.line_no += 1;
                            last = it.peek().is_none();
                            *ended = last;
                        }
                        None => break,
                    },
                    Op::Hold => self.hold = space.clone(),
                    Op::HoldAppend => {
                        self.hold.push('\n');
                        self.hold.push_str(&space);
                    }
                    Op::Get => space = self.hold.clone(),
                    Op::GetAppend => {
                        space.push('\n');
                        space.push_str(&self.hold);
                    }
                    Op::Exchange => std::mem::swap(&mut space, &mut self.hold),
                    Op::Zap => space.clear(),
                    Op::FileName => writeln!(out, "{}", self.file)?,
                    Op::List(w) => self.list(&space, *w, out)?,
                    Op::ReadFile(f) => {
                        if let Ok(t) = std::fs::read_to_string(f) {
                            append.push(t.strip_suffix('\n').unwrap_or(&t).to_string());
                        }
                    }
                    Op::ReadLine(f) => {
                        if !self.rfiles.contains_key(f) {
                            if let Ok(h) = File::open(f) {
                                use std::io::BufRead;
                                self.rfiles.insert(f.clone(), io::BufReader::new(h).lines());
                            }
                        }
                        if let Some(Ok(l)) = self.rfiles.get_mut(f).and_then(|r| r.next()) {
                            append.push(l);
                        }
                    }
                    Op::Write(k) => {
                        let name = prog.wfiles[*k].clone();
                        let text = space.clone();
                        self.write_to(*k, &name, &text, out)?;
                    }
                    Op::WriteFirst(k) => {
                        let name = prog.wfiles[*k].clone();
                        let first = space.split('\n').next().unwrap_or("").to_string();
                        self.write_to(*k, &name, &first, out)?;
                    }
                    Op::Branch(kind, label) => {
                        let jump = match kind {
                            Jump::Always => true,
                            Jump::IfSubst => substituted,
                            Jump::UnlessSubst => !substituted,
                        };
                        if *kind != Jump::Always {
                            substituted = false;
                        }
                        if jump {
                            pc = if label.is_empty() { prog.cmds.len() } else { prog.labels[label] };
                            continue;
                        }
                    }
                    Op::Quit { print, code } => {
                        if *print && !quiet {
                            self.put(out, &space)?;
                        }
                        for a in append.drain(..) {
                            writeln!(out, "{a}")?;
                        }
                        return Ok(Flow::Quit(*code));
                    }
                }
                pc += 1;
            }
            if autoprint {
                self.put(out, &space)?;
            }
            for a in append {
                writeln!(out, "{a}")?;
            }
        }
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

/// The replacement text, with `&`, `\1`..`\9`, `\n`, `\t` and GNU's case conversion
/// (`\U` `\L` up to `\E`, `\u` `\l` for the next character).
fn expand(repl: &str, s: &str, m: &[Option<(usize, usize)>; 10], out: &mut String) {
    #[derive(Clone, Copy, PartialEq)]
    enum Case {
        Keep,
        Upper,
        Lower,
    }
    let mut mode = Case::Keep;
    let mut once = Case::Keep;
    let emit = |text: &str, out: &mut String, mode: Case, once: &mut Case| {
        for c in text.chars() {
            let c = match (*once, mode) {
                (Case::Upper, _) => c.to_uppercase().next().unwrap_or(c),
                (Case::Lower, _) => c.to_lowercase().next().unwrap_or(c),
                (_, Case::Upper) => c.to_uppercase().next().unwrap_or(c),
                (_, Case::Lower) => c.to_lowercase().next().unwrap_or(c),
                _ => c,
            };
            *once = Case::Keep;
            out.push(c);
        }
    };
    let mut it = repl.chars();
    while let Some(c) = it.next() {
        match c {
            '&' => {
                if let Some((a, b)) = m[0] {
                    emit(&s[a..b], out, mode, &mut once);
                }
            }
            '\\' => match it.next() {
                Some(d @ '0'..='9') => {
                    if let Some((a, b)) = m[d as usize - '0' as usize] {
                        emit(&s[a..b], out, mode, &mut once);
                    }
                }
                Some('n') => emit("\n", out, mode, &mut once),
                Some('t') => emit("\t", out, mode, &mut once),
                Some('U') => mode = Case::Upper,
                Some('L') => mode = Case::Lower,
                Some('E') => mode = Case::Keep,
                Some('u') => once = Case::Upper,
                Some('l') => once = Case::Lower,
                Some(o) => emit(&o.to_string(), out, mode, &mut once),
                None => out.push('\\'),
            },
            c => emit(&c.to_string(), out, mode, &mut once),
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
        State::new(&prog, b'\n', 70).run(&prog, lines, quiet, &mut out).unwrap();
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
    fn hold_space_next_and_branches() {
        assert_eq!(sed("1!G;h;$!d", "a\nb\nc\n", false, false), "c\nb\na\n");
        assert_eq!(sed("N;s/\\n/+/", "a\nb\nc\n", false, false), "a+b\nc\n");
        assert_eq!(sed(":a;N;$!ba;s/\\n/,/g", "a\nb\nc\n", false, false), "a,b,c\n");
        assert_eq!(sed("$!N;P;D", "a\nb\nc\n", false, false), "a\nb\nc\n");
        assert_eq!(sed("s/x/y/;t;s/$/!/", "x\nz\n", false, false), "y\nz!\n");
    }

    #[test]
    fn step_addresses_case_and_list() {
        assert_eq!(sed("2~2d", "1\n2\n3\n4\n5\n", false, false), "1\n3\n5\n");
        assert_eq!(sed("2,+1d", "1\n2\n3\n4\n", false, false), "1\n4\n");
        assert_eq!(sed("0,/a/s//X/", "a\na\n", false, false), "X\na\n");
        assert_eq!(sed(r"s/\(.\)\(.*\)/\u\1\U\2\E!/", "abc\n", false, false), "ABC!\n");
        assert_eq!(sed("l", "a\tb\n", true, false), "a\\tb$\n");
    }

    #[test]
    fn rejects_bad_scripts() {
        assert!(Program::parse("s/foo", false).is_err());
        assert!(Program::parse("k", false).is_err());
        assert!(Program::parse("{p", false).is_err());
    }
}
