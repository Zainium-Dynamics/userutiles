//! user lslocks — list local file locks held (or waited on) by processes on
//! this system, by cross-referencing `/proc/locks` with each process's
//! `/proc/<pid>/fdinfo/<fd>` `lock:` lines.
mod render;

use std::fs;
use std::path::Path;

use usercore::Ui;

use render::{render_column_reference, render_locks, OutputMode, ALL_COLUMNS, DEFAULT_COLUMNS};

const HELP: &str = "Usage: lslocks [options]\n\
List local system locks.\n\n\
  -b, --bytes           print SIZE in bytes rather than human-readable\n\
  -i, --noinaccessible  ignore locks without read permissions\n\
  -J, --json            use JSON output format\n\
  -H, --list-columns    list the available columns\n\
  -n, --noheadings      don't print headings\n\
  -o, --output <list>   output columns (see --list-columns)\n\
      --output-all      output all columns\n\
  -p, --pid <pid>       display only locks held by this process\n\
  -u, --notruncate      don't truncate text in columns\n\
  -r, --raw             use the raw output format\n\
  -h, --help            display this help and exit\n\
      --version         output version information and exit\n";

#[derive(Default)]
struct Options {
    bytes: bool,
    no_inaccessible: bool,
    json: bool,
    raw: bool,
    noheadings: bool,
    output: Option<String>,
    output_all: bool,
    pid: Option<i32>,
    notruncate: bool,
    list_columns: bool,
}

/// Entry point for the `lslocks` utility.
pub fn run() -> i32 {
    let ui = Ui::new("lslocks");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut opts = Options::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("lslocks (user_utils) 0.1.0");
                return 0;
            }
            "-b" | "--bytes" => opts.bytes = true,
            "-i" | "--noinaccessible" => opts.no_inaccessible = true,
            "-J" | "--json" => opts.json = true,
            "-r" | "--raw" => opts.raw = true,
            "-n" | "--noheadings" => opts.noheadings = true,
            "-u" | "--notruncate" => opts.notruncate = true,
            "-H" | "--list-columns" => opts.list_columns = true,
            "--output-all" => opts.output_all = true,
            "-o" | "--output" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                opts.output = Some(value.clone());
            }
            "-p" | "--pid" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                match value.parse::<i32>() {
                    Ok(pid) => opts.pid = Some(pid),
                    Err(_) => {
                        ui.err(&format!("invalid pid argument: '{value}'"));
                        return 1;
                    }
                }
            }
            // Combined short options, e.g. `-bi`.
            s if s.starts_with('-') && !s.starts_with("--") && s.len() > 1 => {
                for c in s[1..].chars() {
                    match c {
                        'b' => opts.bytes = true,
                        'i' => opts.no_inaccessible = true,
                        'J' => opts.json = true,
                        'r' => opts.raw = true,
                        'n' => opts.noheadings = true,
                        'u' => opts.notruncate = true,
                        'H' => opts.list_columns = true,
                        other => {
                            ui.err(&format!("invalid option -- '{other}'"));
                            return 1;
                        }
                    }
                }
            }
            other => {
                ui.err(&format!("unknown option -- '{other}'"));
                return 1;
            }
        }
        i += 1;
    }

    if opts.json && opts.raw {
        ui.err("the options -J/--json and -r/--raw are mutually exclusive");
        return 1;
    }
    let output_mode = if opts.json {
        OutputMode::Json
    } else if opts.raw {
        OutputMode::Raw
    } else {
        OutputMode::Text
    };

    if opts.list_columns {
        render_column_reference(output_mode);
        return 0;
    }

    let columns = match resolve_columns(opts.output.as_deref(), opts.output_all) {
        Ok(c) => c,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    let pid_locks = match collect_pid_locks(opts.no_inaccessible) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };
    let mut proc_locks = match collect_proc_locks(opts.no_inaccessible, &pid_locks) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    if let Some(target) = opts.pid {
        proc_locks.retain(|l| l.process_id == target);
    }

    render_locks(
        output_mode,
        &columns,
        opts.bytes,
        opts.noheadings,
        opts.notruncate,
        &proc_locks,
        &pid_locks,
    );

    0
}

