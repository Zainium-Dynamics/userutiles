use colored::Colorize;

use crate::utils::priority::{format_memory, format_nice, nice_to_label};
use crate::utils::process::ProcessInfo;
use crate::utils::timebound::format_duration;

// -- Colour Palette ------------------------------------------------------------
//
// Heading : bright cyan
// Label key : soft green (bright green)
// Value : bright magenta / purple
// Negative nice: bright red / orange (bright_red)
// Positive nice: yellow
// Zero nice : bright magenta
// Success (✓) : bright green
// Error (✖) : bright red
// Separator : dark grey / dim white

const TICK: &str = "✓";
const CROSS: &str = "✖";
const ARROW: &str = "→";

// -- Internal helpers ----------------------------------------------------------

fn heading(s: &str) -> String {
    format!("{}", s.bright_cyan().bold())
}

fn label(s: &str) -> String {
    format!("{}", s.bright_green())
}

fn value(s: &str) -> String {
    format!("{}", s.bright_magenta())
}

fn nice_colored(n: i32) -> String {
    let s = format_nice(n);
    match n.cmp(&0) {
        std::cmp::Ordering::Less => format!("{}", s.bright_red().bold()),
        std::cmp::Ordering::Equal => format!("{}", s.bright_magenta()),
        std::cmp::Ordering::Greater => format!("{}", s.yellow()),
    }
}

fn cpu_pct_colored(pct: f32) -> String {
    let s = format!("{:.0}%", pct);
    if pct >= 80.0 {
        format!("{}", s.bright_red())
    } else if pct >= 50.0 {
        format!("{}", s.yellow())
    } else {
        format!("{}", s.bright_magenta())
    }
}

fn status_colored(nice: i32) -> String {
    let label_str = nice_to_label(nice);
    match label_str {
        "Critical" | "High" => format!("{}", label_str.bright_red().bold()),
        "Normal" => format!("{}", label_str.bright_magenta()),
        "Low" => format!("{}", label_str.yellow()),
        _ => format!("{}", label_str.dimmed()),
    }
}

fn kv(key: &str, val: String) {
    println!("  {:<14} {}", label(&format!("{} :", key)), val);
}

fn separator() {
    println!("{}", " -----------------------------------------".dimmed());
}

fn blank() {
    println!();
}

// -- Mode 1 — Basic Priority with Command -------------------------------------

/// Arguments for [`print_basic`]: the pre-spawn priority summary shown when
/// launching a command without `--auto` or full-power mode.
pub struct BasicArgs<'a> {
    pub command: &'a str,
    pub nice: i32,
    pub cpu: Option<u32>,
    pub pid: Option<u32>,
}

/// Print the pre-spawn "Setting priority..." panel for the basic (non-auto,
/// non-full-power) launch mode.
pub fn print_basic(args: &BasicArgs) {
    blank();
    println!("{}", heading("Setting priority..."));
    blank();
    kv("Command", value(args.command));
    kv("Niceness", nice_colored(args.nice));
    if let Some(cpu) = args.cpu {
        kv("CPU", value(&format!("{}%", cpu)));
    } else {
        let label_s = nice_to_label(args.nice);
        kv("CPU", value(label_s));
    }
    if let Some(pid) = args.pid {
        kv("PID", value(&pid.to_string()));
    }
    blank();
}

/// Print the "Priority boosted successfully" confirmation line for basic
/// launch mode, once the real PID is known.
pub fn print_basic_success() {
    println!(
        "{} {}",
        TICK.bright_green().bold(),
        "Priority boosted successfully".bright_green()
    );
    blank();
}

// -- Mode 2 — Full Power Mode --------------------------------------------------

/// Arguments for [`print_full_power`]: niceness, CPU/I/O/RAM, and duration
/// for a full-power launch (nice + I/O + memory cap + time bound all set).
pub struct FullPowerArgs<'a> {
    pub command: &'a str,
    pub nice: i32,
    pub cpu: u32,
    pub io_mode: &'a str,
    pub max_ram: u64,
    pub duration: std::time::Duration,
    #[allow(dead_code)]
    pub pid: u32,
}

/// Print the full "Setting enhanced priority..." panel in one shot
/// (currently unused in favor of the split pre/post spawn variants in
/// `main.rs`, kept for programmatic/test use).
#[allow(dead_code)]
pub fn print_full_power(args: &FullPowerArgs) {
    blank();
    println!("{}", heading("Setting enhanced priority..."));
    blank();
    kv("Command", value(args.command));
    kv("Niceness", nice_colored(args.nice));
    kv("CPU", value(&format!("{}%", args.cpu)));
    kv("I/O", value(args.io_mode));
    kv("Max RAM", value(&format_memory(args.max_ram)));
    kv("Duration", value(&format_duration(args.duration)));
    blank();
}

