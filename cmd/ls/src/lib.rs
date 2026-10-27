//! user ls — list directory contents (Zainium coloured).
//!
//! GNU ls's layouts: one per line when not writing to a terminal, columns (`-C`, `-x`)
//! otherwise, `-m` with commas, `-l` with columns as wide as their widest entry.
//! Operands that aren't directories come first, then each directory's contents.

use colored::Colorize;
use std::cmp::Ordering;
use std::fs;
use std::io::{IsTerminal, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use std::io;
use usercore::Ui;

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Long,
    One,
    Columns,
    Across,
    Commas,
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Name,
    Size,
    Time,
    None,
    Version,
    Extension,
}

#[derive(Clone, Copy, PartialEq)]
enum TimeField {
    Modify,
    Change,
    Access,
}

struct Opts {
    all: bool,
    almost_all: bool,
    format: Format,
    human: bool,
    reverse: bool,
    classify: bool,
    slash: bool,
    inode: bool,
    blocks: bool,
    directory: bool,
    recursive: bool,
    deref: bool,
    deref_args: bool,
    sort: Sort,
    time: TimeField,
    dirs_first: bool,
    quote: bool,
    numeric: bool,
    show_owner: bool,
    show_group: bool,
    color: bool,
    width: usize,
    tabsize: usize,
    /// strftime formats for `-l` times: (older than six months or future, recent).
    time_fmt: (String, String),
    /// Current time in seconds, for choosing between the two `time_fmt`s.
    now: i64,
}

impl Opts {
    fn new() -> Self {
        let tty = io::stdout().is_terminal();
        Opts {
            all: false,
            almost_all: false,
            format: if tty { Format::Columns } else { Format::One },
            human: false,
            reverse: false,
            classify: false,
            slash: false,
            inode: false,
            blocks: false,
            directory: false,
            recursive: false,
            deref: false,
            deref_args: false,
            sort: Sort::Name,
            time: TimeField::Modify,
            dirs_first: false,
            quote: false,
            numeric: false,
            show_owner: true,
            show_group: true,
            color: tty && std::env::var_os("NO_COLOR").is_none(),
            width: terminal_width(tty),
            tabsize: 8,
            time_fmt: std::env::var("TIME_STYLE")
                .ok()
                .and_then(|s| time_style(&s))
                .unwrap_or_else(|| time_style("locale").unwrap()),
            now: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
        }
    }
}

/// COLUMNS, else the terminal's width, else 80.
fn terminal_width(tty: bool) -> usize {
    if let Some(w) = std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).filter(|w: &usize| *w > 0) {
        return w;
    }
    if tty {
        // SAFETY: TIOCGWINSZ fills a winsize struct.
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
                return ws.ws_col as usize;
            }
        }
    }
    80
}

/// One thing to list: where it is, the name shown, and its metadata.
struct Item {
    path: PathBuf,
    name: String,
    meta: fs::Metadata,
}

/// Entry point for the `ls` utility. Parses `std::env::args()` and lists
/// the named FILE(s), or the current directory if none are given.
///
/// Returns 0 on success, 1 for a minor problem (a subdirectory), 2 for serious
/// trouble (an operand that can't be accessed, a usage error), as GNU ls.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_args(&args)
}

