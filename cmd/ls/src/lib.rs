//! user ls — list directory contents (Zainium coloured).

use colored::Colorize;
use std::fs::{self};
use std::io::{IsTerminal, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use std::io;
use usercore::Ui;

struct Opts {
    all: bool,
    almost_all: bool,
    long: bool,
    human: bool,
    reverse: bool,
    classify: bool,
    inode: bool,
    one: bool,
    directory: bool,
    recursive: bool,
    sort_time: bool,
    sort_size: bool,
    color: bool,
}

/// Entry point for the `ls` utility. Parses `std::env::args()` and lists
/// the named FILE(s), or the current directory if none are given.
///
/// Returns 0 on success, 1 if any path could not be accessed or on a
/// usage error.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_args(&args)
}

/// Parse and list with explicit argv (for dir/vdir multicall wrappers).
///
/// Returns 0 on success, 1 if any path could not be accessed or on a
/// usage error.
pub fn run_args(args: &[String]) -> i32 {
    let ui = Ui::new("ls");
    let mut o = Opts {
        all: false,
        almost_all: false,
        long: false,
        human: false,
        reverse: false,
        classify: false,
        inode: false,
        one: false,
        directory: false,
        recursive: false,
        sort_time: false,
        sort_size: false,
        color: io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    };
    let mut paths: Vec<PathBuf> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" if !o.human => {
                // conflict: -h is human in GNU when listing; --help always help
                if arg == "--help" {
                    print_help();
                    return 0;
                }
                o.human = true;
            }
            "--help" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("ls (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => o.all = true,
            "-A" | "--almost-all" => o.almost_all = true,
            "-l" => o.long = true,
            "-h" | "--human-readable" => o.human = true,
            "-r" | "--reverse" => o.reverse = true,
            "-F" | "--classify" => o.classify = true,
            "-i" | "--inode" => o.inode = true,
            "-1" => o.one = true,
            "-d" | "--directory" => o.directory = true,
            "-R" | "--recursive" => o.recursive = true,
            "-t" => o.sort_time = true,
            "-S" => o.sort_size = true,
            "--color" | "--color=always" => o.color = true,
            "--color=never" => o.color = false,
            "--color=auto" => o.color = io::stdout().is_terminal(),
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                for c in s.chars().skip(1) {
                    match c {
                        'a' => o.all = true,
                        'A' => o.almost_all = true,
                        'l' => o.long = true,
                        'h' => o.human = true,
                        'r' => o.reverse = true,
                        'F' => o.classify = true,
                        'i' => o.inode = true,
                        '1' => o.one = true,
                        'd' => o.directory = true,
                        'R' => o.recursive = true,
                        't' => o.sort_time = true,
                        'S' => o.sort_size = true,
                        _ => {
                            ui.err(&format!("invalid option -- '{c}'"));
                            return 1;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => paths.push(PathBuf::from(other)),
        }
    }
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }

    let mut status = 0;
    let multi = paths.len() > 1 || o.recursive;
    for (idx, p) in paths.iter().enumerate() {
        if let Err(e) = list_path(p, &o, multi, idx > 0) {
            ui.err(&format!("cannot access '{}': {e}", p.display()));
            status = 1;
        }
    }
    status
}

