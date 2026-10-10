//! user tar — ustar/GNU/pax archives: create, extract, list, with -C and gzip/bzip2/xz/zstd/lz4.
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};

use usercore::Ui;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Comp {
    None,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Lz4,
}

impl Comp {
    fn tool(self) -> Option<&'static str> {
        match self {
            Comp::None => None,
            Comp::Gzip => Some("gzip"),
            Comp::Bzip2 => Some("bzip2"),
            Comp::Xz => Some("xz"),
            Comp::Zstd => Some("zstd"),
            Comp::Lz4 => Some("lz4"),
        }
    }

    fn from_name(p: &Path) -> Comp {
        let n = p.to_string_lossy();
        if n.ends_with(".gz") || n.ends_with(".tgz") {
            Comp::Gzip
        } else if n.ends_with(".bz2") || n.ends_with(".tbz") || n.ends_with(".tbz2") {
            Comp::Bzip2
        } else if n.ends_with(".xz") || n.ends_with(".txz") {
            Comp::Xz
        } else if n.ends_with(".zst") || n.ends_with(".tzst") {
            Comp::Zstd
        } else if n.ends_with(".lz4") {
            Comp::Lz4
        } else {
            Comp::None
        }
    }

    fn from_magic(b: &[u8]) -> Comp {
        if b.starts_with(&[0x1f, 0x8b]) {
            Comp::Gzip
        } else if b.starts_with(b"BZh") {
            Comp::Bzip2
        } else if b.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]) {
            Comp::Xz
        } else if b.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
            Comp::Zstd
        } else if b.starts_with(&[0x04, 0x22, 0x4d, 0x18]) {
            Comp::Lz4
        } else {
            Comp::None
        }
    }
}

#[derive(Default)]
struct Opts {
    verbose: bool,
    to_stdout: bool,
    keep_old: bool,
    strip: usize,
    exclude: Vec<String>,
    comp: Option<Comp>,
    auto_comp: bool,
}

#[derive(PartialEq)]
enum Mode {
    Create,
    Extract,
    List,
}

const HELP: &str = "Usage: tar {-c|-x|-t} [OPTION]... [-f ARCHIVE] [FILE]...\n  -c, -x, -t            create / extract / list\n  -f ARCHIVE            archive file ('-' for stdin/stdout)\n  -C DIR                change to DIR first\n  -v                    verbose\n  -z, -j, -J            gzip / bzip2 / xz\n      --zstd, --lz4     zstd / lz4 (compression is auto-detected on extract)\n  -a, --auto-compress   pick compression from the archive name\n      --strip-components=N  drop N leading path parts on extract\n      --exclude=GLOB    skip matching names\n  -O, --to-stdout       extract file contents to stdout\n  -k, --keep-old-files  don't replace existing files\n";

