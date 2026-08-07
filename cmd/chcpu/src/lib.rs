//! user chcpu — configure CPUs (enable/disable/configure/deconfigure,
//! set the CPU dispatching mode, or trigger a CPU rescan) via the
//! `/sys/devices/system/cpu` sysfs interface.
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use usercore::Ui;

/// Real location of the sysfs CPU tree on Linux.
const PATH_SYS_CPU: &str = "/sys/devices/system/cpu";

const HELP: &str = "Usage: chcpu [options]\n\
Configure CPUs.\n\n\
  -e, --enable <cpu-list>       enable CPUs\n\
  -d, --disable <cpu-list>      disable CPUs\n\
  -c, --configure <cpu-list>    configure CPUs\n\
  -g, --deconfigure <cpu-list>  deconfigure CPUs\n\
  -p, --dispatch <mode>         set dispatching mode (horizontal, vertical)\n\
  -r, --rescan                  trigger a rescan of CPUs\n\
  -h, --help                    display this help and exit\n\
      --version                 output version information and exit\n\n\
<cpu-list> is one or more elements separated by commas. Each element is\n\
either a positive integer (e.g., 3), or an inclusive range of positive\n\
integers (e.g., 0-5). For example, 0,2,7,10-13 refers to CPUs whose\n\
addresses are: 0, 2, 7, 10, 11, 12, and 13.\n";

/// Entry point for the `chcpu` utility. Parses `std::env::args()`,
/// performs the requested action against `/sys/devices/system/cpu`, and
/// prints one status line per CPU touched.
///
/// Returns 0 on full success, 64 on partial success (some CPUs in the
/// list failed, at least one succeeded — matches util-linux's `chcpu`),
/// and 1 on a usage error or total failure.
pub fn run() -> i32 {
    let ui = Ui::new("chcpu");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return if args.is_empty() { 1 } else { 0 };
    }
    if args.iter().any(|a| a == "--version") {
        println!("chcpu (user_utils) 0.1.0");
        return 0;
    }

    let action = match parse_action(&args) {
        Ok(a) => a,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    let sys = SysCpu::new(Path::new(PATH_SYS_CPU));
    match run_action(&sys, &action) {
        Ok(RunOutcome::Success) => 0,
        Ok(RunOutcome::PartialFailure) => 64,
        Err(e) => {
            ui.err(&e);
            1
        }
    }
}

/// The single action requested on the command line.
enum Action {
    Enable(CpuList),
    Disable(CpuList),
    Configure(CpuList),
    Deconfigure(CpuList),
    Dispatch(DispatchMode),
    Rescan,
}

/// Parse `chcpu`'s (mutually-exclusive) action out of `args` (already
/// stripped of `argv[0]`; `--help`/`--version` handled by the caller).
fn parse_action(args: &[String]) -> Result<Action, String> {
    let mut action: Option<Action> = None;
    let set = |action: &mut Option<Action>, new: Action| -> Result<(), String> {
        if action.is_some() {
            return Err(
                "only one of --enable, --disable, --configure, --deconfigure, --dispatch, \
                 --rescan may be given"
                    .to_string(),
            );
        }
        *action = Some(new);
        Ok(())
    };

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-e" | "--enable" => {
                let v = next_value(args, &mut i, "--enable")?;
                set(&mut action, Action::Enable(CpuList::parse(v)?))?;
            }
            "-d" | "--disable" => {
                let v = next_value(args, &mut i, "--disable")?;
                set(&mut action, Action::Disable(CpuList::parse(v)?))?;
            }
            "-c" | "--configure" => {
                let v = next_value(args, &mut i, "--configure")?;
                set(&mut action, Action::Configure(CpuList::parse(v)?))?;
            }
            "-g" | "--deconfigure" => {
                let v = next_value(args, &mut i, "--deconfigure")?;
                set(&mut action, Action::Deconfigure(CpuList::parse(v)?))?;
            }
            "-p" | "--dispatch" => {
                let v = next_value(args, &mut i, "--dispatch")?;
                set(&mut action, Action::Dispatch(DispatchMode::parse(v)?))?;
            }
            "-r" | "--rescan" => set(&mut action, Action::Rescan)?,
            other => return Err(format!("unknown option -- '{other}'")),
        }
        i += 1;
    }
    action.ok_or_else(|| "no action specified".to_string())
}