/// Parse and list with explicit argv (for dir/vdir multicall wrappers).
pub fn run_args(args: &[String]) -> i32 {
    let ui = Ui::new("ls");
    let mut o = Opts::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    // -c/-u sort by that time unless -l is given without -t
    let mut time_given = false;
    let mut sort_time_given = false;

    let mut it = args.iter();
    let mut operands_only = false;
    while let Some(arg) = it.next() {
        if operands_only {
            paths.push(PathBuf::from(arg));
            continue;
        }
        let long_value = |name: &str, it: &mut std::slice::Iter<String>| -> Option<String> {
            match arg.strip_prefix(&format!("{name}=")) {
                Some(v) => Some(v.to_string()),
                None if arg == name => it.next().cloned(),
                None => None,
            }
        };
        match arg.as_str() {
            "--" => operands_only = true,
            "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("ls (user_utils) 0.2.0");
                return 0;
            }
            "--all" => o.all = true,
            "--almost-all" => o.almost_all = true,
            "--human-readable" => o.human = true,
            "--si" => o.human = true,
            "--reverse" => o.reverse = true,
            "--classify" | "--classify=always" | "--classify=auto" | "--indicator-style=classify" => o.classify = true,
            "--indicator-style=slash" => o.slash = true,
            "--indicator-style=none" => {
                o.classify = false;
                o.slash = false;
            }
            "--inode" => o.inode = true,
            "--size" => o.blocks = true,
            "--directory" => o.directory = true,
            "--recursive" => o.recursive = true,
            "--dereference" => o.deref = true,
            "--dereference-command-line" => o.deref_args = true,
            "--group-directories-first" => o.dirs_first = true,
            "--quote-name" => o.quote = true,
            "--literal" => o.quote = false,
            "--numeric-uid-gid" => {
                o.numeric = true;
                o.format = Format::Long;
            }
            "--no-group" => o.show_group = false,
            "--color" | "--color=always" | "--color=yes" | "--colour" | "--colour=always" => o.color = true,
            "--color=never" | "--color=no" | "--colour=never" => o.color = false,
            "--color=auto" | "--colour=auto" => o.color = io::stdout().is_terminal(),
            "--full-time" => {
                o.format = Format::Long;
                o.time_fmt = time_style("full-iso").unwrap();
            }
            s if s == "--format" || s.starts_with("--format=") => {
                o.format = match long_value("--format", &mut it).as_deref() {
                    Some("long" | "verbose") => Format::Long,
                    Some("single-column") => Format::One,
                    Some("vertical") => Format::Columns,
                    Some("across" | "horizontal") => Format::Across,
                    Some("commas") => Format::Commas,
                    v => {
                        ui.err(&format!("invalid argument '{}' for '--format'", v.unwrap_or("")));
                        return 2;
                    }
                }
            }
            s if s == "--sort" || s.starts_with("--sort=") => {
                o.sort = match long_value("--sort", &mut it).as_deref() {
                    Some("none") => Sort::None,
                    Some("size") => Sort::Size,
                    Some("time") => {
                        sort_time_given = true;
                        Sort::Time
                    }
                    Some("version") => Sort::Version,
                    Some("extension") => Sort::Extension,
                    Some("name") => Sort::Name,
                    v => {
                        ui.err(&format!("invalid argument '{}' for '--sort'", v.unwrap_or("")));
                        return 2;
                    }
                }
            }
            s if s == "--time" || s.starts_with("--time=") => {
                time_given = true;
                o.time = match long_value("--time", &mut it).as_deref() {
                    Some("atime" | "access" | "use") => TimeField::Access,
                    Some("ctime" | "status") => TimeField::Change,
                    Some("mtime" | "modification") => TimeField::Modify,
                    v => {
                        ui.err(&format!("invalid argument '{}' for '--time'", v.unwrap_or("")));
                        return 2;
                    }
                }
            }
            s if s == "--width" || s.starts_with("--width=") => match long_value("--width", &mut it).and_then(|v| v.parse().ok()) {
                Some(w) => o.width = if w == 0 { usize::MAX } else { w },
                None => {
                    ui.err("invalid line width");
                    return 2;
                }
            },
            s if s == "--tabsize" || s.starts_with("--tabsize=") => {
                o.tabsize = long_value("--tabsize", &mut it).and_then(|v| v.parse().ok()).unwrap_or(8);
            }
            s if s == "--time-style" || s.starts_with("--time-style=") => {
                let Some(v) = long_value("--time-style", &mut it) else {
                    ui.err("option '--time-style' requires an argument");
                    return 2;
                };
                match time_style(&v) {
                    Some(f) => o.time_fmt = f,
                    None => {
                        ui.err(&format!(
                            "invalid argument '{v}' for '--time-style'\n\
                             Valid arguments are: [posix-]full-iso, [posix-]long-iso, \
                             [posix-]iso, [posix-]locale, +FORMAT"
                        ));
                        return 2;
                    }
                }
            }
            s if s.starts_with("--") => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 2;
            }
            s if s.starts_with('-') && s.len() > 1 => {
                let chars: Vec<char> = s.chars().skip(1).collect();
                let mut k = 0;
                while k < chars.len() {
                    let c = chars[k];
                    match c {
                        'a' => o.all = true,
                        'A' => o.almost_all = true,
                        'l' => o.format = Format::Long,
                        '1' => o.format = Format::One,
                        'C' => o.format = Format::Columns,
                        'x' => o.format = Format::Across,
                        'm' => o.format = Format::Commas,
                        'g' => {
                            o.format = Format::Long;
                            o.show_owner = false;
                        }
                        'o' => {
                            o.format = Format::Long;
                            o.show_group = false;
                        }
                        'n' => {
                            o.format = Format::Long;
                            o.numeric = true;
                        }
                        'G' => o.show_group = false,
                        'h' => o.human = true,
                        'k' => {}
                        'r' => o.reverse = true,
                        'F' => o.classify = true,
                        'p' => o.slash = true,
                        'i' => o.inode = true,
                        's' => o.blocks = true,
                        'd' => o.directory = true,
                        'R' => o.recursive = true,
                        'L' => o.deref = true,
                        'H' => o.deref_args = true,
                        't' => {
                            o.sort = Sort::Time;
                            sort_time_given = true;
                        }
                        'S' => o.sort = Sort::Size,
                        'U' => o.sort = Sort::None,
                        'f' => {
                            o.sort = Sort::None;
                            o.all = true;
                            o.color = false;
                        }
                        'v' => o.sort = Sort::Version,
                        'X' => o.sort = Sort::Extension,
                        'c' => {
                            o.time = TimeField::Change;
                            time_given = true;
                        }
                        'u' => {
                            o.time = TimeField::Access;
                            time_given = true;
                        }
                        'Q' => o.quote = true,
                        'N' => o.quote = false,
                        'b' | 'q' | 'B' | 'Z' => {}
                        'w' | 'T' | 'I' => {
                            let inline: String = chars[k + 1..].iter().collect();
                            let v = if inline.is_empty() { it.next().cloned() } else { Some(inline) };
                            let Some(v) = v else {
                                ui.err(&format!("option requires an argument -- '{c}'"));
                                return 2;
                            };
                            match c {
                                'w' => match v.parse::<usize>() {
                                    Ok(w) => o.width = if w == 0 { usize::MAX } else { w },
                                    Err(_) => {
                                        ui.err(&format!("invalid line width: '{v}'"));
                                        return 2;
                                    }
                                },
                                'T' => o.tabsize = v.parse().unwrap_or(8),
                                _ => {}
                            }
                            break;
                        }
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 2;
                        }
                    }
                    k += 1;
                }
            }
            other => paths.push(PathBuf::from(other)),
        }
    }
    // -c / -u: sort by that time, unless only -l asks to show it
    if time_given && o.sort == Sort::Name && !(o.format == Format::Long && !sort_time_given) {
        o.sort = Sort::Time;
    }
    let explicit = !paths.is_empty();
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }

    let mut status = 0;
    let mut files: Vec<Item> = Vec::new();
    let mut dirs: Vec<Item> = Vec::new();
    // symlinks named on the command line are followed unless -l/-d/-F looks at the link
    let follow_args = o.deref || o.deref_args || !(o.format == Format::Long || o.directory || o.classify);
    for p in &paths {
        let meta = if follow_args {
            fs::metadata(p).or_else(|_| fs::symlink_metadata(p))
        } else {
            fs::symlink_metadata(p)
        };
        match meta {
            Ok(m) => {
                let item = Item { path: p.clone(), name: p.to_string_lossy().into_owned(), meta: m };
                if item.meta.is_dir() && !o.directory {
                    dirs.push(item);
                } else {
                    files.push(item);
                }
            }
            Err(e) => {
                ui.err(&format!("cannot access '{}': {}", p.display(), reason(&e)));
                status = 2;
            }
        }
    }
    sort_items(&mut files, &o);
    sort_items(&mut dirs, &o);

    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut printed = false;
    if !files.is_empty() {
        print_items(&mut out, &files, &o, false);
        printed = true;
    }
    let headers = explicit && (paths.len() > 1 || o.recursive) || o.recursive;
    for d in &dirs {
        if let Err(e) = list_dir(&mut out, &d.path, &o, headers, printed) {
            let _ = out.flush();
            ui.err(&format!("cannot open directory '{}': {}", d.path.display(), reason(&e)));
            status = 2;
        }
        printed = true;
    }
    let _ = out.flush();
    status
}