pub fn run() -> i32 {
    // a closed pipe (`| head`) ends the program quietly instead of panicking
    // SAFETY: restoring the default SIGPIPE action has no preconditions.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let ui = Ui::new("tar");
    let mut mode: Option<Mode> = None;
    let mut file: Option<PathBuf> = None;
    let mut o = Opts::default();
    // (directory to work in, path) for create; dir changes apply to the paths after them
    let mut items: Vec<(Option<PathBuf>, PathBuf)> = Vec::new();
    let mut cwd: Option<PathBuf> = None;
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // old style: `tar xzvf a.tar.gz` without a dash
    if let Some(first) = args.first() {
        if !first.starts_with('-') && !first.is_empty() && first.chars().all(|c| "ctxvfzjJaOkpC".contains(c)) {
            args[0] = format!("-{first}");
        }
    }

    let set_mode = |m: Mode, mode: &mut Option<Mode>| -> bool {
        if mode.as_ref().is_some_and(|x| *x != m) {
            return false;
        }
        *mode = Some(m);
        true
    };

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        macro_rules! value {
            ($n:expr) => {{
                i += 1;
                match args.get(i) {
                    Some(v) => v.clone(),
                    None => {
                        ui.err(&format!("option requires an argument -- '{}'", $n));
                        return 2;
                    }
                }
            }};
        }
        if let Some(long) = a.strip_prefix("--") {
            let (k, v) = match long.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (long, None),
            };
            let ok = match k {
                "help" => {
                    print!("{HELP}");
                    return 0;
                }
                "version" => {
                    println!("tar (user_utils) 0.2.0");
                    return 0;
                }
                "create" => set_mode(Mode::Create, &mut mode),
                "extract" | "get" => set_mode(Mode::Extract, &mut mode),
                "list" => set_mode(Mode::List, &mut mode),
                "verbose" => {
                    o.verbose = true;
                    true
                }
                "file" => {
                    file = Some(PathBuf::from(match v { Some(v) => v, None => value!("file") }));
                    true
                }
                "directory" => {
                    cwd = Some(PathBuf::from(match v { Some(v) => v, None => value!("directory") }));
                    true
                }
                "gzip" | "gunzip" => {
                    o.comp = Some(Comp::Gzip);
                    true
                }
                "bzip2" => {
                    o.comp = Some(Comp::Bzip2);
                    true
                }
                "xz" => {
                    o.comp = Some(Comp::Xz);
                    true
                }
                "zstd" => {
                    o.comp = Some(Comp::Zstd);
                    true
                }
                "lz4" => {
                    o.comp = Some(Comp::Lz4);
                    true
                }
                "auto-compress" => {
                    o.auto_comp = true;
                    true
                }
                "to-stdout" => {
                    o.to_stdout = true;
                    true
                }
                "keep-old-files" => {
                    o.keep_old = true;
                    true
                }
                "strip-components" => match match v { Some(v) => v, None => value!("strip-components") }.parse() {
                    Ok(n) => {
                        o.strip = n;
                        true
                    }
                    Err(_) => false,
                },
                "exclude" => {
                    o.exclude.push(match v { Some(v) => v, None => value!("exclude") });
                    true
                }
                "preserve-permissions" | "same-permissions" | "no-same-owner" | "same-owner" | "overwrite"
                | "numeric-owner" | "no-same-permissions" | "warning" => true,
                _ => false,
            };
            if !ok {
                ui.err(&format!("unrecognized option '{a}'"));
                return 2;
            }
            i += 1;
            continue;
        }
        if a.starts_with('-') && a.len() > 1 {
            let chars: Vec<char> = a[1..].chars().collect();
            let mut k = 0;
            while k < chars.len() {
                let c = chars[k];
                let rest: String = chars[k + 1..].iter().collect();
                match c {
                    'c' | 'x' | 't' => {
                        let m = match c {
                            'c' => Mode::Create,
                            'x' => Mode::Extract,
                            _ => Mode::List,
                        };
                        if !set_mode(m, &mut mode) {
                            ui.err("You may not specify more than one '-Acdtrux' option");
                            return 2;
                        }
                    }
                    'v' => o.verbose = true,
                    'z' => o.comp = Some(Comp::Gzip),
                    'j' => o.comp = Some(Comp::Bzip2),
                    'J' => o.comp = Some(Comp::Xz),
                    'a' => o.auto_comp = true,
                    'O' => o.to_stdout = true,
                    'k' => o.keep_old = true,
                    'p' | 'o' => {}
                    'h' => {
                        print!("{HELP}");
                        return 0;
                    }
                    'f' | 'C' => {
                        let v = if rest.is_empty() { value!(c) } else { rest };
                        if c == 'f' {
                            file = Some(PathBuf::from(v));
                        } else {
                            cwd = Some(PathBuf::from(v));
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
            i += 1;
            continue;
        }
        items.push((cwd.clone(), PathBuf::from(&a)));
        i += 1;
    }

    let Some(mode) = mode else {
        ui.err("You must specify one of the '-Acdtrux' options");
        return 2;
    };
    let archive = file.unwrap_or_else(|| PathBuf::from("-"));
    // the archive is named relative to where tar started, before any -C
    let archive = if archive.as_os_str() == "-" || archive.is_absolute() {
        archive
    } else {
        std::env::current_dir().map(|d| d.join(&archive)).unwrap_or(archive)
    };
    if o.comp.is_none() && (o.auto_comp || mode == Mode::Create) && archive.as_os_str() != "-" {
        let guess = Comp::from_name(&archive);
        if o.auto_comp || guess != Comp::None {
            o.comp = Some(guess);
        }
    }

    let result = match mode {
        Mode::Create => {
            if items.is_empty() {
                ui.err("Cowardly refusing to create an empty archive");
                return 2;
            }
            create(&archive, &items, &o)
        }
        Mode::Extract | Mode::List => {
            // -C for extract applies to the whole archive; names after it filter members
            let filters: Vec<PathBuf> = items.iter().map(|(_, p)| p.clone()).collect();
            if mode == Mode::Extract {
                if let Some(d) = &cwd {
                    if let Err(e) = std::env::set_current_dir(d) {
                        ui.err(&format!("{}: Cannot open: {e}", d.display()));
                        return 2;
                    }
                }
            }
            read_archive(&archive, &o, mode == Mode::List, &filters)
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&e.to_string());
            2
        }
    }
}

fn tool_path(tool: &str) -> io::Result<String> {
    for d in ["/overlayer/syshub/bin", "/overlayer/zexlib/union/bin", "/usr/bin", "/bin"] {
        let p = format!("{d}/{tool}");
        if Path::new(&p).exists() {
            return Ok(p);
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, format!("{tool} is not installed (needed for this archive)")))
}

// ── create ────────────────────────────────────────────────────────────────────

fn create(archive: &Path, items: &[(Option<PathBuf>, PathBuf)], o: &Opts) -> io::Result<()> {
    let comp = o.comp.unwrap_or(Comp::None);
    let start_dir = std::env::current_dir()?;
    let abs_archive = if archive.as_os_str() == "-" { None } else { Some(start_dir.join(archive)) };
    let sink: Box<dyn Write> = match &abs_archive {
        None => Box::new(io::stdout()),
        Some(p) => Box::new(File::create(p)?),
    };
    let mut child: Option<Child> = None;
    let mut out: Box<dyn Write> = match comp.tool() {
        None => sink,
        Some(t) => {
            let stdout: Stdio = match &abs_archive {
                None => Stdio::inherit(),
                Some(p) => Stdio::from(File::create(p)?),
            };
            let mut c = Command::new(tool_path(t)?).arg("-c").stdin(Stdio::piped()).stdout(stdout).spawn()?;
            let w = c.stdin.take().expect("piped stdin");
            child = Some(c);
            Box::new(w)
        }
    };
    let mut buf = io::BufWriter::new(&mut out);
    for (dir, p) in items {
        std::env::set_current_dir(&start_dir)?;
        if let Some(d) = dir {
            std::env::set_current_dir(d)?;
        }
        add_path(&mut buf, p, o)?;
    }
    std::env::set_current_dir(&start_dir)?;
    buf.write_all(&[0u8; 1024])?;
    buf.flush()?;
    drop(buf);
    drop(out);
    if let Some(mut c) = child {
        let st = c.wait()?;
        if !st.success() {
            return Err(io::Error::other(format!("compressor exited with {st}")));
        }
    }
    Ok(())
}

fn excluded(name: &str, o: &Opts) -> bool {
    let base = name.trim_end_matches('/').rsplit('/').next().unwrap_or(name);
    o.exclude.iter().any(|g| glob(g, base) || glob(g, name))
}

fn glob(p: &str, n: &str) -> bool {
    fn m(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    m(&p.chars().collect::<Vec<_>>(), &n.chars().collect::<Vec<_>>())
}

fn add_path(out: &mut dyn Write, path: &Path, o: &Opts) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let mut name = path.to_string_lossy().trim_start_matches('/').to_string();
    while let Some(r) = name.strip_prefix("./") {
        name = r.to_string();
    }
    if name.is_empty() {
        name = ".".into();
    }
    if excluded(&name, o) {
        return Ok(());
    }
    let ft = meta.file_type();
    let mut h = Header::from_meta(&meta);
    if ft.is_dir() {
        h.typ = b'5';
        h.size = 0;
        let dname = if name.ends_with('/') { name.clone() } else { format!("{name}/") };
        write_entry(out, &dname, &h, "")?;
        if o.verbose {
            eprintln!("{dname}");
        }
        let mut ents: Vec<_> = fs::read_dir(path)?.filter_map(|e| e.ok()).collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            add_path(out, &e.path(), o)?;
        }
    } else if ft.is_symlink() {
        h.typ = b'2';
        h.size = 0;
        let target = fs::read_link(path)?.to_string_lossy().into_owned();
        write_entry(out, &name, &h, &target)?;
        if o.verbose {
            eprintln!("{name}");
        }
    } else if ft.is_file() {
        h.typ = b'0';
        write_entry(out, &name, &h, "")?;
        let mut f = File::open(path)?;
        let copied = io::copy(&mut Read::by_ref(&mut f).take(h.size), out)?;
        if copied < h.size {
            out.write_all(&vec![0u8; (h.size - copied) as usize])?;
        }
        pad(out, h.size)?;
        if o.verbose {
            eprintln!("{name}");
        }
    } else {
        eprintln!("tar: {name}: socket/device ignored");
    }
    Ok(())
}