fn next_value<'a>(args: &'a [String], i: &mut usize, opt: &str) -> Result<&'a str, String> {
    *i += 1;
    args.get(*i)
        .map(String::as_str)
        .ok_or_else(|| format!("option '{opt}' requires an argument"))
}

enum RunOutcome {
    Success,
    PartialFailure,
}

fn run_action(sys: &SysCpu, action: &Action) -> Result<RunOutcome, String> {
    match action {
        Action::Enable(list) => {
            let mut enabled = sys.enabled_cpu_list();
            run_over_list(list, |idx| sys.enable_cpu(enabled.as_mut(), idx, true))
        }
        Action::Disable(list) => {
            let mut enabled = sys.enabled_cpu_list();
            run_over_list(list, |idx| sys.enable_cpu(enabled.as_mut(), idx, false))
        }
        Action::Configure(list) => {
            let enabled = sys.enabled_cpu_list();
            run_over_list(list, |idx| sys.configure_cpu(enabled.as_ref(), idx, true))
        }
        Action::Deconfigure(list) => {
            let enabled = sys.enabled_cpu_list();
            run_over_list(list, |idx| sys.configure_cpu(enabled.as_ref(), idx, false))
        }
        Action::Dispatch(mode) => match sys.set_dispatch_mode(*mode) {
            Ok(msg) => {
                println!("{msg}");
                Ok(RunOutcome::Success)
            }
            Err(e) => Err(e),
        },
        Action::Rescan => match sys.rescan_cpus() {
            Ok(msg) => {
                println!("{msg}");
                Ok(RunOutcome::Success)
            }
            Err(e) => Err(e),
        },
    }
}

/// Run `f` over every CPU index in `list`, printing a line per success
/// and an error line per failure, and continuing past individual
/// failures (matches util-linux's `chcpu`, which reports the whole
/// requested list rather than stopping at the first bad CPU).
fn run_over_list(
    list: &CpuList,
    mut f: impl FnMut(usize) -> Result<String, String>,
) -> Result<RunOutcome, String> {
    let mut succeeded = false;
    let mut first_error: Option<String> = None;
    for idx in list.iter() {
        match f(idx) {
            Ok(msg) => {
                println!("{msg}");
                succeeded = true;
            }
            Err(e) => {
                eprintln!("{e}");
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    match (succeeded, first_error) {
        (_, None) => Ok(RunOutcome::Success),
        (true, Some(_)) => Ok(RunOutcome::PartialFailure),
        (false, Some(e)) => Err(e),
    }
}

/// A parsed CPU dispatching mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum DispatchMode {
    Horizontal = 0,
    Vertical = 1,
}

impl DispatchMode {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "horizontal" => Ok(Self::Horizontal),
            "vertical" => Ok(Self::Vertical),
            _ => Err(format!("invalid dispatch mode: '{s}'")),
        }
    }
}

impl fmt::Display for DispatchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Horizontal => write!(f, "horizontal"),
            Self::Vertical => write!(f, "vertical"),
        }
    }
}

/// A parsed `<cpu-list>` such as `0-3,5` — a set of CPU indices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CpuList(BTreeSet<usize>);

