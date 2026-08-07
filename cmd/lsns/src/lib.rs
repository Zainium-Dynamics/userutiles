//! user lsns — list the Linux namespaces present on the system and which
//! processes belong to each one.
//!
//! Namespace identity is the inode number of `/proc/<pid>/ns/<type>` (a
//! `nsfs` inode, unique across the whole system for the lifetime of the
//! namespace), matching the real `lsns(8)`/util-linux algorithm. Namespaces
//! with no attached process ("persistent", i.e. kept alive only by a
//! bind-mount, typically under `/var/run/netns` or `/run/user/*/netns`) are
//! discovered separately by scanning `/proc/self/mountinfo` for `nsfs`
//! mounts.
use std::collections::HashMap;
use std::fs;
use std::os::linux::fs::MetadataExt;
use std::path::Path;

use usercore::Ui;

const HELP: &str = "Usage: lsns [options]\n\
List information about all the currently accessible namespaces.\n\n\
  -n, --noheadings   don't print headings\n\
  -P, --persistent   print only namespaces without processes\n\
  -h, --help         display this help and exit\n\
      --version      output version information and exit\n";

/// The eight Linux namespace kinds, in the order `/proc/<pid>/ns/<name>`
/// exposes them. This order is also used to index [`Process::ns_ids`].
const NS_NAMES: [&str; 8] = ["cgroup", "ipc", "mnt", "net", "pid", "user", "uts", "time"];

/// Entry point for the `lsns` utility.
pub fn run() -> i32 {
    let ui = Ui::new("lsns");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut noheadings = false;
    let mut persistent = false;

    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("lsns (user_utils) 0.1.0");
                return 0;
            }
            "-n" | "--noheadings" => noheadings = true,
            "-P" | "--persistent" => persistent = true,
            // Combined short options, e.g. `-nP`.
            s if s.starts_with('-') && !s.starts_with("--") && s.len() > 1 => {
                for c in s[1..].chars() {
                    match c {
                        'n' => noheadings = true,
                        'P' => persistent = true,
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
    }

    let processes = match list_processes(Path::new("/proc")) {
        Ok(p) => p,
        Err(e) => {
            ui.err(&format!("cannot read /proc: {e}"));
            return 1;
        }
    };

    let mut namespaces = collect_assigned_namespaces(&processes);
    match add_persistent_namespaces(Path::new("/proc/self/mountinfo"), &mut namespaces) {
        Ok(()) => {}
        Err(e) => {
            // Persistent-namespace discovery is best-effort: a missing or
            // unreadable mountinfo shouldn't prevent reporting the
            // process-attached namespaces we already found.
            ui.warn(&format!(
                "cannot read /proc/self/mountinfo, persistent namespaces may be missing: {e}"
            ));
        }
    }
    namespaces.sort_by_key(|ns| ns.id);

    print_table(&namespaces, &processes, noheadings, persistent);

    0
}

struct Process {
    pid: i32,
    uid: u32,
    /// Namespace inode for each of the 8 [`NS_NAMES`] kinds; 0 means this
    /// process has no entry for that kind (namespace type not supported by
    /// the running kernel, or the `/proc/<pid>/ns/<type>` read failed).
    ns_ids: [u64; 8],
    command: String,
}

struct Namespace {
    id: u64,
    ns_type: usize,
    nprocs: u32,
    representative_pid: Option<i32>,
    /// Owner uid to show when there is no representative process (a
    /// persistent, process-less namespace); real processes report their own
    /// uid via `representative_pid` instead.
    uid_fallback: u32,
}

/// Enumerate every numerically-named entry of `/proc` and read each
/// process's namespace inodes, uid, and command. A process that exits
/// mid-scan simply yields no entry (its per-file reads fail and are
/// skipped) rather than aborting the whole listing, matching how `ps`
/// tolerates processes disappearing during a scan.
fn list_processes(proc_dir: &Path) -> std::io::Result<Vec<Process>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(proc_dir)?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = name.parse::<i32>() else {
            continue;
        };

        let pid_dir = entry.path();
        let Ok(meta) = fs::metadata(&pid_dir) else {
            continue;
        };
        let uid = meta.st_uid();

        let mut ns_ids = [0u64; 8];
        for (i, ns_name) in NS_NAMES.iter().enumerate() {
            if let Ok(m) = fs::metadata(pid_dir.join("ns").join(ns_name)) {
                ns_ids[i] = m.st_ino();
            }
        }

        let command = read_process_command(&pid_dir);

        out.push(Process {
            pid,
            uid,
            ns_ids,
            command,
        });
    }
    Ok(out)
}