fn reason(e: &io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// Print `ls --help` usage text to stdout.
fn print_help() {
    print!(
        "Usage: ls [OPTION]... [FILE]...\n\
 List information about the FILEs (the current directory by default).\n\n\
 -a, --all                  do not ignore entries starting with .\n\
 -A, --almost-all           do not list implied . and ..\n\
 -C / -x / -m / -1          list in columns / across / with commas / one per line\n\
 -c / -u                    use ctime / atime (with -l: show it; with -t: sort by it)\n\
 -d, --directory            list directories themselves, not their contents\n\
 -f                         like -aU\n\
 -F, --classify             append indicator (one of */=@|) to entries\n\
 -g / -o                    like -l, without the owner / group\n\
 -G, --no-group             in a long listing, don't print group names\n\
 -h, --human-readable       with -l and -s, print sizes like 1K 234M 2G etc.\n\
 -H / -L                    follow symlinks on the command line / everywhere\n\
 -i, --inode                print the index number of each file\n\
 -l                         use a long listing format\n\
 -n, --numeric-uid-gid      like -l, but list numeric user and group IDs\n\
 -p                         append / indicator to directories\n\
 -Q, --quote-name           enclose entry names in double quotes\n\
 -r, --reverse              reverse order while sorting\n\
 -R, --recursive            list subdirectories recursively\n\
 -s, --size                 print the allocated size of each file, in blocks\n\
 -S / -t / -U / -v / -X     sort by size / time / none / version / extension\n\
 -w, --width=COLS           set output width to COLS; 0 means no limit\n\
 --group-directories-first  group directories before files\n\
 --sort=WORD / --time=WORD / --format=WORD\n\
 --full-time                like -l --time-style=full-iso\n\
 --time-style=STYLE         full-iso, long-iso, iso, locale or +FORMAT\n\
 --color[=WHEN]             colorize the output; WHEN can be always/auto/never\n"
    );
}

fn file_time(m: &fs::Metadata, f: TimeField) -> (i64, i64) {
    match f {
        TimeField::Modify => (m.mtime(), m.mtime_nsec()),
        TimeField::Change => (m.ctime(), m.ctime_nsec()),
        TimeField::Access => (m.atime(), m.atime_nsec()),
    }
}

fn is_dir_item(i: &Item) -> bool {
    i.meta.is_dir() || (i.meta.file_type().is_symlink() && fs::metadata(&i.path).is_ok_and(|m| m.is_dir()))
}

fn sort_items(items: &mut [Item], o: &Opts) {
    if o.sort == Sort::None {
        if o.reverse {
            items.reverse();
        }
    } else {
        items.sort_by(|a, b| {
            let by_name = || a.name.as_bytes().cmp(b.name.as_bytes());
            let ord = match o.sort {
                Sort::Size => b.meta.len().cmp(&a.meta.len()).then_with(by_name),
                Sort::Time => file_time(&b.meta, o.time).cmp(&file_time(&a.meta, o.time)).then_with(by_name),
                Sort::Version => version_cmp(&a.name, &b.name).then_with(by_name),
                Sort::Extension => {
                    let ext = |n: &str| n.rfind('.').filter(|i| *i > 0).map_or(String::new(), |i| n[i..].to_string());
                    ext(&a.name).cmp(&ext(&b.name)).then_with(by_name)
                }
                _ => by_name(),
            };
            if o.reverse { ord.reverse() } else { ord }
        });
    }
    if o.dirs_first {
        items.sort_by_key(|i| !is_dir_item(i));
    }
}

/// GNU-like version order: digit runs compare as numbers.
fn version_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na = std::str::from_utf8(&a[si..i]).unwrap_or("0").trim_start_matches('0');
            let nb = std::str::from_utf8(&b[sj..j]).unwrap_or("0").trim_start_matches('0');
            match na.len().cmp(&nb.len()).then_with(|| na.cmp(nb)) {
                Ordering::Equal => {}
                o => return o,
            }
        } else {
            match a[i].cmp(&b[j]) {
                Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
                o => return o,
            }
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// List the contents of directory `path`, then (with -R) its subdirectories.
fn list_dir(out: &mut impl Write, path: &Path, o: &Opts, header: bool, blank_before: bool) -> io::Result<()> {
    let ui = Ui::new("ls");
    let rd = fs::read_dir(path)?;
    let mut items: Vec<Item> = Vec::new();
    if o.all {
        for special in [".", ".."] {
            if let Ok(m) = fs::symlink_metadata(path.join(special)) {
                items.push(Item { path: path.join(special), name: special.into(), meta: m });
            }
        }
    }
    for ent in rd {
        let Ok(ent) = ent else { continue };
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && !o.all && !o.almost_all {
            continue;
        }
        let p = ent.path();
        let meta = if o.deref { fs::metadata(&p).or_else(|_| fs::symlink_metadata(&p)) } else { fs::symlink_metadata(&p) };
        match meta {
            Ok(m) => items.push(Item { path: p, name, meta: m }),
            Err(e) => ui.err(&format!("cannot access '{}': {}", p.display(), reason(&e))),
        }
    }
    sort_items(&mut items, o);
    if blank_before {
        writeln!(out)?;
    }
    if header {
        writeln!(out, "{}:", path.display())?;
    }
    print_items(out, &items, o, true);
    if o.recursive {
        for i in &items {
            if i.meta.is_dir() && i.name != "." && i.name != ".." {
                if let Err(e) = list_dir(out, &i.path, o, true, true) {
                    let _ = out.flush();
                    ui.err(&format!("cannot open directory '{}': {}", i.path.display(), reason(&e)));
                }
            }
        }
    }
    Ok(())
}

/// The name as shown: quoted with -Q, coloured, with its -F/-p indicator.
fn shown_name(i: &Item, o: &Opts) -> (String, usize) {
    let name = if o.quote { format!("\"{}\"", i.name.replace('\\', "\\\\").replace('"', "\\\"")) } else { i.name.clone() };
    let class = if o.classify {
        classify_char(&i.meta)
    } else if o.slash && i.meta.is_dir() {
        "/"
    } else {
        ""
    };
    let width = name.chars().count() + class.len();
    (format!("{}{class}", colorize_name(&name, &i.meta, o)), width)
}

/// GNU's -h: under 1024 as it is, else one decimal below 10 and whole numbers above,
/// always rounded up.
fn human_size(n: u64) -> String {
    const UNITS: [&str; 7] = ["", "K", "M", "G", "T", "P", "E"];
    if n < 1024 {
        return n.to_string();
    }
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if v < 10.0 {
        let r = (v * 10.0).ceil() / 10.0;
        if r < 10.0 {
            return format!("{r:.1}{}", UNITS[u]);
        }
        return format!("{}{}", 10, UNITS[u]);
    }
    let r = v.ceil();
    if r >= 1024.0 && u < UNITS.len() - 1 {
        return format!("1.0{}", UNITS[u + 1]);
    }
    format!("{r:.0}{}", UNITS[u])
}

/// Allocated size in 1K blocks (or -h).
fn block_size(m: &fs::Metadata, o: &Opts) -> String {
    let kb = m.blocks().div_ceil(2);
    if o.human { human_size(kb * 1024) } else { kb.to_string() }
}

/// Pad from column `from` to `to`, with tabs where they fit, as GNU ls does.
fn indent(out: &mut impl Write, mut from: usize, to: usize, tabsize: usize) {
    while from < to {
        if tabsize != 0 && to / tabsize > (from + 1) / tabsize {
            let _ = out.write_all(b"\t");
            from += tabsize - from % tabsize;
        } else {
            let _ = out.write_all(b" ");
            from += 1;
        }
    }
}

fn print_items(out: &mut impl Write, items: &[Item], o: &Opts, in_dir: bool) {
    if o.format == Format::Long {
        print_long(out, items, o, in_dir);
        return;
    }
    if in_dir && o.blocks {
        let total: u64 = items.iter().map(|i| i.meta.blocks().div_ceil(2)).sum();
        let total = if o.human { human_size(total * 1024) } else { total.to_string() };
        let _ = writeln!(out, "total {total}");
    }
    // prefix columns (inode, blocks) are right-aligned to their widest
    let ino_w = items.iter().map(|i| i.meta.ino().to_string().len()).max().unwrap_or(0);
    let blk_w = items.iter().map(|i| block_size(&i.meta, o).len()).max().unwrap_or(0);
    let cells: Vec<(String, usize)> = items
        .iter()
        .map(|i| {
            let mut s = String::new();
            if o.inode {
                s += &format!("{:>ino_w$} ", i.meta.ino());
            }
            if o.blocks {
                s += &format!("{:>blk_w$} ", block_size(&i.meta, o));
            }
            let pre = s.chars().count();
            let (n, w) = shown_name(i, o);
            (s + &n, pre + w)
        })
        .collect();
    match o.format {
        Format::One => {
            for (c, _) in &cells {
                let _ = writeln!(out, "{c}");
            }
        }
        Format::Commas => {
            let mut pos = 0;
            for (k, (c, w)) in cells.iter().enumerate() {
                if k > 0 {
                    if pos + w + 2 < o.width {
                        let _ = out.write_all(b", ");
                        pos += 2;
                    } else {
                        let _ = out.write_all(b",\n");
                        pos = 0;
                    }
                }
                let _ = out.write_all(c.as_bytes());
                pos += w;
            }
            if !cells.is_empty() {
                let _ = out.write_all(b"\n");
            }
        }
        Format::Columns | Format::Across => {
            let across = o.format == Format::Across;
            let n = cells.len();
            if n == 0 {
                return;
            }
            // the most columns that fit, each as wide as its widest name + 2
            let mut best = (1, vec![cells.iter().map(|c| c.1).max().unwrap_or(0)]);
            for cols in (2..=n.min(o.width / 3 + 1)).rev() {
                let rows = n.div_ceil(cols);
                if !across && cols > 1 && (cols - 1) * rows >= n {
                    continue;
                }
                let mut widths = vec![0usize; cols];
                for (k, c) in cells.iter().enumerate() {
                    let col = if across { k % cols } else { k / rows };
                    widths[col] = widths[col].max(c.1 + if col + 1 < cols { 2 } else { 0 });
                }
                if widths.iter().sum::<usize>() < o.width {
                    best = (cols, widths);
                    break;
                }
            }
            let (cols, widths) = best;
            let rows = n.div_ceil(cols);
            if across {
                let mut pos = 0;
                for (k, (c, w)) in cells.iter().enumerate() {
                    let col = k % cols;
                    let _ = out.write_all(c.as_bytes());
                    if col + 1 == cols || k + 1 == n {
                        let _ = out.write_all(b"\n");
                        pos = 0;
                    } else {
                        indent(out, pos + w, pos + widths[col], o.tabsize);
                        pos += widths[col];
                    }
                }
            } else {
                for r in 0..rows {
                    let mut pos = 0;
                    for col in 0..cols {
                        let k = col * rows + r;
                        let Some((c, w)) = cells.get(k) else { break };
                        let _ = out.write_all(c.as_bytes());
                        if col * rows + r + rows < n && col + 1 < cols {
                            indent(out, pos + w, pos + widths[col], o.tabsize);
                            pos += widths[col];
                        }
                    }
                    let _ = out.write_all(b"\n");
                }
            }
        }
        Format::Long => unreachable!(),
    }
}

/// `-l`: every column as wide as its widest entry, as GNU ls.
fn print_long(out: &mut impl Write, items: &[Item], o: &Opts, in_dir: bool) {
    if in_dir {
        let total: u64 = items.iter().map(|i| i.meta.blocks().div_ceil(2)).sum();
        let total = if o.human { human_size(total * 1024) } else { total.to_string() };
        let _ = writeln!(out, "total {total}");
    }
    struct Row {
        ino: String,
        blocks: String,
        mode: String,
        nlink: String,
        owner: String,
        group: String,
        size: String,
        time: String,
        name: String,
    }
    let rows: Vec<Row> = items
        .iter()
        .map(|i| {
            let m = &i.meta;
            let ft = m.file_type();
            let size = if ft.is_char_device() || ft.is_block_device() {
                let r = m.rdev();
                format!("{}, {}", libc::major(r), libc::minor(r))
            } else if o.human {
                human_size(m.len())
            } else {
                m.len().to_string()
            };
            let (t, ns) = file_time(m, o.time);
            const SIX_MONTHS: i64 = 31_556_952 / 2;
            let recent = t <= o.now && o.now - t < SIX_MONTHS;
            let fmt = if recent { &o.time_fmt.1 } else { &o.time_fmt.0 };
            let (name, _) = shown_name(i, o);
            let mut name = name;
            if ft.is_symlink() {
                if let Ok(target) = fs::read_link(&i.path) {
                    name += &format!(" -> {}", target.display());
                }
            }
            Row {
                ino: m.ino().to_string(),
                blocks: block_size(m, o),
                mode: mode_string(m),
                nlink: m.nlink().to_string(),
                owner: if o.numeric { m.uid().to_string() } else { uid_name(m.uid()) },
                group: if o.numeric { m.gid().to_string() } else { gid_name(m.gid()) },
                size,
                time: strftime_local(fmt, t, ns),
                name,
            }
        })
        .collect();
    let w = |f: &dyn Fn(&Row) -> &String| rows.iter().map(|r| f(r).chars().count()).max().unwrap_or(0);
    let (wi, wb, wl, wo, wg, ws) = (w(&|r| &r.ino), w(&|r| &r.blocks), w(&|r| &r.nlink), w(&|r| &r.owner), w(&|r| &r.group), w(&|r| &r.size));
    for r in &rows {
        let mut line = String::new();
        if o.inode {
            line += &format!("{:>wi$} ", r.ino);
        }
        if o.blocks {
            line += &format!("{:>wb$} ", r.blocks);
        }
        line += &format!("{} {:>wl$} ", r.mode, r.nlink);
        if o.show_owner {
            line += &format!("{:<wo$} ", r.owner);
        }
        if o.show_group {
            line += &format!("{:<wg$} ", r.group);
        }
        line += &format!("{:>ws$} {} {}", r.size, r.time, r.name);
        let _ = writeln!(out, "{line}");
    }
}

/// Render the 10-character `-rwxr-xr-x`-style mode string for `meta`:
/// file-type letter followed by owner/group/other `rwx` triples, with
/// setuid/setgid/sticky folded into the executable-bit positions.
fn mode_string(meta: &fs::Metadata) -> String {
    let ft = meta.file_type();
    let mut s = String::with_capacity(10);
    s.push(if ft.is_dir() {
        'd'
    } else if ft.is_symlink() {
        'l'
    } else if ft.is_char_device() {
        'c'
    } else if ft.is_block_device() {
        'b'
    } else if ft.is_fifo() {
        'p'
    } else if ft.is_socket() {
        's'
    } else {
        '-'
    });
    let mode = meta.mode();
    let bits = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    for (bit, ch) in bits {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    // setuid/setgid/sticky display
    if mode & 0o4000 != 0 {
        let c = if mode & 0o100 != 0 { 's' } else { 'S' };
        s.replace_range(3..4, &c.to_string());
    }
    if mode & 0o2000 != 0 {
        let c = if mode & 0o010 != 0 { 's' } else { 'S' };
        s.replace_range(6..7, &c.to_string());
    }
    if mode & 0o1000 != 0 {
        let c = if mode & 0o001 != 0 { 't' } else { 'T' };
        s.replace_range(9..10, &c.to_string());
    }
    s
}

/// Apply the Zainium color palette to `name` based on `meta`'s file type
/// (directory/symlink/executable/fifo/socket/device/known archive
/// extension), or return `name` unchanged if `o.color` is off.
fn colorize_name(name: &str, meta: &fs::Metadata, o: &Opts) -> String {
    if !o.color {
        return name.to_string();
    }
    let ft = meta.file_type();
    if ft.is_dir() {
        name.bright_blue().bold().to_string()
    } else if ft.is_symlink() {
        name.bright_cyan().to_string()
    } else if meta.mode() & 0o111 != 0 {
        name.bright_green().to_string()
    } else if ft.is_fifo() {
        name.yellow().to_string()
    } else if ft.is_socket() {
        name.bright_magenta().to_string()
    } else if ft.is_block_device() || ft.is_char_device() {
        name.bright_yellow().bold().to_string()
    } else if name.ends_with(".tar")
        || name.ends_with(".gz")
        || name.ends_with(".xz")
        || name.ends_with(".zip")
        || name.ends_with(".zst")
    {
        name.bright_red().to_string()
    } else {
        name.to_string()
    }
}

/// Return the `-F`/`--classify` indicator suffix for `meta`'s file type
/// (`/` dir, `@` symlink, `|` fifo, `=` socket, `*` executable), or `""`
/// for a plain non-executable file.
fn classify_char(meta: &fs::Metadata) -> &'static str {
    let ft = meta.file_type();
    if ft.is_dir() {
        "/"
    } else if ft.is_symlink() {
        "@"
    } else if ft.is_fifo() {
        "|"
    } else if ft.is_socket() {
        "="
    } else if meta.mode() & 0o111 != 0 {
        "*"
    } else {
        ""
    }
}

/// Map a `--time-style` value to (old, recent) strftime formats, as GNU ls
/// does. `posix-STYLE` means the locale style here, since ls runs in the C locale.
fn time_style(style: &str) -> Option<(String, String)> {
    if let Some(f) = style.strip_prefix('+') {
        let (old, recent) = f.split_once('\n').unwrap_or((f, f));
        return Some((old.to_string(), recent.to_string()));
    }
    let (old, recent) = match style {
        "full-iso" => ("%Y-%m-%d %H:%M:%S.%N %z", "%Y-%m-%d %H:%M:%S.%N %z"),
        "long-iso" => ("%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M"),
        "iso" => ("%Y-%m-%d ", "%m-%d %H:%M"),
        "locale" | "default" => ("%b %e  %Y", "%b %e %H:%M"),
        s if s.starts_with("posix-") && time_style(&s[6..]).is_some() => {
            ("%b %e  %Y", "%b %e %H:%M")
        }
        _ => return None,
    };
    Some((old.to_string(), recent.to_string()))
}

/// `strftime(3)` of `secs` in local time, with `%N` expanded to `nsec`.
fn strftime_local(fmt: &str, secs: i64, nsec: i64) -> String {
    let mut f = String::with_capacity(fmt.len());
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            f.push(c);
            continue;
        }
        match chars.next() {
            Some('N') => f.push_str(&format!("{:09}", nsec.clamp(0, 999_999_999))),
            Some(n) => {
                f.push('%');
                f.push(n);
            }
            None => f.push('%'),
        }
    }
    let Ok(cfmt) = std::ffi::CString::new(f) else {
        return String::new();
    };
    // `libc::tm` has no `Default` impl, but every field is a public primitive
    // (int/long/pointer), so we can build a zeroed value with a plain struct
    // literal instead of `mem::zeroed`.
    let mut tm = libc::tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    let t = secs as libc::time_t;
    let mut buf = vec![0u8; 512];
    // SAFETY: `t` and `tm` are valid, initialized stack values that outlive
    // both calls; `localtime_r` reads `t` and writes `tm`. `strftime` writes at
    // most `buf.len()` bytes into `buf`, reads the NUL-terminated `cfmt` and
    // `tm`, and returns the number of bytes written (0 if they did not fit).
    let n = unsafe {
        libc::localtime_r(&t, &mut tm);
        libc::strftime(
            buf.as_mut_ptr().cast::<libc::c_char>(),
            buf.len(),
            cfmt.as_ptr(),
            &tm,
        )
    };
    buf.truncate(n);
    String::from_utf8_lossy(&buf).into_owned()
}