impl CpuList {
    /// Parse a comma-separated list of CPU indices and/or inclusive
    /// ranges (`first-last`). Matches the syntax `chcpu`'s `--enable`
    /// etc. accept, and the format the kernel uses for files like
    /// `/sys/devices/system/cpu/online`.
    fn parse(s: &str) -> Result<Self, String> {
        let mut set = BTreeSet::new();
        for element in s.split(',') {
            let element = element.trim();
            if element.is_empty() {
                return Err("CPU list element is not a positive number".to_string());
            }
            let mut parts = element.splitn(2, '-');
            let first_str = parts.next().unwrap().trim();
            let last_str = parts.next().map(str::trim);
            let first: usize = first_str
                .parse()
                .map_err(|_| "CPU list element is not a positive number".to_string())?;
            match last_str {
                Some(last_str) => {
                    let last: usize = last_str
                        .parse()
                        .map_err(|_| "CPU list element is not a positive number".to_string())?;
                    if first > last {
                        return Err(
                            "first element of CPU list range is greater than its last element"
                                .to_string(),
                        );
                    }
                    set.extend(first..=last);
                }
                None => {
                    set.insert(first);
                }
            }
        }
        if set.is_empty() {
            return Err("CPU list is empty".to_string());
        }
        Ok(Self(set))
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().copied()
    }

    fn insert(&mut self, idx: usize) {
        self.0.insert(idx);
    }

    fn remove(&mut self, idx: usize) {
        self.0.remove(&idx);
    }

    fn contains(&self, idx: usize) -> bool {
        self.0.contains(&idx)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

/// Thin wrapper around a sysfs CPU tree (normally
/// `/sys/devices/system/cpu`, overridable so tests can point it at a
/// fixture directory).
struct SysCpu<'a> {
    base: &'a Path,
}

impl<'a> SysCpu<'a> {
    fn new(base: &'a Path) -> Self {
        Self { base }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.base.join(rel)
    }

    fn cpu_dir(&self, idx: usize) -> Result<PathBuf, String> {
        let dir = self.path(&format!("cpu{idx}"));
        if dir.exists() {
            Ok(dir)
        } else {
            Err(format!("CPU {idx} does not exist"))
        }
    }

    fn read_bool(&self, path: &Path) -> Result<bool, String> {
        let s = fs::read_to_string(path)
            .map_err(|e| format!("failed to read file '{}': {e}", path.display()))?;
        let trimmed = s.trim();
        let v: i32 = trimmed
            .parse()
            .map_err(|_| format!("data is not an integer '{trimmed}'"))?;
        Ok(v != 0)
    }

    fn write_value(&self, path: &Path, value: &str) -> Result<(), String> {
        fs::write(path, value)
            .map_err(|e| format!("failed to write file '{}': {e}", path.display()))
    }

    /// The set of currently-online CPUs, from `online`. `None` if that
    /// file is missing or unparseable (mirrors the upstream tool, which
    /// treats this as "unknown" rather than fatal).
    fn enabled_cpu_list(&self) -> Option<CpuList> {
        fs::read_to_string(self.path("online"))
            .ok()
            .and_then(|s| CpuList::parse(s.trim()).ok())
    }

    fn enable_cpu(
        &self,
        enabled_cpu_list: Option<&mut CpuList>,
        cpu_index: usize,
        enable: bool,
    ) -> Result<String, String> {
        let dir = self.cpu_dir(cpu_index)?;
        let online_path = dir.join("online");
        if !online_path.exists() {
            return Err(format!("CPU {cpu_index} is not hot pluggable"));
        }

        let online = self.read_bool(&online_path)?;
        let new_state = if enable { "enabled" } else { "disabled" };

        if enable == online {
            return Ok(format!("CPU {cpu_index} is already {new_state}"));
        }

        if !enable {
            if let Some(list) = enabled_cpu_list.as_deref() {
                if list.len() <= 1 {
                    return Err("only one CPU is enabled".to_string());
                }
            }
        }

        let configured = self.read_bool(&dir.join("configure"));

        if let Err(e) = self.write_value(&online_path, if enable { "1" } else { "0" }) {
            let op = if enable { "enable" } else { "disable" };
            let reason = if enable && configured == Ok(false) {
                " (CPU is deconfigured)"
            } else {
                ""
            };
            return Err(format!("CPU {cpu_index} {op} failed{reason}: {e}"));
        }

        if let Some(list) = enabled_cpu_list {
            if enable {
                list.insert(cpu_index);
            } else {
                list.remove(cpu_index);
            }
        }

        Ok(format!("CPU {cpu_index} {new_state}"))
    }