fn pad(out: &mut dyn Write, size: u64) -> io::Result<()> {
    let p = ((512 - size % 512) % 512) as usize;
    if p > 0 {
        out.write_all(&vec![0u8; p])?;
    }
    Ok(())
}

struct Header {
    mode: u32,
    uid: u32,
    gid: u32,
    size: u64,
    mtime: u64,
    typ: u8,
    uname: String,
    gname: String,
}

impl Header {
    fn from_meta(m: &fs::Metadata) -> Header {
        Header {
            mode: m.mode() & 0o7777,
            uid: m.uid(),
            gid: m.gid(),
            size: m.len(),
            mtime: m.mtime().max(0) as u64,
            typ: b'0',
            uname: usercore::nss::user_by_uid(m.uid()).map(|u| u.name).unwrap_or_default(),
            gname: usercore::nss::group_by_gid(m.gid()).map(|g| g.name).unwrap_or_default(),
        }
    }
}

/// Header for `name` (with a GNU long-name / long-link record first when the ustar fields can't hold it).
fn write_entry(out: &mut dyn Write, name: &str, h: &Header, link: &str) -> io::Result<()> {
    let (prefix, short) = split_name(name);
    if short.is_none() {
        let data = format!("{name}\0");
        let lh = Header { mode: 0o644, uid: 0, gid: 0, size: data.len() as u64, mtime: 0, typ: b'L', uname: String::new(), gname: String::new() };
        out.write_all(&block("././@LongLink", "", &lh, ""))?;
        out.write_all(data.as_bytes())?;
        pad(out, data.len() as u64)?;
    }
    if link.len() > 100 {
        let data = format!("{link}\0");
        let lh = Header { mode: 0o644, uid: 0, gid: 0, size: data.len() as u64, mtime: 0, typ: b'K', uname: String::new(), gname: String::new() };
        out.write_all(&block("././@LongLink", "", &lh, ""))?;
        out.write_all(data.as_bytes())?;
        pad(out, data.len() as u64)?;
    }
    let short_name = short.unwrap_or_else(|| name.chars().take(100).collect());
    out.write_all(&block(&short_name, prefix.as_deref().unwrap_or(""), h, link))
}