/// One parsed lock, from either the top-level `/proc/locks` table or a
/// single process's `/proc/<pid>/fdinfo/<fd>` `lock:` line.
#[derive(Clone)]
pub(crate) struct Lock {
    /// The numeric id `/proc/locks` groups a lock and its waiters under
    /// (`"<id>: ..."`); `-1` for locks sourced from `fdinfo` (which have no
    /// id of their own — they're only used for cross-referencing/HOLDERS).
    id: i64,
    blocked: bool,
    kind: String,
    mandatory: bool,
    mode: String,
    process_id: i32,
    major: u32,
    minor: u32,
    inode: u64,
    start: u64,
    end: u64,
    command_name: Option<String>,
    path: Option<String>,
    size: Option<u64>,
    file_descriptor: i32,
}

/// Resolves the effective output column list from `-o`/`--output` and
/// `--output-all`, matching real `lslocks(1)` semantics:
/// - neither given: `DEFAULT_COLUMNS`, or `ALL_COLUMNS` if `--output-all`.
/// - `-o LIST` (no leading `+`): exactly `LIST`, replacing the default set
///   entirely (and ignoring `--output-all`).
/// - `-o +LIST`: the default (or all, if `--output-all`) columns with
///   `LIST` appended.
fn resolve_columns(output: Option<&str>, output_all: bool) -> Result<Vec<&'static str>, String> {
    let base = || -> Vec<&'static str> {
        if output_all {
            ALL_COLUMNS.to_vec()
        } else {
            DEFAULT_COLUMNS.to_vec()
        }
    };

    let Some(spec) = output else {
        return Ok(base());
    };

    let (append, list_str) = match spec.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, spec),
    };

    let mut list = Vec::new();
    for name in list_str.split(',') {
        let found = ALL_COLUMNS
            .iter()
            .find(|&&c| c == name)
            .ok_or_else(|| format!("unknown column: {name}"))?;
        list.push(*found);
    }
    if list.is_empty() {
        return Err(format!("unknown column: {spec}"));
    }

    if append {
        let mut columns = base();
        columns.extend(list);
        Ok(columns)
    } else {
        Ok(list)
    }
}

/// Reads `/proc/<pid>/comm`, trimmed, or `None` if it can't be read.
fn read_comm(pid: i32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
}

/// Finds the path and size of the open file in process `pid` whose inode is
/// `inode`, by scanning `/proc/<pid>/fd/*` for a symlink resolving to it.
/// This is how a bare `(device, inode)` pair from `/proc/locks` gets turned
/// into a human-readable path: the lock itself carries no path, only the
/// identity of the locked file.
fn find_fd_path_and_size(pid: i32, inode: u64) -> Option<(String, u64)> {
    let fd_dir = Path::new("/proc").join(pid.to_string()).join("fd");
    for entry in fs::read_dir(fd_dir).ok()?.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.st_ino_matches(inode) {
            let target = fs::read_link(&path).ok()?;
            return Some((target.to_string_lossy().into_owned(), meta.len()));
        }
    }
    None
}

/// Thin adapter so [`find_fd_path_and_size`] doesn't need to name the
/// platform-specific `MetadataExt` trait at every call site.
trait InodeEq {
    fn st_ino_matches(&self, inode: u64) -> bool;
}
impl InodeEq for fs::Metadata {
    fn st_ino_matches(&self, inode: u64) -> bool {
        use std::os::linux::fs::MetadataExt;
        self.st_ino() == inode
    }
}

/// When a lock's file can't be found open in its owning process (the
/// process may have already closed the fd, or the lock belongs to a remote
/// NFS client with no local process at all), falls back to reporting just
/// the filesystem's mount point the device belongs to, e.g. `/run...`,
/// matching real `lslocks(1)`'s behavior for inaccessible/unresolvable
/// locks. Found by scanning `/proc/self/mountinfo` for an entry whose
/// `major:minor` matches; the last matching line is preferred (mirrors
/// searching mount history most-recent-first).
fn fallback_file_name(major: u32, minor: u32) -> Option<String> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").ok()?;
    let want = format!("{major}:{minor}");
    let mut found: Option<&str> = None;
    for line in mountinfo.lines() {
        let mut fields = line.split_whitespace();
        let Some(dev) = fields.nth(2) else { continue };
        if dev != want {
            continue;
        }
        // Field 4 (`root`, already consumed by `nth`) precedes field 5
        // (`mount point`, what we actually want) — skip one more.
        if let Some(mount_point) = fields.nth(1) {
            found = Some(mount_point);
        }
    }
    // Matches the real `lslocks(1)` binary's fallback format exactly
    // (empirically verified): the mount point with a literal `...`
    // appended directly, no separator inserted even when the mount point
    // isn't `/`.
    Some(format!("{found}...", found = found?))
}

