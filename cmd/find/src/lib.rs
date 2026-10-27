//! user find — search for files in a directory hierarchy.
//!
//! The expression is parsed the way GNU find does it: `-o` binds loosest, then `-a`
//! (also implied between two terms), then `!`/`-not`, then `( )`. Without an action
//! (`-print`, `-print0`, `-printf`, `-ls`, `-exec`, `-execdir`, `-delete`, `-quit`) the
//! whole expression is followed by `-print`.
use std::ffi::CString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Test(Test),
    Action(Action),
}

#[derive(Clone, Debug)]
enum Test {
    True,
    False,
    Name { pat: String, icase: bool },
    Path { pat: String, icase: bool },
    LinkName { pat: String, icase: bool },
    Regex { re: String, icase: bool, extended: bool },
    Type { types: String, follow: bool },
    Empty,
    Executable,
    Readable,
    Writable,
    Size { op: char, n: u64, unit: u64 },
    Time { field: char, op: char, n: i64, minutes: bool },
    Newer { field: char, secs: i64, nsecs: i64 },
    Perm { kind: char, mode: u32 },
    Uid(char, u64),
    Gid(char, u64),
    User(u32),
    Group(u32),
    NoUser,
    NoGroup,
    Links(char, u64),
    Inum(char, u64),
    SameFile(u64, u64),
}

#[derive(Clone, Debug)]
enum Action {
    Print,
    Print0,
    Printf(String),
    Ls,
    Exec { cmd: Vec<String>, batch: Option<usize>, in_dir: bool },
    Delete,
    Prune,
    Quit,
}

struct Opts {
    max_depth: usize,
    min_depth: usize,
    depth_first: bool,
    follow: bool,
    xdev: bool,
}

/// What one file is checked against.
struct Entry<'a> {
    path: &'a Path,
    start: &'a Path,
    depth: usize,
    meta: Option<fs::Metadata>,
    follow: bool,
}

impl Entry<'_> {
    fn lmeta(&self) -> Option<fs::Metadata> {
        fs::symlink_metadata(self.path).ok()
    }
}

struct State {
    out: io::BufWriter<io::StdoutLock<'static>>,
    status: i32,
    pruned: bool,
    quit: bool,
    /// Paths collected for each `-exec ... {} +` (by its position in the expression).
    batches: Vec<(Vec<String>, bool, Vec<PathBuf>)>,
    now: i64,
}

fn usage_error(msg: &str) -> i32 {
    eprintln!("find: {msg}");
    1
}

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--help") {
        print!(
            "Usage: find [-H] [-L] [-P] [PATH]... [EXPRESSION]\n\
 Search for files in a directory hierarchy.\n\n\
 Operators: ( EXPR )  ! EXPR  -not EXPR  EXPR -a EXPR  EXPR -o EXPR  EXPR , EXPR\n\
 Options:   -maxdepth N -mindepth N -depth -xdev -mount -follow -noleaf -daystart\n\
 Tests:     -name -iname -path -ipath -wholename -lname -regex -iregex -type -xtype\n\
            -empty -executable -readable -writable -size -mtime -atime -ctime -mmin\n\
            -amin -cmin -newer -anewer -cnewer -perm -user -group -uid -gid -nouser\n\
            -nogroup -links -inum -samefile -true -false\n\
 Actions:   -print -print0 -printf FMT -ls -exec CMD ; -exec CMD {{}} + -execdir -delete\n\
            -prune -quit\n"
        );
        return 0;
    }
    if args.first().is_some_and(|a| a == "--version") {
        println!("find (user_utils) 0.2.0");
        return 0;
    }

    let mut opts = Opts { max_depth: usize::MAX, min_depth: 0, depth_first: false, follow: false, xdev: false };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-L" => opts.follow = true,
            "-P" | "-H" => opts.follow = false,
            "-D" | "-O" if i + 1 < args.len() => i += 1,
            _ => break,
        }
        i += 1;
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    while i < args.len() && !is_expr_start(&args[i]) {
        paths.push(PathBuf::from(&args[i]));
        i += 1;
    }
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }

    let mut p = Parser { args: &args[i..], pos: 0, opts: &mut opts, has_action: false, batches: 0 };
    let expr = if p.args.is_empty() {
        None
    } else {
        match p.parse_or() {
            Ok(e) => Some(e),
            Err(e) => return usage_error(&e),
        }
    };
    if p.pos < p.args.len() {
        return usage_error(&format!("unexpected extra predicate '{}'", p.args[p.pos]));
    }
    let batches = p.batches;
    let expr = match (expr, p.has_action) {
        (Some(e), true) => e,
        (Some(e), false) => Expr::And(Box::new(e), Box::new(Expr::Action(Action::Print))),
        (None, _) => Expr::Action(Action::Print),
    };
    let expr = number_batches(expr, &mut 0);

    let stdout: &'static io::Stdout = Box::leak(Box::new(io::stdout()));
    let mut st = State {
        out: io::BufWriter::new(stdout.lock()),
        status: 0,
        pruned: false,
        quit: false,
        batches: vec![(Vec::new(), false, Vec::new()); batches],
        now: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
    };
    collect_batches(&expr, &mut st);

    for p in &paths {
        if st.quit {
            break;
        }
        let meta = if opts.follow { fs::metadata(p).or_else(|_| fs::symlink_metadata(p)) } else { fs::symlink_metadata(p) };
        match meta {
            Ok(m) => {
                let dev = m.dev();
                walk(p, p, 0, m, dev, &opts, &expr, &mut st);
            }
            Err(e) => {
                eprintln!("find: '{}': {}", p.display(), io_reason(&e));
                st.status = 1;
            }
        }
    }
    flush_batches(&mut st);
    let _ = st.out.flush();
    st.status
}