/// ustar split: name ≤ 100 bytes, or prefix ≤ 155 + "/" + name ≤ 100.
fn split_name(name: &str) -> (Option<String>, Option<String>) {
    if name.len() <= 100 {
        return (None, Some(name.to_string()));
    }
    let bytes = name.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'/' && i <= 155 && bytes.len() - i - 1 <= 100 && i > 0 && bytes.len() - i - 1 > 0 {
            return (Some(name[..i].to_string()), Some(name[i + 1..].to_string()));
        }
    }
    (None, None)
}

fn block(name: &str, prefix: &str, h: &Header, link: &str) -> [u8; 512] {
    let mut b = [0u8; 512];
    put(&mut b[0..100], name.as_bytes());
    put(&mut b[100..108], format!("{:07o}\0", h.mode).as_bytes());
    put(&mut b[108..116], format!("{:07o}\0", h.uid.min(0o7777777)).as_bytes());
    put(&mut b[116..124], format!("{:07o}\0", h.gid.min(0o7777777)).as_bytes());
    put(&mut b[124..136], format!("{:011o}\0", h.size).as_bytes());
    put(&mut b[136..148], format!("{:011o}\0", h.mtime).as_bytes());
    b[156] = h.typ;
    put(&mut b[157..257], link.as_bytes());
    put(&mut b[257..263], b"ustar\0");
    put(&mut b[263..265], b"00");
    put(&mut b[265..297], h.uname.as_bytes());
    put(&mut b[297..329], h.gname.as_bytes());
    put(&mut b[345..500], prefix.as_bytes());
    b[148..156].copy_from_slice(b"        ");
    let sum: u32 = b.iter().map(|&x| x as u32).sum();
    put(&mut b[148..156], format!("{sum:06o}\0 ").as_bytes());
    b
}

