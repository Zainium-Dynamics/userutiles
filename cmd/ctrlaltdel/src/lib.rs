//! user ctrlaltdel — get or set the kernel's handling of the
//! Ctrl-Alt-Del key combination, via `/proc/sys/kernel/ctrl-alt-del`.
use std::path::Path;

use usercore::Ui;

/// Real location of the ctrl-alt-del sysctl on Linux.
const CTRL_ALT_DEL_PATH: &str = "/proc/sys/kernel/ctrl-alt-del";

const HELP: &str = "Usage: ctrlaltdel [hard|soft]\n\
Set (or query) the function of the ctrl-alt-del combination.\n\n\
  hard        immediate reboot (like SIGINT)\n\
  soft        send SIGINT to init (graceful)\n\
  (no args)   print the current setting\n\
  -h, --help  display this help and exit\n\
      --version  output version information and exit\n";

/// How the kernel currently reacts to Ctrl-Alt-Del.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CtrlAltDel {
    Soft,
    Hard,
}

impl CtrlAltDel {
    /// Interpret the raw sysctl value. Any value other than `0`/`1` is
    /// reported as unknown data rather than panicking (the upstream tool
    /// panics here since the kernel never actually writes anything else,
    /// but a from-scratch port should not trust that as a safety
    /// invariant for a value read from `/proc`).
    fn from_sysctl(value: i32) -> Result<Self, String> {
        match value {
            0 => Ok(Self::Soft),
            1 => Ok(Self::Hard),
            _ => Err("unknown data".to_string()),
        }
    }

    fn to_sysctl(self) -> i32 {
        match self {
            Self::Soft => 0,
            Self::Hard => 1,
        }
    }
}

impl std::fmt::Display for CtrlAltDel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Soft => write!(f, "soft"),
            Self::Hard => write!(f, "hard"),
        }
    }
}

/// Entry point for the `ctrlaltdel` utility. Parses `std::env::args()`;
/// with no operand, prints the current setting (`hard`/`soft`); with
/// `hard` or `soft`, writes the new setting to
/// `/proc/sys/kernel/ctrl-alt-del` (requires root).
///
/// Returns 0 on success, 1 on a usage error, unreadable/unwritable
/// sysctl, or unrecognized data in the sysctl file.
pub fn run() -> i32 {
    let ui = Ui::new("ctrlaltdel");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("ctrlaltdel (user_utils) 0.1.0");
        return 0;
    }

    let path = Path::new(CTRL_ALT_DEL_PATH);
    match args.as_slice() {
        [] => match get_ctrlaltdel(path) {
            Ok(v) => {
                println!("{v}");
                0
            }
            Err(e) => {
                ui.err(&e);
                1
            }
        },
        [pattern] => {
            let target = match pattern.as_str() {
                "hard" => CtrlAltDel::Hard,
                "soft" => CtrlAltDel::Soft,
                _ => {
                    ui.err(&format!("unknown argument: {pattern}"));
                    return 1;
                }
            };
            match set_ctrlaltdel(path, target) {
                Ok(()) => 0,
                Err(e) => {
                    ui.err(&e);
                    1
                }
            }
        }
        _ => {
            ui.err("too many arguments");
            1
        }
    }
}

/// Read and parse the current setting from `path` (normally
/// `/proc/sys/kernel/ctrl-alt-del`).
fn get_ctrlaltdel(path: &Path) -> Result<CtrlAltDel, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: i32 = contents
        .trim()
        .parse()
        .map_err(|_| "unknown data".to_string())?;
    CtrlAltDel::from_sysctl(value)
}