/// Parses one lock record, either from a top-level `/proc/locks` line
/// (`fd_source: None`) or from a single fdinfo `lock:` line already
/// associated with a known `(pid, comm, fd)` (`fd_source: Some(..)`).
///
/// `pid_locks_for_xref`, when given, is the already-fully-built set of
/// per-process fdinfo locks; it's used to recover the true owning process
/// and command name for a `/proc/locks` entry when the naive pid field
/// there doesn't resolve locally (e.g. differing PID namespace, or an NFS
/// server reporting a remote client's lock) — matching real `lslocks(1)`.
fn parse_and_resolve(
    no_inaccessible: bool,
    line: &str,
    fd_source: Option<(i32, &str, i32)>,
    pid_locks_for_xref: Option<&[Lock]>,
) -> Option<Lock> {
    let mut tokens = line.split_ascii_whitespace();

    // Both `/proc/locks` lines and `/proc/<pid>/fdinfo/<fd>` `lock:` lines
    // carry the same leading `<id>: ` token (verified against a live
    // fdinfo file: `lock:\t1: FLOCK  ADVISORY  WRITE 2268 ...`) — always
    // consume it, but only the top-level `/proc/locks` path (`fd_source:
    // None`) actually needs the id, for `BLOCKER` cross-referencing later.
    let id_tok = tokens.next()?.strip_suffix(':')?;
    let id: i64 = if fd_source.is_none() {
        id_tok.parse().ok()?
    } else {
        -1
    };

    let mut blocked = false;
    let kind = loop {
        let tok = tokens.next()?;
        if tok != "->" {
            break tok.to_string();
        }
        blocked = true;
    };

    let mandatory = tokens.next()?.starts_with('M');
    let mode = tokens.next()?.to_string();

    // Consumed positionally regardless of source: for a top-level
    // `/proc/locks` line this is the owning pid; for an fdinfo `lock:`
    // line the caller already knows the pid, so this field is discarded.
    let pid_field = tokens.next()?;

    let (mut process_id, mut command_name, unknown_command_name) =
        if let Some((pid, comm, _fd)) = fd_source {
            (pid, Some(comm.to_string()), false)
        } else {
            let pid: i32 = pid_field.parse().ok()?;
            if pid > 0 {
                match read_comm(pid) {
                    Some(comm) => (pid, Some(comm), false),
                    None => (pid, None, true),
                }
            } else {
                (pid, None, false)
            }
        };

    let dev_field = tokens.next()?;
    let mut dev_parts = dev_field.split(':');
    let major = u32::from_str_radix(dev_parts.next()?, 16).ok()?;
    let minor = u32::from_str_radix(dev_parts.next()?, 16).ok()?;
    let inode: u64 = dev_parts.next()?.parse().ok()?;

    let start_field = tokens.next()?;
    let start: u64 = if start_field == "EOF" {
        0
    } else {
        start_field.parse().ok()?
    };
    let end_field = tokens.next()?;
    let end: u64 = if end_field == "EOF" {
        0
    } else {
        end_field.parse().ok()?
    };

    if let Some(pid_locks) = pid_locks_for_xref {
        if command_name.is_none() && !blocked {
            if let Some(found) = pid_locks.iter().find(|l| {
                l.start == start
                    && l.end == end
                    && l.inode == inode
                    && l.major == major
                    && l.minor == minor
                    && l.mandatory == mandatory
                    && l.blocked == blocked
                    && l.kind == kind
                    && l.mode == mode
            }) {
                process_id = found.process_id;
                command_name = found.command_name.clone();
            }
        }
    }

    if command_name.is_none() {
        command_name = Some(
            if unknown_command_name {
                "(unknown)"
            } else {
                "(undefined)"
            }
            .to_string(),
        );
    }

    let (mut path, size) = find_fd_path_and_size(process_id, inode)
        .map(|(p, s)| (Some(p), Some(s)))
        .unwrap_or((None, None));

    if path.is_none() {
        if no_inaccessible {
            return None;
        }
        path = fallback_file_name(major, minor);
    }

    let file_descriptor = fd_source.map(|(_, _, fd)| fd).unwrap_or(-1);

    Some(Lock {
        id,
        blocked,
        kind,
        mandatory,
        mode,
        process_id,
        major,
        minor,
        inode,
        start,
        end,
        command_name,
        path,
        size,
        file_descriptor,
    })
}