fn put(dst: &mut [u8], src: &[u8]) {
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);
}

// ── extract / list ────────────────────────────────────────────────────────────

/// Reader that first hands back bytes already peeked at.
struct Peeked<R: Read + Send> {
    head: Vec<u8>,
    pos: usize,
    inner: R,
}

impl<R: Read + Send> Read for Peeked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos < self.head.len() {
            let n = (self.head.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.head[self.pos..self.pos + n]);
            self.pos += n;
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

fn open_input(archive: &Path, o: &Opts) -> io::Result<(Box<dyn Read>, Option<Child>)> {
    let mut raw: Box<dyn Read + Send> = if archive.as_os_str() == "-" { Box::new(io::stdin()) } else { Box::new(File::open(archive)?) };
    let mut head = vec![0u8; 6];
    let mut got = 0;
    while got < head.len() {
        let n = raw.read(&mut head[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    head.truncate(got);
    let comp = match o.comp {
        Some(c) if c != Comp::None => c,
        _ => Comp::from_magic(&head),
    };
    let input = Peeked { head, pos: 0, inner: raw };
    match comp.tool() {
        None => Ok((Box::new(input), None)),
        Some(t) => {
            let mut c = Command::new(tool_path(t)?).arg("-dc").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
            let mut w = c.stdin.take().expect("piped stdin");
            let mut feed = input;
            std::thread::spawn(move || {
                let _ = io::copy(&mut feed, &mut w);
            });
            let r = c.stdout.take().expect("piped stdout");
            Ok((Box::new(r), Some(c)))
        }
    }
}

fn read_full(r: &mut dyn Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut got = 0;
    while got < buf.len() {
        let n = r.read(&mut buf[got..])?;
        if n == 0 {
            if got == 0 {
                return Ok(false);
            }
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected end of archive"));
        }
        got += n;
    }
    Ok(true)
}

fn read_data(r: &mut dyn Read, size: u64) -> io::Result<Vec<u8>> {
    let mut v = vec![0u8; size as usize];
    if size > 0 && !read_full(r, &mut v)? {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected end of archive"));
    }
    skip(r, (512 - size % 512) % 512)?;
    Ok(v)
}

fn skip(r: &mut dyn Read, n: u64) -> io::Result<()> {
    let copied = io::copy(&mut r.take(n), &mut io::sink())?;
    if copied < n {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected end of archive"));
    }
    Ok(())
}

fn pax_value(data: &[u8], key: &str) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    let mut rest: &str = &text;
    while !rest.is_empty() {
        let (len_s, _) = rest.split_once(' ')?;
        let len: usize = len_s.parse().ok()?;
        let rec = rest.get(..len)?;
        let kv = rec.split_once(' ')?.1.trim_end_matches('\n');
        if let Some((k, v)) = kv.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
        rest = &rest[len..];
    }
    None
}

fn read_archive(archive: &Path, o: &Opts, list: bool, filters: &[PathBuf]) -> io::Result<()> {
    let (mut input, child) = open_input(archive, o)?;
    let root = unsafe { libc::geteuid() } == 0;
    let mut long_name: Option<String> = None;
    let mut long_link: Option<String> = None;
    let mut dir_times: Vec<(PathBuf, u64, u32)> = Vec::new();
    let mut zeros = 0;
    loop {
        let mut hdr = [0u8; 512];
        if !read_full(&mut input, &mut hdr)? {
            break;
        }
        if hdr.iter().all(|&b| b == 0) {
            zeros += 1;
            if zeros == 2 {
                break;
            }
            continue;
        }
        zeros = 0;
        let stored: u32 = oct(&hdr[148..156])? as u32;
        let mut h2 = hdr;
        h2[148..156].copy_from_slice(b"        ");
        if h2.iter().map(|&b| b as u32).sum::<u32>() != stored {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "corrupt archive: header checksum mismatch"));
        }
        let size = oct(&hdr[124..136])?;
        let typ = hdr[156];
        match typ {
            b'L' => {
                long_name = Some(cstr(&read_data(&mut input, size)?));
                continue;
            }
            b'K' => {
                long_link = Some(cstr(&read_data(&mut input, size)?));
                continue;
            }
            b'x' => {
                let d = read_data(&mut input, size)?;
                if let Some(p) = pax_value(&d, "path") {
                    long_name = Some(p);
                }
                if let Some(l) = pax_value(&d, "linkpath") {
                    long_link = Some(l);
                }
                continue;
            }
            b'g' => {
                read_data(&mut input, size)?;
                continue;
            }
            _ => {}
        }
        let name = long_name.take().unwrap_or_else(|| {
            let prefix = cstr(&hdr[345..500]);
            let n = cstr(&hdr[0..100]);
            if &hdr[257..262] == b"ustar" && !prefix.is_empty() { format!("{prefix}/{n}") } else { n }
        });
        let link = long_link.take().unwrap_or_else(|| cstr(&hdr[157..257]));
        let mode = oct(&hdr[100..108])? as u32 & 0o7777;
        let mtime = oct(&hdr[136..148]).unwrap_or(0);
        let uid = oct(&hdr[108..116]).unwrap_or(0) as u32;
        let gid = oct(&hdr[116..124]).unwrap_or(0) as u32;
        let has_data = matches!(typ, b'0' | 0 | b'7');
        let data_len = if has_data { size } else if matches!(typ, b'1' | b'2' | b'3' | b'4' | b'5' | b'6') { 0 } else { size };

        let wanted = filters.is_empty()
            || filters.iter().any(|f| {
                let f = f.to_string_lossy();
                let f = f.trim_end_matches('/');
                name == f || name.starts_with(&format!("{f}/"))
            });
        if !wanted || excluded(&name, o) {
            skip(&mut input, data_len.div_ceil(512) * 512)?;
            continue;
        }

        if list {
            if o.verbose {
                let t = match typ {
                    b'5' => 'd',
                    b'2' => 'l',
                    b'1' => 'h',
                    _ => '-',
                };
                let mut line = format!("{t}{} {}/{} {:>9} {}", perms(mode), owner(&hdr[265..297], uid), owner(&hdr[297..329], gid), size, name);
                if typ == b'2' {
                    line += &format!(" -> {link}");
                } else if typ == b'1' {
                    line += &format!(" link to {link}");
                }
                println!("{line}");
            } else {
                println!("{name}");
            }
            skip(&mut input, data_len.div_ceil(512) * 512)?;
            continue;
        }

        let Some(path) = safe_path(&name, o.strip) else {
            skip(&mut input, data_len.div_ceil(512) * 512)?;
            continue;
        };
        if o.verbose {
            eprintln!("{name}");
        }
        if o.to_stdout {
            if has_data {
                let mut so = io::stdout();
                io::copy(&mut input.by_ref().take(size), &mut so)?;
                skip(&mut input, (512 - size % 512) % 512)?;
            } else {
                skip(&mut input, data_len.div_ceil(512) * 512)?;
            }
            continue;
        }
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let exists = fs::symlink_metadata(&path).is_ok();
        if o.keep_old && exists && typ != b'5' {
            eprintln!("tar: {}: Cannot open: File exists", path.display());
            skip(&mut input, data_len.div_ceil(512) * 512)?;
            continue;
        }
        let mode = if root { mode } else { mode & 0o1777 };
        match typ {
            b'5' => {
                fs::create_dir_all(&path)?;
                dir_times.push((path.clone(), mtime, mode));
            }
            b'2' => {
                if exists {
                    let _ = fs::remove_file(&path);
                }
                std::os::unix::fs::symlink(&link, &path)?;
            }
            b'1' => {
                let Some(target) = safe_path(&link, o.strip) else { continue };
                if exists {
                    let _ = fs::remove_file(&path);
                }
                fs::hard_link(target, &path)?;
            }
            b'3' | b'4' | b'6' => {
                eprintln!("tar: {name}: device/fifo skipped");
            }
            _ => {
                if exists && fs::symlink_metadata(&path)?.is_dir() {
                    return Err(io::Error::other(format!("{name}: Cannot replace a directory with a file")));
                }
                if exists {
                    let _ = fs::remove_file(&path);
                }
                let mut f = File::create(&path)?;
                let copied = io::copy(&mut input.by_ref().take(size), &mut f)?;
                if copied < size {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected end of archive"));
                }
                skip(&mut input, (512 - size % 512) % 512)?;
                drop(f);
                fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
                set_mtime(&path, mtime, false);
            }
        }
        if root && typ != b'5' {
            let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok();
            if let Some(c) = c {
                // SAFETY: c is a valid NUL-terminated path.
                unsafe { libc::lchown(c.as_ptr(), uid, gid) };
            }
        }
        if typ == b'2' {
            set_mtime(&path, mtime, true);
        }
    }
    // directories last, so files written inside don't bump their times
    for (d, t, m) in dir_times.into_iter().rev() {
        let _ = fs::set_permissions(&d, fs::Permissions::from_mode(m | 0o700 & m));
        set_mtime(&d, t, false);
    }
    drop(input);
    if let Some(mut c) = child {
        let st = c.wait()?;
        if !st.success() {
            return Err(io::Error::other(format!("decompressor exited with {st}")));
        }
    }
    Ok(())
}

/// Leading `/` is dropped (like GNU tar); `..` anywhere is refused.
fn safe_path(name: &str, strip: usize) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        eprintln!("tar: skipping unsafe path {name}");
        return None;
    }
    let parts: Vec<_> = p.components().filter(|c| matches!(c, Component::Normal(_))).collect();
    if parts.len() <= strip {
        return None;
    }
    Some(parts[strip..].iter().collect())
}