/// Print `ls --help` usage text to stdout.
fn print_help() {
    print!(
        "Usage: ls [OPTION]... [FILE]...\n\
 List information about the FILEs (the current directory by default).\n\n\
 -a, --all do not ignore entries starting with .\n\
 -A, --almost-all do not list implied . and ..\n\
 -d, --directory list directories themselves, not their contents\n\
 -F, --classify append indicator (one of */=>@|) to entries\n\
 -h, --human-readable with -l, print sizes like 1K 234M 2G etc.\n\
 -i, --inode print the index number of each file\n\
 -l use a long listing format\n\
 -r, --reverse reverse order while sorting\n\
 -R, --recursive list subdirectories recursively\n\
 -S sort by file size, largest first\n\
 -t sort by time, newest first\n\
 -1 list one file per line\n\
 --color[=WHEN] colorize the output; WHEN can be always/auto/never\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// List `path`: a single-entry line if it's a non-directory (or `-d` was
/// given), otherwise its directory contents via [`list_dir`].
fn list_path(path: &Path, o: &Opts, show_header: bool, blank_before: bool) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if o.directory || !meta.is_dir() {
        print_entry(path, path, &meta, o)?;
        return Ok(());
    }
    list_dir(path, o, show_header, blank_before)
}

/// List the contents of directory `path`: gather, filter (`-a`/`-A`),
/// sort (`-t`/`-S`/`-r`), and print each entry, then recurse into
/// subdirectories if `-R` was given.
fn list_dir(path: &Path, o: &Opts, show_header: bool, blank_before: bool) -> io::Result<()> {
    if blank_before {
        println!();
    }
    if show_header {
        println!("{}:", path.display());
    }

    let ui = Ui::new("ls");
    let mut entries: Vec<(PathBuf, String, fs::Metadata)> = Vec::new();
    for ent in fs::read_dir(path)? {
        let ent = ent?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if !o.all && !o.almost_all && name.starts_with('.') {
            continue;
        }
        if o.almost_all && (name == "." || name == "..") {
            continue;
        }
        // Use `symlink_metadata` directly (not `DirEntry::metadata`/
        // `fs::metadata`) so entries are typed by the link itself rather
        // than by whatever a symlink points at — required for `-l`'s `l`
        // file-type column and for `-R` to not recurse through symlinked
        // directories. A single stat per entry; on failure (e.g. the
        // entry was removed between `read_dir` yielding it and this
        // call), warn and skip just that entry rather than aborting the
        // whole directory listing.
        let meta = match fs::symlink_metadata(ent.path()) {
            Ok(m) => m,
            Err(e) => {
                ui.err(&format!("cannot access '{}': {e}", ent.path().display()));
                continue;
            }
        };
        entries.push((ent.path(), name, meta));
    }

    if o.all {
        // ensure . and .. present when -a
        for special in [".", ".."] {
            if !entries.iter().any(|(_, n, _)| n == special) {
                let p = path.join(special);
                if let Ok(m) = fs::symlink_metadata(&p) {
                    entries.push((p, special.to_string(), m));
                }
            }
        }
    }

    entries.sort_by(|a, b| {
        let ord = if o.sort_size {
            b.2.len().cmp(&a.2.len()).then_with(|| a.1.cmp(&b.1))
        } else if o.sort_time {
            let ta = a.2.modified().ok();
            let tb = b.2.modified().ok();
            tb.cmp(&ta).then_with(|| a.1.cmp(&b.1))
        } else {
            a.1.cmp(&b.1)
        };
        if o.reverse {
            ord.reverse()
        } else {
            ord
        }
    });

    if o.long {
        // total blocks
        let blocks: u64 = entries.iter().map(|(_, _, m)| m.blocks() as u64).sum();
        println!("total {}", blocks / 2); // 512-byte to 1K blocks like GNU
        for (p, name, meta) in &entries {
            print_long(p, name, meta, o)?;
        }
    } else {
        for (p, name, meta) in &entries {
            print_short(p, name, meta, o)?;
            if o.one || !o.color {
                println!();
            } else {
                print!(" ");
            }
        }
        if !o.one && o.color {
            println!();
        }
        // always newline if we used spaces
        if !o.one && !o.color && !entries.is_empty() {
            // already println each
        }
    }

    if o.recursive {
        for (p, name, meta) in &entries {
            // `meta` came from `symlink_metadata` above, so `is_dir()`
            // here is false for a symlink pointing at a directory —
            // recursion follows real subdirectories only, never
            // symlinks (avoids symlink loops).
            if meta.is_dir() && name != "." && name != ".." {
                println!();
                // A single unreadable subdirectory (e.g. permission
                // denied) must not abort the rest of the `-R` walk —
                // report it and keep listing sibling/later directories,
                // matching GNU `ls`'s best-effort recursive behavior.
                if let Err(e) = list_dir(p, o, true, false) {
                    ui.err(&format!("cannot access '{}': {e}", p.display()));
                }
            }
        }
    }
    Ok(())
}

/// Print a single non-directory path (or a directory named with `-d`) as
/// one entry: `-l` long form or the short/colorized form.
fn print_entry(full: &Path, display: &Path, meta: &fs::Metadata, o: &Opts) -> io::Result<()> {
    let name = display
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| display.display().to_string());
    if o.long {
        print_long(full, &name, meta, o)
    } else {
        print_short(full, &name, meta, o)?;
        println!();
        Ok(())
    }
}

/// Write one entry in short (non-`-l`) form: optional inode column,
/// colorized/plain name, optional `-F` classify suffix. No trailing
/// newline — the caller decides the separator (space-packed vs. one per
/// line).
fn print_short(_path: &Path, name: &str, meta: &fs::Metadata, o: &Opts) -> io::Result<()> {
    let mut out = io::stdout().lock();
    if o.inode {
        write!(out, "{:8} ", meta.ino())?;
    }
    let display = colorize_name(name, meta, o);
    let class = if o.classify { classify_char(meta) } else { "" };
    write!(out, "{display}{class}")?;
    Ok(())
}