fn is_expr_start(a: &str) -> bool {
    (a.starts_with('-') && a.len() > 1) || a == "!" || a == "(" || a == ")" || a == ","
}

fn io_reason(e: &io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

struct Parser<'a> {
    args: &'a [String],
    pos: usize,
    opts: &'a mut Opts,
    has_action: bool,
    batches: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&str> {
        self.args.get(self.pos).map(|s| s.as_str())
    }

    fn arg(&mut self, opt: &str) -> Result<String, String> {
        self.pos += 1;
        let a = self.args.get(self.pos).cloned().ok_or(format!("missing argument to `{opt}'"))?;
        self.pos += 1;
        Ok(a)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while let Some(op) = self.peek() {
            if op == "-o" || op == "-or" {
                self.pos += 1;
                let right = self.parse_and()?;
                left = Expr::Or(Box::new(left), Box::new(right));
            } else if op == "," {
                self.pos += 1;
                let right = self.parse_and()?;
                // both evaluated, the value is the right one's
                left = Expr::And(Box::new(Expr::Or(Box::new(left), Box::new(Expr::Test(Test::True)))), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_not()?;
        while let Some(op) = self.peek() {
            if op == "-o" || op == "-or" || op == ")" || op == "," {
                break;
            }
            if op == "-a" || op == "-and" {
                self.pos += 1;
            }
            let right = self.parse_not()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some("!") | Some("-not") => {
                self.pos += 1;
                Ok(Expr::Not(Box::new(self.parse_not()?)))
            }
            Some("(") => {
                self.pos += 1;
                let e = self.parse_or()?;
                if self.peek() != Some(")") {
                    return Err("invalid expression; I was expecting to find a ')' somewhere".into());
                }
                self.pos += 1;
                Ok(e)
            }
            Some(_) => self.primary(),
            None => Err("invalid expression; expected an expression".into()),
        }
    }

    fn num_arg(&mut self, opt: &str) -> Result<(char, i64), String> {
        let a = self.arg(opt)?;
        parse_n(&a).map_err(|_| format!("invalid argument `{a}' to `{opt}'"))
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let tok = self.peek().unwrap_or_default().to_string();
        let test = |t| Ok(Expr::Test(t));
        let one = |p: &mut Self| {
            p.pos += 1;
        };
        match tok.as_str() {
            // options: true wherever they stand
            "-maxdepth" | "-mindepth" => {
                let a = self.arg(&tok)?;
                let n = parse_depth(&a).map_err(|_| format!("invalid argument `{a}' to `{tok}'"))?;
                if tok == "-maxdepth" {
                    self.opts.max_depth = n;
                } else {
                    self.opts.min_depth = n;
                }
                test(Test::True)
            }
            "-depth" | "-d" => {
                one(self);
                self.opts.depth_first = true;
                test(Test::True)
            }
            "-xdev" | "-mount" => {
                one(self);
                self.opts.xdev = true;
                test(Test::True)
            }
            "-follow" => {
                one(self);
                self.opts.follow = true;
                test(Test::True)
            }
            "-noleaf" | "-daystart" | "-ignore_readdir_race" | "-nowarn" | "-warn" => {
                one(self);
                test(Test::True)
            }
            "-regextype" => {
                self.arg(&tok)?;
                test(Test::True)
            }
            "-true" => {
                one(self);
                test(Test::True)
            }
            "-false" => {
                one(self);
                test(Test::False)
            }
            "-name" | "-iname" => {
                let pat = self.arg(&tok)?;
                test(Test::Name { pat, icase: tok == "-iname" })
            }
            "-path" | "-wholename" | "-ipath" | "-iwholename" => {
                let pat = self.arg(&tok)?;
                test(Test::Path { pat, icase: tok.starts_with("-i") })
            }
            "-lname" | "-ilname" => {
                let pat = self.arg(&tok)?;
                test(Test::LinkName { pat, icase: tok == "-ilname" })
            }
            "-regex" | "-iregex" => {
                let re = self.arg(&tok)?;
                test(Test::Regex { re, icase: tok == "-iregex", extended: false })
            }
            "-type" | "-xtype" => {
                let types = self.arg(&tok)?.replace(',', "");
                if types.is_empty() || !types.chars().all(|c| "fdlpscbD".contains(c)) {
                    return Err(format!("Unknown argument to {tok}: {types}"));
                }
                test(Test::Type { types, follow: tok == "-xtype" })
            }
            "-empty" => {
                one(self);
                test(Test::Empty)
            }
            "-executable" => {
                one(self);
                test(Test::Executable)
            }
            "-readable" => {
                one(self);
                test(Test::Readable)
            }
            "-writable" => {
                one(self);
                test(Test::Writable)
            }
            "-size" => {
                let a = self.arg(&tok)?;
                let (op, n, unit) = parse_size(&a).map_err(|_| format!("invalid -size argument `{a}'"))?;
                test(Test::Size { op, n, unit })
            }
            "-mtime" | "-atime" | "-ctime" | "-mmin" | "-amin" | "-cmin" => {
                let (op, n) = self.num_arg(&tok)?;
                test(Test::Time { field: tok.as_bytes()[1] as char, op, n, minutes: tok.ends_with("min") })
            }
            "-newer" | "-anewer" | "-cnewer" => {
                let f = self.arg(&tok)?;
                let m = fs::metadata(&f).map_err(|e| format!("'{f}': {}", io_reason(&e)))?;
                let field = if tok == "-newer" { 'm' } else { tok.as_bytes()[1] as char };
                // compared against the reference file's modification time
                test(Test::Newer { field, secs: m.mtime(), nsecs: m.mtime_nsec() })
            }
            "-perm" => {
                let a = self.arg(&tok)?;
                let (kind, spec) = match a.chars().next() {
                    Some(c @ ('-' | '/' | '+')) => (if c == '+' { '/' } else { c }, &a[1..]),
                    _ => ('=', a.as_str()),
                };
                let mode = parse_mode(spec).ok_or(format!("invalid mode `{a}'"))?;
                test(Test::Perm { kind, mode })
            }
            "-uid" | "-gid" => {
                let (op, n) = self.num_arg(&tok)?;
                let n = n as u64;
                test(if tok == "-uid" { Test::Uid(op, n) } else { Test::Gid(op, n) })
            }
            "-user" => {
                let a = self.arg(&tok)?;
                let uid = usercore::nss::user_by_name(&a)
                    .map(|u| u.uid)
                    .or_else(|| libc_id(&a, true))
                    .or_else(|| a.parse().ok())
                    .ok_or(format!("'{a}' is not the name of a known user"))?;
                test(Test::User(uid))
            }
            "-group" => {
                let a = self.arg(&tok)?;
                let gid = usercore::nss::group_by_name(&a)
                    .map(|g| g.gid)
                    .or_else(|| libc_id(&a, false))
                    .or_else(|| a.parse().ok())
                    .ok_or(format!("'{a}' is not the name of an existing group"))?;
                test(Test::Group(gid))
            }
            "-nouser" => {
                one(self);
                test(Test::NoUser)
            }
            "-nogroup" => {
                one(self);
                test(Test::NoGroup)
            }
            "-links" | "-inum" => {
                let (op, n) = self.num_arg(&tok)?;
                test(if tok == "-links" { Test::Links(op, n as u64) } else { Test::Inum(op, n as u64) })
            }
            "-samefile" => {
                let f = self.arg(&tok)?;
                let m = fs::metadata(&f).map_err(|e| format!("'{f}': {}", io_reason(&e)))?;
                test(Test::SameFile(m.dev(), m.ino()))
            }
            // actions
            "-print" | "-print0" | "-ls" | "-delete" | "-prune" | "-quit" => {
                one(self);
                if tok != "-prune" {
                    self.has_action = true;
                }
                if tok == "-delete" {
                    self.opts.depth_first = true;
                }
                Ok(Expr::Action(match tok.as_str() {
                    "-print" => Action::Print,
                    "-print0" => Action::Print0,
                    "-ls" => Action::Ls,
                    "-delete" => Action::Delete,
                    "-prune" => Action::Prune,
                    _ => Action::Quit,
                }))
            }
            "-printf" => {
                let f = self.arg(&tok)?;
                self.has_action = true;
                Ok(Expr::Action(Action::Printf(f)))
            }
            "-exec" | "-execdir" | "-ok" | "-okdir" => {
                self.pos += 1;
                let mut cmd = Vec::new();
                let mut batch = false;
                loop {
                    let Some(a) = self.args.get(self.pos) else {
                        return Err(format!("missing argument to `{tok}'"));
                    };
                    self.pos += 1;
                    if a == ";" {
                        break;
                    }
                    if a == "+" && cmd.last().is_some_and(|l: &String| l == "{}") {
                        batch = true;
                        break;
                    }
                    cmd.push(a.clone());
                }
                if cmd.is_empty() {
                    return Err(format!("missing argument to `{tok}'"));
                }
                self.has_action = true;
                if batch {
                    cmd.pop();
                    self.batches += 1;
                }
                Ok(Expr::Action(Action::Exec {
                    cmd,
                    batch: batch.then_some(usize::MAX),
                    in_dir: tok.ends_with("dir"),
                }))
            }
            other => Err(format!("unknown predicate `{other}'")),
        }
    }
}

/// A user's (or group's) id through the C library, for names not in the passwd file.
fn libc_id(name: &str, user: bool) -> Option<u32> {
    let c = CString::new(name).ok()?;
    // SAFETY: c is NUL-terminated; the returned entry is read at once.
    unsafe {
        if user {
            let p = libc::getpwnam(c.as_ptr());
            (!p.is_null()).then(|| (*p).pw_uid)
        } else {
            let g = libc::getgrnam(c.as_ptr());
            (!g.is_null()).then(|| (*g).gr_gid)
        }
    }
}

/// Give each `-exec ... +` its slot in `State::batches`.
fn number_batches(e: Expr, n: &mut usize) -> Expr {
    match e {
        Expr::And(a, b) => {
            let a = number_batches(*a, n);
            Expr::And(Box::new(a), Box::new(number_batches(*b, n)))
        }
        Expr::Or(a, b) => {
            let a = number_batches(*a, n);
            Expr::Or(Box::new(a), Box::new(number_batches(*b, n)))
        }
        Expr::Not(a) => Expr::Not(Box::new(number_batches(*a, n))),
        Expr::Action(Action::Exec { cmd, batch: Some(_), in_dir }) => {
            *n += 1;
            Expr::Action(Action::Exec { cmd, batch: Some(*n - 1), in_dir })
        }
        other => other,
    }
}

fn collect_batches(e: &Expr, st: &mut State) {
    match e {
        Expr::And(a, b) | Expr::Or(a, b) => {
            collect_batches(a, st);
            collect_batches(b, st);
        }
        Expr::Not(a) => collect_batches(a, st),
        Expr::Action(Action::Exec { cmd, batch: Some(k), in_dir }) => {
            st.batches[*k] = (cmd.clone(), *in_dir, Vec::new());
        }
        _ => {}
    }
}

fn flush_batches(st: &mut State) {
    let _ = st.out.flush();
    for k in 0..st.batches.len() {
        let (cmd, _, paths) = &mut st.batches[k];
        if paths.is_empty() {
            continue;
        }
        let paths = std::mem::take(paths);
        let cmd = cmd.clone();
        if !run_command(&cmd, &paths, None) {
            st.status = 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(path: &Path, start: &Path, depth: usize, meta: fs::Metadata, dev: u64, o: &Opts, expr: &Expr, st: &mut State) {
    if st.quit {
        return;
    }
    let is_dir = meta.is_dir();
    let entry = Entry { path, start, depth, meta: Some(meta), follow: o.follow };
    st.pruned = false;
    if !o.depth_first && depth >= o.min_depth {
        eval(expr, &entry, st);
    }
    let pruned = std::mem::take(&mut st.pruned);
    if is_dir && depth < o.max_depth && !pruned && !st.quit {
        match fs::read_dir(path) {
            Ok(rd) => {
                // directory order, as GNU find
                let names: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                for child in names {
                    let m = if o.follow {
                        fs::metadata(&child).or_else(|_| fs::symlink_metadata(&child))
                    } else {
                        fs::symlink_metadata(&child)
                    };
                    let Ok(m) = m else { continue };
                    if o.xdev && m.dev() != dev {
                        if depth + 1 >= o.min_depth && !o.depth_first {
                            let e = Entry { path: &child, start, depth: depth + 1, meta: Some(m), follow: o.follow };
                            eval(expr, &e, st);
                        }
                        continue;
                    }
                    walk(&child, start, depth + 1, m, dev, o, expr, st);
                }
            }
            Err(e) => {
                let _ = st.out.flush();
                eprintln!("find: '{}': {}", path.display(), io_reason(&e));
                st.status = 1;
            }
        }
    }
    if o.depth_first && depth >= o.min_depth && !st.quit {
        let entry = Entry { path, start, depth, meta: fs::symlink_metadata(path).ok(), follow: o.follow };
        eval(expr, &entry, st);
    }
}

fn eval(e: &Expr, f: &Entry, st: &mut State) -> bool {
    match e {
        // -quit ends the evaluation where it stands
        Expr::And(a, b) => eval(a, f, st) && !st.quit && eval(b, f, st),
        Expr::Or(a, b) => {
            let left = eval(a, f, st);
            if left || st.quit { left } else { eval(b, f, st) }
        }
        Expr::Not(a) => !eval(a, f, st),
        Expr::Test(t) => test(t, f, st),
        Expr::Action(a) => act(a, f, st),
    }
}

/// `pat` against `s` as fnmatch(3) does it (no special `/` or leading `.`).
fn fnmatch(pat: &str, s: &[u8], icase: bool) -> bool {
    let (Ok(p), Ok(s)) = (CString::new(pat), CString::new(s)) else { return false };
    const FNM_CASEFOLD: libc::c_int = 1 << 4;
    // SAFETY: both strings are NUL-terminated.
    unsafe { libc::fnmatch(p.as_ptr(), s.as_ptr(), if icase { FNM_CASEFOLD } else { 0 }) == 0 }
}

fn cmp(op: char, val: i64, n: i64) -> bool {
    match op {
        '+' => val > n,
        '-' => val < n,
        _ => val == n,
    }
}

fn type_char(ft: fs::FileType) -> char {
    if ft.is_symlink() {
        'l'
    } else if ft.is_dir() {
        'd'
    } else if ft.is_file() {
        'f'
    } else if ft.is_fifo() {
        'p'
    } else if ft.is_socket() {
        's'
    } else if ft.is_char_device() {
        'c'
    } else if ft.is_block_device() {
        'b'
    } else {
        'U'
    }
}

fn access_ok(p: &Path, mode: libc::c_int) -> bool {
    let Ok(c) = CString::new(p.as_os_str().as_bytes()) else { return false };
    // SAFETY: c is NUL-terminated.
    unsafe { libc::access(c.as_ptr(), mode) == 0 }
}

fn test(t: &Test, f: &Entry, st: &State) -> bool {
    let Some(m) = &f.meta else { return false };
    match t {
        Test::True => true,
        Test::False => false,
        Test::Name { pat, icase } => {
            let name = f.path.file_name().map_or_else(|| f.path.as_os_str().as_bytes().to_vec(), |n| n.as_bytes().to_vec());
            // a start point like "dir/" is matched without its trailing slash
            let name = if name.is_empty() { f.path.as_os_str().as_bytes().to_vec() } else { name };
            fnmatch(pat, &name, *icase)
        }
        Test::Path { pat, icase } => fnmatch(pat, f.path.as_os_str().as_bytes(), *icase),
        Test::LinkName { pat, icase } => {
            fs::read_link(f.path).is_ok_and(|t| fnmatch(pat, t.as_os_str().as_bytes(), *icase))
        }
        Test::Regex { re, icase, extended } => {
            let anchored = format!("^({re})$");
            let anchored = if *extended { anchored } else { format!("^\\({re}\\)$") };
            usercore::posix_regex::Regex::new(&anchored, *extended, *icase)
                .is_ok_and(|r| r.is_match(&f.path.to_string_lossy()))
        }
        Test::Type { types, follow } => {
            // -type looks at the file itself (or what -L follows); -xtype at the other one
            let ft = if *follow != f.follow {
                if f.follow { f.lmeta().map(|m| m.file_type()) } else { fs::metadata(f.path).ok().map(|m| m.file_type()) }
                    .unwrap_or(m.file_type())
            } else {
                m.file_type()
            };
            types.contains(type_char(ft))
        }
        Test::Empty => {
            if m.is_file() {
                m.len() == 0
            } else if m.is_dir() {
                fs::read_dir(f.path).is_ok_and(|mut r| r.next().is_none())
            } else {
                false
            }
        }
        Test::Executable => access_ok(f.path, libc::X_OK),
        Test::Readable => access_ok(f.path, libc::R_OK),
        Test::Writable => access_ok(f.path, libc::W_OK),
        Test::Size { op, n, unit } => {
            // GNU rounds the size up to whole units
            let units = m.len().div_ceil(*unit);
            cmp(*op, units as i64, *n as i64)
        }
        Test::Time { field, op, n, minutes } => {
            let t = match field {
                'a' => m.atime(),
                'c' => m.ctime(),
                _ => m.mtime(),
            };
            let span = if *minutes { 60 } else { 86400 };
            let age = st.now - t;
            // -mtime N: whole days, fractions dropped; -mmin N: minutes rounded up
            let units = if *minutes { (age + span - 1).div_euclid(span) } else { age.div_euclid(span) };
            cmp(*op, units, *n)
        }
        Test::Newer { field, secs, nsecs } => {
            let (s, ns) = match field {
                'a' => (m.atime(), m.atime_nsec()),
                'c' => (m.ctime(), m.ctime_nsec()),
                _ => (m.mtime(), m.mtime_nsec()),
            };
            (s, ns) > (*secs, *nsecs)
        }
        Test::Perm { kind, mode } => {
            let have = m.permissions().mode() & 0o7777;
            match kind {
                '-' => have & mode == *mode,
                '/' => *mode == 0 || have & mode != 0,
                _ => have == *mode,
            }
        }
        Test::Uid(op, n) => cmp(*op, m.uid() as i64, *n as i64),
        Test::Gid(op, n) => cmp(*op, m.gid() as i64, *n as i64),
        Test::User(u) => m.uid() == *u,
        Test::Group(g) => m.gid() == *g,
        Test::NoUser => usercore::nss::user_by_uid(m.uid()).is_none(),
        Test::NoGroup => usercore::nss::group_by_gid(m.gid()).is_none(),
        Test::Links(op, n) => cmp(*op, m.nlink() as i64, *n as i64),
        Test::Inum(op, n) => cmp(*op, m.ino() as i64, *n as i64),
        Test::SameFile(dev, ino) => m.dev() == *dev && m.ino() == *ino,
    }
}

fn act(a: &Action, f: &Entry, st: &mut State) -> bool {
    match a {
        Action::Print => {
            let _ = st.out.write_all(f.path.as_os_str().as_bytes());
            let _ = st.out.write_all(b"\n");
            true
        }
        Action::Print0 => {
            let _ = st.out.write_all(f.path.as_os_str().as_bytes());
            let _ = st.out.write_all(b"\0");
            true
        }
        Action::Printf(fmt) => {
            let s = printf(fmt, f);
            let _ = st.out.write_all(&s);
            true
        }
        Action::Ls => {
            let s = ls_line(f);
            let _ = st.out.write_all(s.as_bytes());
            true
        }
        Action::Prune => {
            st.pruned = true;
            true
        }
        Action::Quit => {
            st.quit = true;
            true
        }
        Action::Delete => {
            let r = if f.meta.as_ref().is_some_and(|m| m.is_dir()) { fs::remove_dir(f.path) } else { fs::remove_file(f.path) };
            match r {
                Ok(()) => true,
                Err(e) => {
                    let _ = st.out.flush();
                    eprintln!("find: cannot delete '{}': {}", f.path.display(), io_reason(&e));
                    st.status = 1;
                    false
                }
            }
        }
        Action::Exec { cmd, batch: Some(k), in_dir } => {
            let p = if *in_dir { PathBuf::from(".").join(f.path.file_name().unwrap_or_default()) } else { f.path.to_path_buf() };
            st.batches[*k].2.push(p);
            if st.batches[*k].2.len() >= 4096 {
                let paths = std::mem::take(&mut st.batches[*k].2);
                let _ = st.out.flush();
                let dir = if *in_dir { f.path.parent() } else { None };
                if !run_command(cmd, &paths, dir) {
                    st.status = 1;
                }
            }
            true
        }
        Action::Exec { cmd, batch: None, in_dir } => {
            let _ = st.out.flush();
            let (target, dir) = if *in_dir {
                (PathBuf::from(".").join(f.path.file_name().unwrap_or_default()), f.path.parent().filter(|p| !p.as_os_str().is_empty()))
            } else {
                (f.path.to_path_buf(), None)
            };
            let t = target.to_string_lossy();
            let argv: Vec<String> = cmd.iter().map(|a| a.replace("{}", &t)).collect();
            run_command(&argv, &[], dir)
        }
    }
}

/// Run `argv` (+ `extra` paths); true when it exits 0.
fn run_command(argv: &[String], extra: &[PathBuf], dir: Option<&Path>) -> bool {
    let mut c = Command::new(&argv[0]);
    c.args(&argv[1..]).args(extra);
    if let Some(d) = dir {
        c.current_dir(d);
    }
    match c.status() {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!("find: '{}': {}", argv[0], io_reason(&e));
            false
        }
    }
}

fn mode_string(m: &fs::Metadata) -> String {
    let mode = m.permissions().mode();
    let mut s = String::new();
    s.push(match type_char(m.file_type()) {
        'f' => '-',
        c => c,
    });
    let bits = [(0o400, 'r'), (0o200, 'w'), (0o100, 'x'), (0o040, 'r'), (0o020, 'w'), (0o010, 'x'), (0o004, 'r'), (0o002, 'w'), (0o001, 'x')];
    for (i, (bit, c)) in bits.iter().enumerate() {
        let special = match i {
            2 => mode & 0o4000 != 0,
            5 => mode & 0o2000 != 0,
            8 => mode & 0o1000 != 0,
            _ => false,
        };
        let on = mode & bit != 0;
        s.push(match (special, on, i) {
            (true, true, 8) => 't',
            (true, false, 8) => 'T',
            (true, true, _) => 's',
            (true, false, _) => 'S',
            (false, true, _) => *c,
            (false, false, _) => '-',
        });
    }
    s
}

fn user_name(uid: u32) -> String {
    usercore::nss::user_by_uid(uid).map_or_else(|| uid.to_string(), |u| u.name)
}

fn group_name(gid: u32) -> String {
    usercore::nss::group_by_gid(gid).map_or_else(|| gid.to_string(), |g| g.name)
}

/// strftime(3) of `secs` in local time.
fn strftime(fmt: &str, secs: i64) -> String {
    let Ok(f) = CString::new(fmt) else { return String::new() };
    let t = secs as libc::time_t;
    // SAFETY: tm is written by localtime_r before use; buf has buf.len() bytes.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let mut buf = [0u8; 256];
        let n = libc::strftime(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), f.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

fn ls_line(f: &Entry) -> String {
    let Some(m) = &f.meta else { return String::new() };
    let date = if (SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) - m.mtime()).abs() > 15_778_476 {
        strftime("%b %e  %Y", m.mtime())
    } else {
        strftime("%b %e %H:%M", m.mtime())
    };
    let mut s = format!(
        "{:>9} {:>6} {} {:>3} {:<8} {:<8} {:>8} {} {}",
        m.ino(),
        m.blocks() / 2,
        mode_string(m),
        m.nlink(),
        user_name(m.uid()),
        group_name(m.gid()),
        m.len(),
        date,
        f.path.display()
    );
    if let Ok(t) = fs::read_link(f.path) {
        s += &format!(" -> {}", t.display());
    }
    s.push('\n');
    s
}

/// `-printf FORMAT`: the directives scripts use.
fn printf(fmt: &str, f: &Entry) -> Vec<u8> {
    let mut out = Vec::new();
    let b = fmt.as_bytes();
    let m = f.meta.as_ref();
    let name = || f.path.file_name().map_or_else(|| f.path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' if i + 1 < b.len() => {
                i += 1;
                match b[i] {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'0' => out.push(0),
                    b'\\' => out.push(b'\\'),
                    b'r' => out.push(b'\r'),
                    b'a' => out.push(7),
                    b'c' => return out,
                    c => out.extend_from_slice(&[b'\\', c]),
                }
                i += 1;
            }
            b'%' if i + 1 < b.len() => {
                // optional width/flags before the directive
                let mut j = i + 1;
                while j < b.len() && (b[j] == b'-' || b[j].is_ascii_digit()) {
                    j += 1;
                }
                let width = std::str::from_utf8(&b[i + 1..j]).unwrap_or("");
                let Some(&d) = b.get(j) else { break };
                let mut next = j + 1;
                let val: String = match d {
                    b'%' => "%".into(),
                    b'p' => f.path.to_string_lossy().into_owned(),
                    b'f' => name(),
                    b'h' => match f.path.parent() {
                        Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().into_owned(),
                        _ => ".".into(),
                    },
                    b'P' => f.path.strip_prefix(f.start).map_or(String::new(), |p| p.to_string_lossy().into_owned()),
                    b'H' => f.start.to_string_lossy().into_owned(),
                    b'd' => f.depth.to_string(),
                    b's' => m.map_or(0, |m| m.len()).to_string(),
                    b'k' => m.map_or(0, |m| m.blocks().div_ceil(2)).to_string(),
                    b'b' => m.map_or(0, |m| m.blocks()).to_string(),
                    b'm' => m.map_or(String::new(), |m| format!("{:o}", m.permissions().mode() & 0o7777)),
                    b'M' => m.map_or(String::new(), mode_string),
                    b'u' => m.map_or(String::new(), |m| user_name(m.uid())),
                    b'g' => m.map_or(String::new(), |m| group_name(m.gid())),
                    b'U' => m.map_or(0, |m| m.uid()).to_string(),
                    b'G' => m.map_or(0, |m| m.gid()).to_string(),
                    b'i' => m.map_or(0, |m| m.ino()).to_string(),
                    b'n' => m.map_or(0, |m| m.nlink()).to_string(),
                    b'y' => m.map_or('U', |m| type_char(m.file_type())).to_string(),
                    b'Y' => fs::metadata(f.path).map_or('N', |m| type_char(m.file_type())).to_string(),
                    b'l' => fs::read_link(f.path).map_or(String::new(), |t| t.to_string_lossy().into_owned()),
                    b't' | b'a' | b'c' => {
                        let t = m.map_or(0, |m| match d { b'a' => m.atime(), b'c' => m.ctime(), _ => m.mtime() });
                        strftime("%a %b %e %H:%M:%S %Y", t)
                    }
                    b'T' | b'A' | b'C' => {
                        let Some(&k) = b.get(j + 1) else { break };
                        next = j + 2;
                        let (t, ns) = m.map_or((0, 0), |m| match d {
                            b'A' => (m.atime(), m.atime_nsec()),
                            b'C' => (m.ctime(), m.ctime_nsec()),
                            _ => (m.mtime(), m.mtime_nsec()),
                        });
                        match k {
                            b'@' => format!("{t}.{ns:09}0"),
                            b'+' => strftime("%Y-%m-%d+%H:%M:%S", t) + &format!(".{ns:09}0"),
                            k => strftime(&format!("%{}", k as char), t),
                        }
                    }
                    other => format!("%{}", other as char),
                };
                let padded = match width.strip_prefix('-') {
                    Some(w) => format!("{val:<w$}", w = w.parse().unwrap_or(0)),
                    None => format!("{val:>w$}", w = width.parse().unwrap_or(0)),
                };
                out.extend_from_slice(padded.as_bytes());
                i = next;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// `[+-]N[unit]`: the comparison, N, and the unit in bytes (default 512-byte blocks).
fn parse_size(s: &str) -> Result<(char, u64, u64), String> {
    let (op, rest) = match s.chars().next() {
        Some(c @ ('+' | '-')) => (c, &s[1..]),
        _ => ('=', s),
    };
    let (num, unit) = match rest.chars().last() {
        Some('c') => (&rest[..rest.len() - 1], 1u64),
        Some('w') => (&rest[..rest.len() - 1], 2),
        Some('b') => (&rest[..rest.len() - 1], 512),
        Some('k') => (&rest[..rest.len() - 1], 1024),
        Some('M') => (&rest[..rest.len() - 1], 1024 * 1024),
        Some('G') => (&rest[..rest.len() - 1], 1024 * 1024 * 1024),
        _ => (rest, 512),
    };
    let n: u64 = num.parse().map_err(|_| format!("invalid size `{s}'"))?;
    Ok((op, n, unit))
}

fn parse_n(s: &str) -> Result<(char, i64), String> {
    let (op, rest) = match s.chars().next() {
        Some(c @ ('+' | '-')) => (c, &s[1..]),
        _ => ('=', s),
    };
    let n: i64 = rest.parse().map_err(|_| format!("invalid number `{s}'"))?;
    Ok((op, n))
}

/// Parse a `-maxdepth`/`-mindepth` argument; garbage is an error, never a silent 0.
fn parse_depth(s: &str) -> Result<usize, String> {
    s.parse().map_err(|_| format!("invalid depth `{s}'"))
}

/// An octal mode or a symbolic one (`u+x,g=r`), applied to 0 as `-perm` does.
fn parse_mode(s: &str) -> Option<u32> {
    if !s.is_empty() && s.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return u32::from_str_radix(s, 8).ok().filter(|m| *m <= 0o7777);
    }
    let mut mode = 0u32;
    for clause in s.split(',') {
        let at = clause.find(['+', '-', '='])?;
        let mut who = 0u32;
        for c in clause[..at].chars() {
            who |= match c {
                'u' => 0o4700,
                'g' => 0o2070,
                'o' => 0o1007,
                'a' => 0o7777,
                _ => return None,
            };
        }
        if who == 0 {
            who = 0o7777;
        }
        let op = clause.as_bytes()[at];
        let mut bits = 0u32;
        for c in clause[at + 1..].chars() {
            bits |= match c {
                'r' => 0o444,
                'w' => 0o222,
                'x' | 'X' => 0o111,
                's' => 0o6000,
                't' => 0o1000,
                _ => return None,
            };
        }
        bits &= who;
        match op {
            b'+' => mode |= bits,
            b'-' => mode &= !bits,
            _ => mode = (mode & !who) | bits,
        }
    }
    Some(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_depth_accepts_valid_numbers() {
        assert_eq!(parse_depth("0"), Ok(0));
        assert_eq!(parse_depth("3"), Ok(3));
    }

    #[test]
    fn parse_depth_rejects_garbage_instead_of_defaulting_to_zero() {
        // Regression: this used to silently become 0 via unwrap_or(0).
        assert!(parse_depth("abc").is_err());
        assert!(parse_depth("-1").is_err());
        assert!(parse_depth("").is_err());
        assert!(parse_depth("3.5").is_err());
    }

    #[test]
    fn parse_size_units() {
        assert_eq!(parse_size("10c"), Ok(('=', 10, 1)));
        assert_eq!(parse_size("+2k"), Ok(('+', 2, 1024)));
        assert_eq!(parse_size("-1M"), Ok(('-', 1, 1024 * 1024)));
        assert_eq!(parse_size("3"), Ok(('=', 3, 512)));
    }

    #[test]
    fn parse_n_signs() {
        assert_eq!(parse_n("+7"), Ok(('+', 7)));
        assert_eq!(parse_n("-3"), Ok(('-', 3)));
        assert_eq!(parse_n("5"), Ok(('=', 5)));
        assert!(parse_n("nope").is_err());
    }

    #[test]
    fn modes() {
        assert_eq!(parse_mode("755"), Some(0o755));
        assert_eq!(parse_mode("u+x"), Some(0o100));
        assert_eq!(parse_mode("g=rw,o+r"), Some(0o064));
        assert_eq!(parse_mode("bad"), None);
    }

    #[test]
    fn expression_precedence() {
        let args: Vec<String> = ["-name", "a", "-o", "-name", "b", "-type", "f"].iter().map(|s| s.to_string()).collect();
        let mut o = Opts { max_depth: 0, min_depth: 0, depth_first: false, follow: false, xdev: false };
        let mut p = Parser { args: &args, pos: 0, opts: &mut o, has_action: false, batches: 0 };
        // -o is looser: a OR (b AND f)
        assert!(matches!(p.parse_or().unwrap(), Expr::Or(_, r) if matches!(*r, Expr::And(..))));
    }
}