/// Builds the per-process lock set from every `/proc/<pid>/fdinfo/<fd>`
/// file's `lock:` lines. Used both to display `HOLDERS` and to recover the
/// true owner of a `/proc/locks` entry whose pid field doesn't resolve.
fn collect_pid_locks(no_inaccessible: bool) -> Result<Vec<Lock>, String> {
    let mut out = Vec::new();
    let proc_dir = fs::read_dir("/proc").map_err(|e| format!("cannot read /proc: {e}"))?;

    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = name.parse::<i32>() else {
            continue;
        };
        let comm = read_comm(pid).unwrap_or_default();

        let fdinfo_dir = entry.path().join("fdinfo");
        let Ok(fdinfo_entries) = fs::read_dir(&fdinfo_dir) else {
            continue;
        };

        for fd_entry in fdinfo_entries.flatten() {
            let fd_name = fd_entry.file_name();
            let Ok(fd) = fd_name.to_string_lossy().parse::<i32>() else {
                continue;
            };
            let Ok(contents) = fs::read_to_string(fd_entry.path()) else {
                continue;
            };
            for line in contents.lines() {
                let Some(suffix) = line.strip_prefix("lock:") else {
                    continue;
                };
                if let Some(lock) =
                    parse_and_resolve(no_inaccessible, suffix.trim(), Some((pid, &comm, fd)), None)
                {
                    out.push(lock);
                }
            }
        }
    }

    Ok(out)
}