/// Print the "Supercharged successfully" confirmation for full-power mode,
/// including the real PID and the auto-revert duration.
pub fn print_full_power_success(pid: u32, duration: std::time::Duration) {
    println!(
        "{} {} {}",
        TICK.bright_green().bold(),
        "Supercharged successfully".bright_green(),
        format!("(will auto-revert after {})", format_duration(duration)).dimmed()
    );
    println!("  {:<14} {}", label("PID :"), value(&pid.to_string()));
    blank();
}

// -- Mode 3 — Quick Boost ------------------------------------------------------

/// Arguments for [`print_boost`]: the process being boosted and its old/new
/// niceness values.
pub struct BoostArgs<'a> {
    pub pid: u32,
    pub name: &'a str,
    pub old_nice: i32,
    pub new_nice: i32,
}

/// Print the "Boosting process..." heading for `--boost`, before the
/// priority change has been applied.
pub fn print_boost_start() {
    blank();
    println!("{}", heading("Boosting process..."));
    blank();
}

/// Print the `--boost` result panel: PID, process name, and old/new
/// niceness.
pub fn print_boost(args: &BoostArgs) {
    kv("PID", value(&args.pid.to_string()));
    kv("Process", value(args.name));
    kv("Old Nice", nice_colored(args.old_nice));
    kv("New Nice", nice_colored(args.new_nice));
    blank();
    println!(
        "{} {}",
        TICK.bright_green().bold(),
        "Process boosted for better responsiveness".bright_green()
    );
    blank();
}

// -- Mode 4 — Process List -----------------------------------------------------

/// Print the `--list` table: PID, name, niceness, CPU%, and status label for
/// each process in `processes` (already sorted/truncated by the caller).
pub fn print_process_list(processes: &[ProcessInfo]) {
    blank();
    println!("{}", heading("Top Processes by Priority:"));
    blank();

    // Header row
    println!("  {} {} {} {} {}",
        format!("{:<7}", "PID").bright_cyan().bold(),
        format!("{:<22}", "Process").bright_cyan().bold(),
        format!("{:<6}", "Nice").bright_cyan().bold(),
        format!("{:<6}", "CPU%").bright_cyan().bold(),
        format!("{:<12}", "Status").bright_cyan().bold(),
    );
    separator();

    for p in processes {
        let nice_str = format!("{:<6}", format_nice(p.nice));
        let nice_out = match p.nice.cmp(&0) {
            std::cmp::Ordering::Less => format!("{}", nice_str.bright_red().bold()),
            std::cmp::Ordering::Equal => format!("{}", nice_str.bright_magenta()),
            std::cmp::Ordering::Greater => format!("{}", nice_str.yellow()),
        };

        let truncated_name = truncate(&p.name, 22);

        println!("  {} {} {} {} {}",
            format!("{:<7}", p.pid).bright_magenta(),
            format!("{:<22}", truncated_name).bright_magenta(),
            nice_out,
            cpu_pct_colored(p.cpu),
            status_colored(p.nice),
        );
    }

    blank();
    println!(
        "{} {}",
        TICK.bright_green().bold(),
        format!("Showing top {} processes", processes.len()).bright_green()
    );
    blank();
}

// -- Mode 5 — Auto Mode --------------------------------------------------------

/// Print the "Smart Mode Activated" panel shown when `--auto` throttling is
/// enabled for a spawned command.
pub fn print_auto_mode(command: &str) {
    blank();
    println!("{}", heading("Smart Mode Activated"));
    blank();
    kv("Command", value(command));
    kv("Mode", value("Auto-Throttling"));
    kv("Monitoring", value("CPU Temp + Load"));
    blank();
    println!(
        "{} {}",
        TICK.bright_green().bold(),
        "Running with dynamic priority management".bright_green()
    );
    blank();
}