fn set_mtime(p: &Path, secs: u64, nofollow: bool) {
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else { return };
    let t = libc::timespec { tv_sec: secs as _, tv_nsec: 0 };
    let times = [t, t];
    let flags = if nofollow { libc::AT_SYMLINK_NOFOLLOW } else { 0 };
    // SAFETY: valid path and two timespecs as utimensat expects.
    unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), flags) };
}

fn perms(m: u32) -> String {
    let mut s = String::new();
    for (i, c) in "rwxrwxrwx".chars().enumerate() {
        s.push(if m & (0o400 >> i) != 0 { c } else { '-' });
    }
    s
}

fn owner(field: &[u8], id: u32) -> String {
    let n = cstr(field);
    if n.is_empty() { id.to_string() } else { n }
}

/// Decode a NUL/space-terminated (or full-width) string field from a ustar header block.
fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// Decode an octal-ASCII numeric field; a malformed field is an error, never silently zero.
fn oct(b: &[u8]) -> io::Result<u64> {
    if b.first().is_some_and(|&c| c & 0x80 != 0) {
        // GNU base-256 for large values
        let mut v: u64 = (b[0] & 0x7f) as u64;
        for &c in &b[1..] {
            v = v.checked_mul(256).and_then(|v| v.checked_add(c as u64)).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "corrupt archive: numeric field overflow")
            })?;
        }
        return Ok(v);
    }
    let s = cstr(b);
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 8).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, format!("corrupt archive: invalid octal field {trimmed:?}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oct_parses_valid_field() {
        assert_eq!(oct(b"00000000644\0").unwrap(), 0o644);
    }

    #[test]
    fn oct_empty_field_is_zero() {
        assert_eq!(oct(b"\0\0\0\0").unwrap(), 0);
        assert_eq!(oct(b"           \0").unwrap(), 0);
    }

    #[test]
    fn oct_rejects_corrupt_field_instead_of_defaulting() {
        assert!(oct(b"not-octal!!\0").is_err());
        assert!(oct(b"999999999999\0").is_err());
    }

    #[test]
    fn cstr_stops_at_first_nul() {
        assert_eq!(cstr(b"hello\0world"), "hello");
        assert_eq!(cstr(b"nopadding"), "nopadding");
    }

    #[test]
    fn long_names_split_or_need_gnu_record() {
        let n = format!("{}/{}", "d".repeat(120), "f".repeat(50));
        let (p, s) = split_name(&n);
        assert_eq!(p.unwrap().len(), 120);
        assert_eq!(s.unwrap().len(), 50);
        assert_eq!(split_name(&"x".repeat(300)), (None, None));
    }

    #[test]
    fn safe_paths() {
        assert_eq!(safe_path("/etc/x", 0), Some(PathBuf::from("etc/x")));
        assert_eq!(safe_path("a/../b", 0), None);
        assert_eq!(safe_path("pkg-1.0/src/x", 1), Some(PathBuf::from("src/x")));
    }

    #[test]
    fn round_trip_keeps_modes_and_long_names() {
        let root = std::env::temp_dir().join(format!("user_tar_test_{}_rt", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let deep = root.join("src").join("a".repeat(90)).join("b".repeat(90));
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("tool.sh"), b"#!/bin/sh\n").unwrap();
        fs::set_permissions(deep.join("tool.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("tool.sh", deep.join("link")).unwrap();
        let archive = root.join("out.tar");
        let o = Opts::default();
        create(&archive, &[(Some(root.clone()), PathBuf::from("src"))], &o).unwrap();

        let out = root.join("x");
        fs::create_dir_all(&out).unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(&out).unwrap();
        let r = read_archive(&archive, &o, false, &[]);
        std::env::set_current_dir(orig).unwrap();
        r.unwrap();
        let got = out.join("src").join("a".repeat(90)).join("b".repeat(90));
        assert_eq!(fs::read(got.join("tool.sh")).unwrap(), b"#!/bin/sh\n");
        assert_eq!(fs::metadata(got.join("tool.sh")).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::read_link(got.join("link")).unwrap(), PathBuf::from("tool.sh"));
        let _ = fs::remove_dir_all(&root);
    }
}