/// Command name for a process: prefers `/proc/<pid>/cmdline` (the full
/// argv, NUL-separated; joined back with spaces the way real `lsns(8)` and
/// `ps(1)` display it), falling back to `/proc/<pid>/comm` for kernel
/// threads and processes whose cmdline is empty (e.g. zombies).
fn read_process_command(pid_dir: &Path) -> String {
    if let Ok(content) = fs::read(pid_dir.join("cmdline")) {
        let trimmed = content.strip_suffix(&[0]).unwrap_or(&content);
        if !trimmed.is_empty() {
            let joined: Vec<u8> = trimmed
                .iter()
                .map(|&b| if b == 0 { b' ' } else { b })
                .collect();
            return String::from_utf8_lossy(&joined).into_owned();
        }
    }
    if let Ok(comm) = fs::read_to_string(pid_dir.join("comm")) {
        let comm = comm.trim();
        if !comm.is_empty() {
            return format!("[{comm}]");
        }
    }
    "?".to_string()
}

/// Group processes by shared namespace inode. The representative process
/// shown for a namespace is the one with the lowest pid, matching `lsns(8)`.
fn collect_assigned_namespaces(processes: &[Process]) -> Vec<Namespace> {
    let mut index_of_inode: HashMap<u64, usize> = HashMap::new();
    let mut namespaces: Vec<Namespace> = Vec::new();

    for process in processes {
        for (ns_type, &inode) in process.ns_ids.iter().enumerate() {
            if inode == 0 {
                continue;
            }
            let idx = *index_of_inode.entry(inode).or_insert_with(|| {
                namespaces.push(Namespace {
                    id: inode,
                    ns_type,
                    nprocs: 0,
                    representative_pid: None,
                    uid_fallback: 0,
                });
                namespaces.len() - 1
            });

            let ns = &mut namespaces[idx];
            ns.nprocs += 1;
            let is_lower = match ns.representative_pid {
                None => true,
                Some(current) => process.pid < current,
            };
            if is_lower {
                ns.representative_pid = Some(process.pid);
            }
        }
    }

    namespaces
}

/// Discover namespaces kept alive only by a bind-mount (no attached
/// process) by scanning `/proc/self/mountinfo` for `nsfs` mounts. Format of
/// interest, per line (fields are whitespace-separated, the `-` separates
/// the fixed leading fields from the filesystem-specific trailer):
///
/// ```text
/// 24 0 0:21 net:[4026531992] /var/run/netns/test rw - nsfs nsfs rw
///                ^^^^^^^^^^^                            ^^^^
///                mount root (field 4)              fs type after `-`
/// ```
fn add_persistent_namespaces(
    mountinfo_path: &Path,
    namespaces: &mut Vec<Namespace>,
) -> std::io::Result<()> {
    let mountinfo = fs::read_to_string(mountinfo_path)?;

    for line in mountinfo.lines() {
        let mut fields = line.split_whitespace();
        // Fields 0..=3 are: mount id, parent id, major:minor, root.
        let Some(mount_root) = fields.nth(3) else {
            continue;
        };

        let mut past_separator = false;
        for field in fields.by_ref() {
            if field == "-" {
                past_separator = true;
                break;
            }
        }
        if !past_separator || fields.next() != Some("nsfs") {
            continue;
        }

        let Some((type_str, inode)) = parse_nsfs_root(mount_root) else {
            continue;
        };
        let Some(ns_type) = NS_NAMES.iter().position(|&n| n == type_str) else {
            continue;
        };
        if namespaces.iter().any(|ns| ns.id == inode) {
            continue;
        }

        namespaces.push(Namespace {
            id: inode,
            ns_type,
            nprocs: 0,
            representative_pid: None,
            uid_fallback: 0,
        });
    }

    Ok(())
}