/// A user's name: Zainium's passwd file, then the C library, else the number.
fn uid_name(uid: u32) -> String {
    if let Some(pw) = usercore::nss::user_by_uid(uid) {
        return pw.name;
    }
    // SAFETY: the returned entry is read at once, before any other passwd call.
    unsafe {
        let p = libc::getpwuid(uid);
        if !p.is_null() {
            return std::ffi::CStr::from_ptr((*p).pw_name).to_string_lossy().into_owned();
        }
    }
    uid.to_string()
}

/// A group's name: Zainium's group file, then the C library, else the number.
fn gid_name(gid: u32) -> String {
    if let Some(gr) = usercore::nss::group_by_gid(gid) {
        return gr.name;
    }
    // SAFETY: the returned entry is read at once, before any other group call.
    unsafe {
        let g = libc::getgrgid(gid);
        if !g.is_null() {
            return std::ffi::CStr::from_ptr((*g).gr_name).to_string_lossy().into_owned();
        }
    }
    gid.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("user_ls_test_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn base_opts() -> Opts {
        let mut o = Opts::new();
        o.color = false;
        o.format = Format::One;
        o
    }

    #[test]
    fn mode_string_regular_file() {
        let dir = scratch_dir("mode_reg");
        let f = dir.join("f");
        fs::write(&f, b"x").unwrap();
        fs::set_permissions(&f, fs::Permissions::from_mode(0o644)).unwrap();
        let meta = fs::symlink_metadata(&f).unwrap();
        assert_eq!(mode_string(&meta), "-rw-r--r--");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mode_string_directory() {
        let dir = scratch_dir("mode_dir");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        let meta = fs::symlink_metadata(&dir).unwrap();
        assert_eq!(mode_string(&meta), "drwxr-xr-x");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mode_string_setuid_bit() {
        let dir = scratch_dir("mode_setuid");
        let f = dir.join("f");
        fs::write(&f, b"x").unwrap();
        fs::set_permissions(&f, fs::Permissions::from_mode(0o4755)).unwrap();
        let meta = fs::symlink_metadata(&f).unwrap();
        assert_eq!(mode_string(&meta), "-rwsr-xr-x");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn human_size_formats_units() {
        assert_eq!(human_size(0), "0");
        assert_eq!(human_size(43), "43");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(1024 * 1024), "1.0M");
        assert_eq!(human_size(12345), "13K");
        assert_eq!(human_size(10239), "10K");
    }

    #[test]
    fn classify_char_by_type() {
        let dir = scratch_dir("classify");
        let f = dir.join("plain");
        fs::write(&f, b"x").unwrap();
        let meta = fs::symlink_metadata(&f).unwrap();
        assert_eq!(classify_char(&meta), "");

        let exe = dir.join("exe");
        fs::write(&exe, b"x").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        let meta = fs::symlink_metadata(&exe).unwrap();
        assert_eq!(classify_char(&meta), "*");

        let meta = fs::symlink_metadata(&dir).unwrap();
        assert_eq!(classify_char(&meta), "/");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn colorize_name_passthrough_when_color_disabled() {
        let dir = scratch_dir("colorize");
        let f = dir.join("f");
        fs::write(&f, b"x").unwrap();
        let meta = fs::symlink_metadata(&f).unwrap();
        let o = base_opts();
        assert_eq!(colorize_name("f", &meta, &o), "f");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn uid_name_root_is_root() {
        assert_eq!(uid_name(0), "root");
    }

    #[test]
    fn uid_name_unknown_uid_falls_back_to_number() {
        assert_eq!(uid_name(u32::MAX - 1), (u32::MAX - 1).to_string());
    }

    #[test]
    fn gid_name_unknown_gid_falls_back_to_number() {
        assert_eq!(gid_name(u32::MAX - 1), (u32::MAX - 1).to_string());
    }

    #[test]
    fn list_dir_empty_directory_succeeds() {
        let dir = scratch_dir("empty");
        let o = base_opts();
        assert!(list_dir(&mut Vec::new(), &dir, &o, false, false).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_dir_missing_path_errors() {
        let missing = PathBuf::from(format!("/nonexistent_user_ls_test_{}", std::process::id()));
        let o = base_opts();
        assert!(list_dir(&mut Vec::new(), &missing, &o, false, false).is_err());
    }

    #[test]
    fn list_dir_regular_directory_with_entries_succeeds() {
        let dir = scratch_dir("entries");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        fs::write(dir.join("b.txt"), b"b").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        let o = base_opts();
        assert!(list_dir(&mut Vec::new(), &dir, &o, false, false).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn time_style_names_map_to_gnu_formats() {
        assert_eq!(time_style("long-iso").unwrap().1, "%Y-%m-%d %H:%M");
        assert_eq!(time_style("iso").unwrap().0, "%Y-%m-%d ");
        assert_eq!(time_style("posix-iso"), time_style("locale"));
        assert_eq!(time_style("+%s\n%H").unwrap(), ("%s".into(), "%H".into()));
        assert!(time_style("bogus").is_none());
        assert!(time_style("posix-bogus").is_none());
    }

    #[test]
    fn strftime_local_expands_nanoseconds() {
        assert_eq!(strftime_local("%s.%N %%N", 0, 5), "0.000000005 %N");
    }

    #[test]
    fn run_args_rejects_bad_time_style() {
        assert_eq!(run_args(&["--time-style=bogus".to_string()]), 2);
        assert_eq!(run_args(&["--time-style".to_string()]), 2);
    }

    #[test]
    fn run_args_invalid_flag_returns_usage_error() {
        assert_eq!(run_args(&["-Y".to_string()]), 2);
    }

    #[test]
    fn run_args_missing_path_returns_failure() {
        let missing = format!("/nonexistent_user_ls_test_{}", std::process::id());
        assert_eq!(run_args(&[missing]), 2);
    }

    #[test]
    fn run_args_lists_explicit_directory_path() {
        // Uses an absolute scratch path rather than changing the process
        // cwd, so this stays safe under cargo's parallel test runner.
        let dir = scratch_dir("run_args");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        assert_eq!(run_args(&[dir.to_string_lossy().into_owned()]), 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