/// Print one line of live `--auto` telemetry: current niceness, CPU temp,
/// and load average (currently unused — the monitor thread in
/// `core::monitor` runs silently; kept for a future `--auto --verbose` live
/// view).
#[allow(dead_code)]
pub fn print_auto_update(nice: i32, temp: Option<f32>, load: Option<f32>) {
    let temp_s = temp
        .map(|t| format!("{:.1}°C", t))
        .unwrap_or_else(|| "n/a".to_string());
    let load_s = load
        .map(|l| format!("{:.2}", l))
        .unwrap_or_else(|| "n/a".to_string());
    println!("  {} {} {} {} {} {}",
        label("Nice:"),
        nice_colored(nice),
        label("Temp:"),
        value(&temp_s),
        label("Load:"),
        value(&load_s),
    );
}

// -- Mode 6 — Reset ------------------------------------------------------------

/// Print the `--reset` result panel: PID, old niceness, and confirmation
/// that it's back to the system default (0).
pub fn print_reset(pid: u32, old_nice: i32) {
    blank();
    println!("{}", heading("Resetting priority..."));
    blank();
    kv("PID", value(&pid.to_string()));
    kv("Old Nice", nice_colored(old_nice));
    kv("New Nice", nice_colored(0));
    blank();
    println!(
        "{} {}",
        TICK.bright_green().bold(),
        "Priority reset to system default".bright_green()
    );
    blank();
}

// -- Apply-to-PID (non-spawn) mode ---------------------------------------------

/// Arguments for [`print_apply`]: the existing process being reconfigured
/// via `--pid`, and the settings applied to it.
pub struct ApplyArgs<'a> {
    pub pid: u32,
    pub name: &'a str,
    pub nice: i32,
    pub io: Option<&'a str>,
}

/// Print the `--pid` result panel: PID, process name, new niceness, and
/// (if set) I/O class.
pub fn print_apply(args: &ApplyArgs) {
    blank();
    println!("{}", heading("Applying priority settings..."));
    blank();
    kv("PID", value(&args.pid.to_string()));
    kv("Process", value(args.name));
    kv("Niceness", nice_colored(args.nice));
    if let Some(io) = args.io {
        kv("I/O", value(io));
    }
    blank();
    println!(
        "{} {}",
        TICK.bright_green().bold(),
        "Priority applied successfully".bright_green()
    );
    blank();
}

// -- Error Display -------------------------------------------------------------

/// Print a styled error panel for `err`, with an optional `hint` line
/// suggesting a fix. Prefer [`print_prio_error`], which derives `hint`
/// automatically via [`crate::error::PrioError::fix_hint`].
pub fn print_error(err: &crate::error::PrioError, hint: Option<&str>) {
    blank();
    println!(
        "{} {}",
        CROSS.bright_red().bold(),
        "Failed to set priority".bright_red().bold()
    );
    println!("  {:<14} {}", label("Reason :"), format!("{}", err).yellow());
    if let Some(h) = hint {
        println!("  {:<14} {}", label("Fix :"), h.bright_cyan());
    }
    blank();
}

/// Convenience wrapper: print error + hint from the error itself.
pub fn print_prio_error(err: &crate::error::PrioError) {
    let hint = err.fix_hint();
    print_error(err, hint.as_deref());
}

// -- Verbose Extras ------------------------------------------------------------

/// If `verbose`, print the `prio` version/build/target banner shown at the
/// start of every command when `-v`/`--verbose` is given; a no-op
/// otherwise.
pub fn print_verbose_banner(verbose: bool) {
    if !verbose {
        return;
    }
    blank();
    println!("  {} {}", label("prio"), value(env!("CARGO_PKG_VERSION")));
    println!("  {} {}", label("build"), value(env!("PRIO_BUILD_DATE")));
    println!("  {} {}", label("target"), value(env!("PRIO_TARGET")));
    blank();
}

/// Print a verbose-mode diagnostic line showing the target PID and whether
/// the calling process is running as root (relevant since raising priority
/// generally requires `CAP_SYS_NICE`/root).
pub fn print_verbose_pid_check(pid: u32, is_root: bool) {
    println!("  {} {} {} {} {}",
        label("PID:"),
        value(&pid.to_string()),
        ARROW,
        label("root:"),
        if is_root {
            "yes".bright_green()
        } else {
            "no".yellow()
        },
    );
}

/// Print a verbose-mode line noting that the priority change will
/// auto-revert after `duration` (used by `--time`).
pub fn print_waiting_for_exit(duration: std::time::Duration) {
    println!("  {} {}",
        label("Timer:"),
        value(&format!("will revert after {}", format_duration(duration)))
    );
}

// -- Utilities -----------------------------------------------------------------

/// Truncate a string to `max` chars, appending `…` if necessary.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", t)
    }
}