/// Write one entry in `-l` long form: mode bits, link count, owner,
/// group, size, mtime, name (colorized/classified), and, for a symlink,
/// its `-> target` suffix.
fn print_long(path: &Path, name: &str, meta: &fs::Metadata, o: &Opts) -> io::Result<()> {
    let mut out = io::stdout().lock();
    if o.inode {
        write!(out, "{:8} ", meta.ino())?;
    }
    let mode = mode_string(meta);
    let nlink = meta.nlink();
    let uid = uid_name(meta.uid());
    let gid = gid_name(meta.gid());
    let size = if o.human {
        human_size(meta.len())
    } else {
        format!("{:>8}", meta.len())
    };
    let mtime = format_mtime(meta);
    let display = colorize_name(name, meta, o);
    let class = if o.classify { classify_char(meta) } else { "" };
    // symlink target
    let mut suffix = String::new();
    if meta.file_type().is_symlink() {
        if let Ok(t) = fs::read_link(path) {
            suffix = format!(" -> {}", t.display());
        }
    }
    writeln!(
        out,
        "{mode} {nlink:>3} {uid:<8} {gid:<8} {size:>8} {mtime} {display}{class}{suffix}"
    )?;
    Ok(())
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

/// Format a byte count as a human-readable size using binary (1024-based)
/// single-letter units (`-h`), e.g. `1536` -> `"   1.5K"`.
fn human_size(n: u64) -> String {
    const UNITS: [&str; 6] = ["", "K", "M", "G", "T", "P"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n:>7}")
    } else {
        format!("{v:>6.1}{}", UNITS[i])
    }
}

/// Format `meta`'s modification time as `-l`'s `"Mon DD HH:MM"` column,
/// converted to local time via `localtime_r(3)`.
fn format_mtime(meta: &fs::Metadata) -> String {
    use std::time::Duration;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let secs = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64;
    // `libc::tm` has no `Default` impl, but every field is a public primitive
    // (int/long/pointer), so we can build a zeroed value with a plain struct
    // literal instead of `mem::zeroed`, avoiding an unsafe block here.
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
    // SAFETY: `t` points to a valid, initialized `libc::time_t` on the stack and
    // `tm` points to a valid, initialized `libc::tm` on the stack that outlives
    // this call. `localtime_r` only reads through the first pointer and writes
    // through the second, both for the duration of the call only.
    unsafe {
        libc::localtime_r(&t, &mut tm);
    }
    format!(
        "{:3} {:2} {:02}:{:02}",
        MONTH[tm.tm_mon.clamp(0, 11) as usize],
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}

const MONTH: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Resolve `uid` to a user name via `getpwuid(3)`, falling back to the
/// numeric id (stringified) if no matching passwd entry exists.
fn uid_name(uid: u32) -> String {
    // SAFETY: `libc::getpwuid` takes a plain `uid_t` by value and cannot itself cause
    // UB. It returns either NULL or a pointer to a `passwd` struct backed by libc's
    // internal static/thread-local buffer, which stays valid until the next call to
    // `getpwuid`/`getpwnam` etc. on this thread. We check for NULL before
    // dereferencing, and `pw_name` is documented to be a valid NUL-terminated C
    // string for the lifetime of that buffer, so `CStr::from_ptr` is sound; the
    // string is copied into an owned `String` before any further libc call could
    // invalidate the buffer.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            uid.to_string()
        } else {
            std::ffi::CStr::from_ptr((*pw).pw_name)
                .to_string_lossy()
                .into_owned()
        }
    }
}

/// Resolve `gid` to a group name via `getgrgid(3)`, falling back to the
/// numeric id (stringified) if no matching group entry exists.
fn gid_name(gid: u32) -> String {
    // SAFETY: `libc::getgrgid` takes a plain `gid_t` by value and cannot itself cause
    // UB. It returns either NULL or a pointer to a `group` struct backed by libc's
    // internal static/thread-local buffer, which stays valid until the next call to
    // `getgrgid`/`getgrnam` etc. on this thread. We check for NULL before
    // dereferencing, and `gr_name` is documented to be a valid NUL-terminated C
    // string for the lifetime of that buffer, so `CStr::from_ptr` is sound; the
    // string is copied into an owned `String` before any further libc call could
    // invalidate the buffer.
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            gid.to_string()
        } else {
            std::ffi::CStr::from_ptr((*gr).gr_name)
                .to_string_lossy()
                .into_owned()
        }
    }
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
        Opts {
            all: false,
            almost_all: false,
            long: false,
            human: false,
            reverse: false,
            classify: false,
            inode: false,
            one: false,
            directory: false,
            recursive: false,
            sort_time: false,
            sort_size: false,
            color: false,
        }
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
        assert_eq!(human_size(0).trim(), "0");
        assert_eq!(human_size(1536).trim(), "1.5K");
        assert_eq!(human_size(1024 * 1024).trim(), "1.0M");
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
        assert!(list_dir(&dir, &o, false, false).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_path_missing_path_errors() {
        let missing =
            PathBuf::from(format!("/nonexistent_user_ls_test_{}", std::process::id()));
        let o = base_opts();
        assert!(list_path(&missing, &o, false, false).is_err());
    }

    #[test]
    fn list_dir_regular_directory_with_entries_succeeds() {
        let dir = scratch_dir("entries");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        fs::write(dir.join("b.txt"), b"b").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        let o = base_opts();
        assert!(list_dir(&dir, &o, false, false).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_args_invalid_flag_returns_usage_error() {
        assert_eq!(run_args(&["-Z".to_string()]), 1);
    }

    #[test]
    fn run_args_missing_path_returns_failure() {
        let missing = format!("/nonexistent_user_ls_test_{}", std::process::id());
        assert_eq!(run_args(&[missing]), 1);
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