/// Parses an `nsfs` mount root like `net:[4026531992]` into `("net", 4026531992)`.
fn parse_nsfs_root(root: &str) -> Option<(&str, u64)> {
    let (type_str, rest) = root.split_once(':')?;
    let inode_str = rest.strip_prefix('[')?.strip_suffix(']')?;
    let inode = inode_str.parse::<u64>().ok()?;
    Some((type_str, inode))
}

fn print_table(
    namespaces: &[Namespace],
    processes: &[Process],
    noheadings: bool,
    persistent: bool,
) {
    let by_pid: HashMap<i32, &Process> = processes.iter().map(|p| (p.pid, p)).collect();
    let mut username_cache: HashMap<u32, String> = HashMap::new();

    if !noheadings {
        println!(
            "{:>10} {:<6} {:>6} {:>7} {:<8} COMMAND",
            "NS", "TYPE", "NPROCS", "PID", "USER"
        );
    }

    for ns in namespaces {
        if persistent && ns.nprocs != 0 {
            continue;
        }

        let representative = ns
            .representative_pid
            .and_then(|pid| by_pid.get(&pid).copied());
        let uid = representative.map(|p| p.uid).unwrap_or(ns.uid_fallback);
        let user = username_cache
            .entry(uid)
            .or_insert_with(|| uid_name(uid))
            .clone();
        let pid_col = ns
            .representative_pid
            .map(|p| p.to_string())
            .unwrap_or_default();
        let command = representative.map(|p| p.command.as_str()).unwrap_or("");

        println!(
            "{:>10} {:<6} {:>6} {:>7} {:<8} {}",
            ns.id, NS_NAMES[ns.ns_type], ns.nprocs, pid_col, user, command
        );
    }
}