/// Parses `/proc/locks` into the primary list of locks to display,
/// resolving each entry's real owner/path via `pid_locks` where needed.
fn collect_proc_locks(no_inaccessible: bool, pid_locks: &[Lock]) -> Result<Vec<Lock>, String> {
    let content =
        fs::read_to_string("/proc/locks").map_err(|e| format!("cannot read /proc/locks: {e}"))?;
    let mut out = Vec::new();
    for line in content.lines() {
        if let Some(lock) = parse_and_resolve(no_inaccessible, line, None, Some(pid_locks)) {
            out.push(lock);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_columns_defaults_to_default_set() {
        assert_eq!(
            resolve_columns(None, false).unwrap(),
            DEFAULT_COLUMNS.to_vec()
        );
        assert_eq!(resolve_columns(None, true).unwrap(), ALL_COLUMNS.to_vec());
    }

    #[test]
    fn resolve_columns_explicit_list_replaces_default() {
        let cols = resolve_columns(Some("PID,PATH"), false).unwrap();
        assert_eq!(cols, vec!["PID", "PATH"]);
        // --output-all is ignored when an explicit (non-`+`) list is given.
        let cols = resolve_columns(Some("PID,PATH"), true).unwrap();
        assert_eq!(cols, vec!["PID", "PATH"]);
    }

    #[test]
    fn resolve_columns_plus_prefix_appends_to_default() {
        let cols = resolve_columns(Some("+INODE,MAJ:MIN"), false).unwrap();
        let mut expected = DEFAULT_COLUMNS.to_vec();
        expected.extend(["INODE", "MAJ:MIN"]);
        assert_eq!(cols, expected);
    }

    #[test]
    fn resolve_columns_rejects_unknown_name() {
        assert!(resolve_columns(Some("BOGUS"), false).is_err());
    }

    #[test]
    fn parse_top_level_lock_line() {
        // A real /proc/locks-style line; the pid here won't resolve to a
        // real process in the test environment, so path resolution falls
        // through to `None` (no_inaccessible=false, and there's unlikely to
        // be a real mount matching major:minor 8:1 in the sandbox, so this
        // just exercises the parse + graceful-fallback path).
        let line = "1: POSIX  MANDATORY  WRITE 999999 08:01:123456 0 EOF";
        let lock = parse_and_resolve(false, line, None, None).expect("should parse");
        assert_eq!(lock.id, 1);
        assert!(!lock.blocked);
        assert_eq!(lock.kind, "POSIX");
        assert!(lock.mandatory);
        assert_eq!(lock.mode, "WRITE");
        assert_eq!(lock.process_id, 999999);
        assert_eq!(lock.major, 8);
        assert_eq!(lock.minor, 1);
        assert_eq!(lock.inode, 123456);
        assert_eq!(lock.start, 0);
        assert_eq!(lock.end, 0);
        // No such pid: comm lookup fails -> "(unknown)".
        assert_eq!(lock.command_name.as_deref(), Some("(unknown)"));
    }

    #[test]
    fn parse_blocked_lock_line_sets_blocked_and_advisory_kind() {
        let line = "5: -> FLOCK  ADVISORY  READ 999998 00:1d:42 10 20";
        let lock = parse_and_resolve(false, line, None, None).expect("should parse");
        assert!(lock.blocked);
        assert_eq!(lock.kind, "FLOCK");
        assert!(!lock.mandatory);
        assert_eq!(lock.start, 10);
        assert_eq!(lock.end, 20);
    }

    #[test]
    fn parse_no_inaccessible_drops_unresolvable_lock() {
        let line = "1: POSIX  ADVISORY  WRITE 999999 08:01:123456 0 EOF";
        // With no_inaccessible=true and no fd/mount match possible for this
        // synthetic pid/inode, the lock must be dropped entirely.
        assert!(parse_and_resolve(true, line, None, None).is_none());
    }

    #[test]
    fn parse_fdinfo_lock_line_uses_given_pid_and_command() {
        // fdinfo `lock:` lines carry the same leading "<id>: " token as
        // `/proc/locks` (verified against a live `/proc/<pid>/fdinfo/<fd>`
        // file); it's consumed but discarded, and so is the pid field
        // present on the line, in favor of the already-known (pid, comm).
        let line = "1: FLOCK  ADVISORY  WRITE 42 00:1d:7 0 EOF";
        let lock =
            parse_and_resolve(false, line, Some((4242, "myproc", 7)), None).expect("should parse");
        assert_eq!(lock.id, -1);
        assert_eq!(lock.process_id, 4242);
        assert_eq!(lock.command_name.as_deref(), Some("myproc"));
        assert_eq!(lock.file_descriptor, 7);
    }

    #[test]
    fn parse_fdinfo_lock_line_without_id_prefix_fails() {
        // Guards the fix above: a line missing the leading "<id>: " token
        // must fail to parse rather than silently misreading "FLOCK" as an
        // id and shifting every subsequent field by one.
        let line = "FLOCK  ADVISORY  WRITE 42 00:1d:7 0 EOF";
        assert!(parse_and_resolve(false, line, Some((4242, "myproc", 7)), None).is_none());
    }

    #[test]
    fn parse_malformed_line_returns_none() {
        assert!(parse_and_resolve(false, "not a lock line", None, None).is_none());
        assert!(parse_and_resolve(false, "", None, None).is_none());
    }

    #[test]
    fn xref_recovers_command_name_from_pid_locks() {
        let pid_locks = vec![Lock {
            id: -1,
            blocked: false,
            kind: "POSIX".to_string(),
            mandatory: true,
            mode: "WRITE".to_string(),
            process_id: 4242,
            major: 8,
            minor: 1,
            inode: 123456,
            start: 0,
            end: 0,
            command_name: Some("realproc".to_string()),
            path: None,
            size: None,
            file_descriptor: 3,
        }];
        // Same (kind, mandatory, mode, range, inode, device) signature as
        // the fixture above, but sourced as a top-level /proc/locks line
        // with an unresolvable pid (999999) — should adopt pid_locks' pid
        // and command name instead of falling back to "(unknown)".
        let line = "1: POSIX  MANDATORY  WRITE 999999 08:01:123456 0 EOF";
        let lock = parse_and_resolve(false, line, None, Some(&pid_locks)).expect("should parse");
        assert_eq!(lock.process_id, 4242);
        assert_eq!(lock.command_name.as_deref(), Some("realproc"));
    }
}