/// Write the new setting to `path`. Any write failure (almost always
/// "permission denied" for an unprivileged caller) is reported as the
/// well-known "must be root" message, matching util-linux's `ctrlaltdel`.
fn set_ctrlaltdel(path: &Path, value: CtrlAltDel) -> Result<(), String> {
    std::fs::write(path, format!("{}\n", value.to_sysctl()))
        .map_err(|_| "You must be root to set the Ctrl-Alt-Del behavior".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmp_file(contents: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("user-ctrlaltdel-test-{}-{n}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn from_sysctl_maps_zero_and_one() {
        assert_eq!(CtrlAltDel::from_sysctl(0).unwrap(), CtrlAltDel::Soft);
        assert_eq!(CtrlAltDel::from_sysctl(1).unwrap(), CtrlAltDel::Hard);
    }

    #[test]
    fn from_sysctl_rejects_other_values() {
        assert!(CtrlAltDel::from_sysctl(2).is_err());
        assert!(CtrlAltDel::from_sysctl(-1).is_err());
    }

    #[test]
    fn to_sysctl_round_trips() {
        assert_eq!(CtrlAltDel::Soft.to_sysctl(), 0);
        assert_eq!(CtrlAltDel::Hard.to_sysctl(), 1);
    }

    #[test]
    fn display_matches_cli_vocabulary() {
        assert_eq!(CtrlAltDel::Soft.to_string(), "soft");
        assert_eq!(CtrlAltDel::Hard.to_string(), "hard");
    }

    #[test]
    fn get_ctrlaltdel_reads_and_parses_fixture_file() {
        let path = tmp_file("1\n");
        assert_eq!(get_ctrlaltdel(&path).unwrap(), CtrlAltDel::Hard);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn get_ctrlaltdel_reports_unknown_data() {
        let path = tmp_file("garbage\n");
        let err = get_ctrlaltdel(&path).unwrap_err();
        assert_eq!(err, "unknown data");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn get_ctrlaltdel_reports_out_of_range_value() {
        let path = tmp_file("9\n");
        let err = get_ctrlaltdel(&path).unwrap_err();
        assert_eq!(err, "unknown data");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn get_ctrlaltdel_missing_file_errors_cleanly() {
        let path = std::env::temp_dir().join("user-ctrlaltdel-does-not-exist");
        assert!(get_ctrlaltdel(&path).is_err());
    }

    #[test]
    fn set_ctrlaltdel_writes_expected_sysctl_value() {
        let path = tmp_file("0\n");
        set_ctrlaltdel(&path, CtrlAltDel::Hard).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1\n");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn set_ctrlaltdel_on_unwritable_path_reports_must_be_root() {
        // A path inside a directory that doesn't exist can never be
        // written, which stands in for the real permission-denied case
        // this sandbox hits against the actual /proc sysctl.
        let path = std::path::Path::new("/nonexistent/user-ctrlaltdel/ctrl-alt-del");
        let err = set_ctrlaltdel(path, CtrlAltDel::Soft).unwrap_err();
        assert!(err.contains("must be root"));
    }

    #[test]
    fn real_proc_sysctl_is_readable_in_this_sandbox() {
        // /proc/sys/kernel/ctrl-alt-del is world-readable on a normal
        // Linux system; exercise the real path read-only.
        let path = Path::new(CTRL_ALT_DEL_PATH);
        if !path.exists() {
            eprintln!("skipping: {CTRL_ALT_DEL_PATH} not present in this sandbox");
            return;
        }
        let result = get_ctrlaltdel(path);
        assert!(result.is_ok(), "expected a parseable value, got {result:?}");
    }

    #[test]
    fn real_proc_sysctl_write_is_denied_without_root() {
        let path = Path::new(CTRL_ALT_DEL_PATH);
        if !path.exists() {
            eprintln!("skipping: {CTRL_ALT_DEL_PATH} not present in this sandbox");
            return;
        }
        // Never actually call `set_ctrlaltdel` (i.e. write) against the
        // real sysctl, even under an unexpectedly privileged test runner:
        // opening for write without `truncate`/`write_all` cannot mutate
        // the file's contents, but still exercises the same permission
        // check the real write path relies on.
        use std::fs::OpenOptions;
        let result = OpenOptions::new().write(true).open(path);
        match result {
            Ok(_) => eprintln!(
                "skipping strict assertion: test runner has write access to {CTRL_ALT_DEL_PATH} \
                 (likely running as root)"
            ),
            Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied),
        }
    }
}