/// Resolve `uid` to a username via `getpwuid(3)`, falling back to the
/// decimal uid itself if there is no matching passwd entry.
fn uid_name(uid: u32) -> String {
    // SAFETY: `getpwuid` takes a plain integer and returns either a null
    // pointer (checked below before use) or a pointer into an internal
    // static buffer owned by libc that stays valid until the next call to
    // `getpwuid`/`getpwnam`/etc. on this thread. We only read from it once,
    // synchronously, before any other libc passwd-database call can
    // invalidate it. `(*pw).pw_name` is a non-null, NUL-terminated C string
    // owned by that same static buffer, so `CStr::from_ptr` on it is valid,
    // and `to_string_lossy().into_owned()` copies the data out before the
    // buffer could be reused.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_with_ns(pid: i32, uid: u32, mnt_inode: u64, net_inode: u64) -> Process {
        let mut ns_ids = [0u64; 8];
        ns_ids[2] = mnt_inode; // mnt
        ns_ids[3] = net_inode; // net
        Process {
            pid,
            uid,
            ns_ids,
            command: format!("proc{pid}"),
        }
    }

    #[test]
    fn collects_and_groups_shared_namespaces() {
        let processes = vec![
            proc_with_ns(10, 0, 100, 200),
            proc_with_ns(5, 0, 100, 200),
            proc_with_ns(20, 1000, 100, 999),
        ];
        let namespaces = collect_assigned_namespaces(&processes);
        // 3 distinct inodes across the fleet: mnt=100 (shared by all three),
        // net=200 (shared by first two), net=999 (only pid 20).
        assert_eq!(namespaces.len(), 3);

        let mnt = namespaces.iter().find(|n| n.id == 100).unwrap();
        assert_eq!(mnt.nprocs, 3);
        // Representative is the lowest pid among {10, 5, 20} => 5.
        assert_eq!(mnt.representative_pid, Some(5));

        let net_shared = namespaces.iter().find(|n| n.id == 200).unwrap();
        assert_eq!(net_shared.nprocs, 2);
        assert_eq!(net_shared.representative_pid, Some(5));

        let net_solo = namespaces.iter().find(|n| n.id == 999).unwrap();
        assert_eq!(net_solo.nprocs, 1);
        assert_eq!(net_solo.representative_pid, Some(20));
    }

    #[test]
    fn zero_inode_is_not_a_namespace() {
        let processes = vec![Process {
            pid: 1,
            uid: 0,
            ns_ids: [0u64; 8],
            command: "init".into(),
        }];
        assert!(collect_assigned_namespaces(&processes).is_empty());
    }

    #[test]
    fn parses_nsfs_mount_root() {
        assert_eq!(
            parse_nsfs_root("net:[4026531992]"),
            Some(("net", 4026531992))
        );
        assert_eq!(
            parse_nsfs_root("mnt:[4026531840]"),
            Some(("mnt", 4026531840))
        );
        assert_eq!(parse_nsfs_root("not-nsfs"), None);
        assert_eq!(parse_nsfs_root("net:4026531992"), None);
    }

    #[test]
    fn adds_persistent_namespace_from_mountinfo() {
        let dir = std::env::temp_dir().join(format!("user-lsns-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mountinfo = dir.join("mountinfo");
        fs::write(
            &mountinfo,
            "24 0 0:21 net:[4026531992] /var/run/netns/test rw,nosuid - nsfs nsfs rw\n\
             25 0 0:4 / /proc rw,relatime shared:12 - proc proc rw\n",
        )
        .unwrap();

        let mut namespaces = Vec::new();
        add_persistent_namespaces(&mountinfo, &mut namespaces).unwrap();
        fs::remove_dir_all(&dir).ok();

        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].id, 4026531992);
        assert_eq!(NS_NAMES[namespaces[0].ns_type], "net");
        assert_eq!(namespaces[0].nprocs, 0);
        assert_eq!(namespaces[0].representative_pid, None);
    }

    #[test]
    fn persistent_namespace_skipped_if_already_assigned() {
        let dir = std::env::temp_dir().join(format!("user-lsns-test2-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mountinfo = dir.join("mountinfo");
        fs::write(
            &mountinfo,
            "24 0 0:21 net:[200] /var/run/netns/test rw - nsfs nsfs rw\n",
        )
        .unwrap();

        let mut namespaces = vec![Namespace {
            id: 200,
            ns_type: 3,
            nprocs: 2,
            representative_pid: Some(5),
            uid_fallback: 0,
        }];
        add_persistent_namespaces(&mountinfo, &mut namespaces).unwrap();
        fs::remove_dir_all(&dir).ok();

        // Already-known namespace must not be duplicated or overwritten.
        assert_eq!(namespaces.len(), 1);
        assert_eq!(namespaces[0].nprocs, 2);
    }

    #[test]
    fn read_process_command_falls_back_to_bracketed_comm() {
        let dir = std::env::temp_dir().join(format!("user-lsns-test3-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("cmdline"), b"").unwrap();
        fs::write(dir.join("comm"), b"kworker/0:1\n").unwrap();
        let cmd = read_process_command(&dir);
        fs::remove_dir_all(&dir).ok();
        assert_eq!(cmd, "[kworker/0:1]");
    }

    #[test]
    fn read_process_command_joins_full_cmdline() {
        let dir = std::env::temp_dir().join(format!("user-lsns-test4-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("cmdline"), b"/sbin/init\0--switched-root\0").unwrap();
        fs::write(dir.join("comm"), b"init\n").unwrap();
        let cmd = read_process_command(&dir);
        fs::remove_dir_all(&dir).ok();
        assert_eq!(cmd, "/sbin/init --switched-root");
    }

    #[test]
    fn read_process_command_handles_single_nul_terminated_cmdline() {
        // Some processes (e.g. Firefox content processes) rewrite their own
        // argv into one descriptive NUL-terminated string containing
        // embedded spaces rather than real per-arg NUL separators — the
        // whole string (minus the trailing NUL) should come through as-is.
        let dir = std::env::temp_dir().join(format!("user-lsns-test5-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("cmdline"), b"firefox -contentproc 279 tab\0").unwrap();
        fs::write(dir.join("comm"), b"firefox\n").unwrap();
        let cmd = read_process_command(&dir);
        fs::remove_dir_all(&dir).ok();
        assert_eq!(cmd, "firefox -contentproc 279 tab");
    }
}