    fn configure_cpu(
        &self,
        enabled_cpu_list: Option<&CpuList>,
        cpu_index: usize,
        configure: bool,
    ) -> Result<String, String> {
        let dir = self.cpu_dir(cpu_index)?;
        let configure_path = dir.join("configure");
        if !configure_path.exists() {
            return Err(format!("CPU {cpu_index} is not configurable"));
        }

        let previous = self.read_bool(&configure_path)?;
        let new_state = if configure {
            "configured"
        } else {
            "deconfigured"
        };

        if configure == previous {
            return Ok(format!("CPU {cpu_index} is already {new_state}"));
        }

        if let Some(list) = enabled_cpu_list {
            if previous && !configure && list.contains(cpu_index) {
                return Err(format!("CPU {cpu_index} is enabled"));
            }
        }

        if let Err(e) = self.write_value(&configure_path, if configure { "1" } else { "0" }) {
            let op = if configure {
                "configure"
            } else {
                "deconfigure"
            };
            Err(format!("CPU {cpu_index} {op} failed: {e}"))
        } else {
            Ok(format!("CPU {cpu_index} {new_state}"))
        }
    }

    fn set_dispatch_mode(&self, mode: DispatchMode) -> Result<String, String> {
        let path = self.path("dispatching");
        if !path.exists() {
            return Err(
                "this system does not support setting the dispatching mode of CPUs".to_string(),
            );
        }
        self.write_value(&path, &(mode as u8).to_string())
            .map_err(|e| format!("failed to set dispatch mode: {e}"))?;
        Ok(format!("Successfully set {mode} dispatching mode"))
    }

    fn rescan_cpus(&self) -> Result<String, String> {
        let path = self.path("rescan");
        if !path.exists() {
            return Err("this system does not support rescanning of CPUs".to_string());
        }
        self.write_value(&path, "1")
            .map_err(|e| format!("failed to trigger rescan of CPUs: {e}"))?;
        Ok("Triggered rescan of CPUs".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmp_cpu_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("user-chcpu-test-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // --- CpuList parsing -----------------------------------------------

    #[test]
    fn cpu_list_parses_single_index() {
        let l = CpuList::parse("3").unwrap();
        assert_eq!(l.iter().collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn cpu_list_parses_range() {
        let l = CpuList::parse("0-3").unwrap();
        assert_eq!(l.iter().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn cpu_list_parses_mixed_commas_and_ranges() {
        let l = CpuList::parse("0,2,7,10-13").unwrap();
        assert_eq!(l.iter().collect::<Vec<_>>(), vec![0, 2, 7, 10, 11, 12, 13]);
    }

    #[test]
    fn cpu_list_dedupes_overlapping_entries() {
        let l = CpuList::parse("0-2,1-3").unwrap();
        assert_eq!(l.iter().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn cpu_list_rejects_empty_string() {
        assert!(CpuList::parse("").is_err());
    }

    #[test]
    fn cpu_list_rejects_non_numeric() {
        assert!(CpuList::parse("abc").is_err());
    }

    #[test]
    fn cpu_list_rejects_reversed_range() {
        let err = CpuList::parse("5-2").unwrap_err();
        assert!(err.contains("greater than"));
    }

    // --- DispatchMode parsing -------------------------------------------

    #[test]
    fn dispatch_mode_parses_known_values() {
        assert_eq!(
            DispatchMode::parse("horizontal").unwrap(),
            DispatchMode::Horizontal
        );
        assert_eq!(
            DispatchMode::parse("vertical").unwrap(),
            DispatchMode::Vertical
        );
    }

    #[test]
    fn dispatch_mode_rejects_unknown_value() {
        assert!(DispatchMode::parse("diagonal").is_err());
    }

    // --- CLI parsing ------------------------------------------------------

    #[test]
    fn parse_action_enable() {
        let args: Vec<String> = ["-e", "0-3"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(parse_action(&args), Ok(Action::Enable(_))));
    }

    #[test]
    fn parse_action_rejects_multiple_actions() {
        let args: Vec<String> = ["-e", "0", "-r"].iter().map(|s| s.to_string()).collect();
        assert!(parse_action(&args).is_err());
    }

    #[test]
    fn parse_action_rejects_missing_value() {
        let args: Vec<String> = ["--enable"].iter().map(|s| s.to_string()).collect();
        assert!(parse_action(&args).is_err());
    }

    #[test]
    fn parse_action_rejects_unknown_flag() {
        let args: Vec<String> = ["--bogus"].iter().map(|s| s.to_string()).collect();
        assert!(parse_action(&args).is_err());
    }

    #[test]
    fn parse_action_rescan() {
        let args: Vec<String> = vec!["-r".to_string()];
        assert!(matches!(parse_action(&args), Ok(Action::Rescan)));
    }

    // --- SysCpu against a real (unprivileged) system, when present ------

    #[test]
    fn real_sysfs_cpu_tree_if_present_reports_cpu0_not_hotpluggable_or_missing() {
        // On this sandbox /sys/devices/system/cpu/cpu0 exists but (like most
        // real x86 systems) the boot CPU has no `online` attribute, so
        // enabling/disabling it must fail cleanly rather than panicking.
        let sys = SysCpu::new(Path::new(PATH_SYS_CPU));
        if !sys.base.exists() {
            eprintln!("skipping: {PATH_SYS_CPU} not present in this sandbox");
            return;
        }
        let mut enabled = sys.enabled_cpu_list();
        let result = sys.enable_cpu(enabled.as_mut(), 0, false);
        assert!(
            result.is_err(),
            "expected cpu0 disable to fail cleanly, got {result:?}"
        );
    }

    #[test]
    fn real_sysfs_dispatch_and_rescan_report_unsupported_when_absent() {
        let sys = SysCpu::new(Path::new(PATH_SYS_CPU));
        if !sys.base.exists() {
            eprintln!("skipping: {PATH_SYS_CPU} not present in this sandbox");
            return;
        }
        if !sys.path("dispatching").exists() {
            let err = sys.set_dispatch_mode(DispatchMode::Horizontal).unwrap_err();
            assert!(err.contains("does not support"));
        }
        if !sys.path("rescan").exists() {
            let err = sys.rescan_cpus().unwrap_err();
            assert!(err.contains("does not support"));
        }
    }

    // --- SysCpu against a fully-controlled fixture directory -------------

    #[test]
    fn fixture_enable_disable_round_trip() {
        let root = tmp_cpu_root();
        let cpu2 = root.join("cpu2");
        fs::create_dir_all(&cpu2).unwrap();
        fs::write(cpu2.join("online"), "0\n").unwrap();
        fs::write(root.join("online"), "0-1\n").unwrap();

        let sys = SysCpu::new(&root);
        let mut enabled = sys.enabled_cpu_list();
        let msg = sys.enable_cpu(enabled.as_mut(), 2, true).unwrap();
        assert_eq!(msg, "CPU 2 enabled");
        assert_eq!(fs::read_to_string(cpu2.join("online")).unwrap(), "1");

        // Enabling again reports "already enabled" and doesn't error.
        let msg2 = sys.enable_cpu(enabled.as_mut(), 2, true).unwrap();
        assert_eq!(msg2, "CPU 2 is already enabled");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fixture_disable_last_cpu_is_refused() {
        let root = tmp_cpu_root();
        let cpu0 = root.join("cpu0");
        fs::create_dir_all(&cpu0).unwrap();
        fs::write(cpu0.join("online"), "1\n").unwrap();
        fs::write(root.join("online"), "0\n").unwrap();

        let sys = SysCpu::new(&root);
        let mut enabled = sys.enabled_cpu_list();
        assert_eq!(enabled.as_ref().unwrap().len(), 1);
        let err = sys.enable_cpu(enabled.as_mut(), 0, false).unwrap_err();
        assert!(err.contains("only one CPU is enabled"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fixture_configure_deconfigure_round_trip() {
        let root = tmp_cpu_root();
        let cpu3 = root.join("cpu3");
        fs::create_dir_all(&cpu3).unwrap();
        fs::write(cpu3.join("configure"), "1\n").unwrap();
        fs::write(root.join("online"), "0-2\n").unwrap(); // cpu3 not enabled

        let sys = SysCpu::new(&root);
        let enabled = sys.enabled_cpu_list();
        let msg = sys.configure_cpu(enabled.as_ref(), 3, false).unwrap();
        assert_eq!(msg, "CPU 3 deconfigured");
        assert_eq!(fs::read_to_string(cpu3.join("configure")).unwrap(), "0");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fixture_deconfigure_enabled_cpu_is_refused() {
        let root = tmp_cpu_root();
        let cpu1 = root.join("cpu1");
        fs::create_dir_all(&cpu1).unwrap();
        fs::write(cpu1.join("configure"), "1\n").unwrap();
        fs::write(root.join("online"), "0-1\n").unwrap(); // cpu1 enabled

        let sys = SysCpu::new(&root);
        let enabled = sys.enabled_cpu_list();
        let err = sys.configure_cpu(enabled.as_ref(), 1, false).unwrap_err();
        assert!(err.contains("is enabled"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fixture_dispatch_mode_write() {
        let root = tmp_cpu_root();
        fs::write(root.join("dispatching"), "0\n").unwrap();

        let sys = SysCpu::new(&root);
        let msg = sys.set_dispatch_mode(DispatchMode::Vertical).unwrap();
        assert_eq!(msg, "Successfully set vertical dispatching mode");
        assert_eq!(fs::read_to_string(root.join("dispatching")).unwrap(), "1");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fixture_rescan_write() {
        let root = tmp_cpu_root();
        fs::write(root.join("rescan"), "0\n").unwrap();

        let sys = SysCpu::new(&root);
        let msg = sys.rescan_cpus().unwrap();
        assert_eq!(msg, "Triggered rescan of CPUs");
        assert_eq!(fs::read_to_string(root.join("rescan")).unwrap(), "1");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fixture_nonexistent_cpu_index_errors() {
        let root = tmp_cpu_root();
        let sys = SysCpu::new(&root);
        let err = sys.cpu_dir(99).unwrap_err();
        assert!(err.contains("does not exist"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn run_over_list_reports_partial_failure() {
        let list = CpuList::parse("0-2").unwrap();
        let result = run_over_list(&list, |idx| {
            if idx == 1 {
                Err(format!("CPU {idx} boom"))
            } else {
                Ok(format!("CPU {idx} ok"))
            }
        });
        assert!(matches!(result, Ok(RunOutcome::PartialFailure)));
    }

    #[test]
    fn run_over_list_reports_total_failure() {
        let list = CpuList::parse("0-1").unwrap();
        let result = run_over_list(&list, |idx| Err(format!("CPU {idx} boom")));
        assert!(result.is_err());
    }

    #[test]
    fn run_over_list_reports_full_success() {
        let list = CpuList::parse("0-1").unwrap();
        let result = run_over_list(&list, |idx| Ok(format!("CPU {idx} ok")));
        assert!(matches!(result, Ok(RunOutcome::Success)));
    }
}
