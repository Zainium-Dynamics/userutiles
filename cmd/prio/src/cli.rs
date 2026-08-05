use clap::Parser;

/// ZainiumOS Process Priority & Resource Scheduler.
///
/// `prio` is a premium Zainium OS process scheduler that wraps niceness,
/// I/O priority, cgroup memory limits, and automatic CPU-temperature
/// throttling behind a single, ergonomic command.
#[derive(Parser, Debug, Clone)]
#[command(
 name = "prio",
 version,
 about = "ZainiumOS Process Priority & Resource Scheduler",
 long_about = concat!(
 "prio — ZainiumOS Process Priority & Resource Scheduler\n\n",
 "A premium process scheduler with cgroup memory control,\n",
 "real-time I/O prioritisation, auto-throttling, and time-bound\n",
 "priority management for Zainium OS.",
 ),
 help_template = "\
{name} {version} — ZainiumOS Process Priority & Resource Scheduler

\x1b[1;36mUsage:\x1b[0m
 {name} [OPTIONS] <COMMAND...>
 {name} [OPTIONS] --pid <PID>
 {name} --list
 {name} --boost <PID|NAME>
 {name} --reset <PID>

\x1b[1;36mOptions:\x1b[0m
{options}
",
 override_usage = "prio [OPTIONS] <COMMAND...> | prio [OPTIONS] --pid <PID>",
)]
pub struct Cli {
    /// Set niceness level (-20 = highest priority, +19 = lowest)
    #[arg(
        short = 'n',
        long = "nice",
        value_name = "LEVEL",
        allow_hyphen_values = true,
        help = "Set niceness (-20 highest to +19 lowest)"
    )]
    pub nice: Option<i32>,

    /// CPU priority on a 0-100 scale (mapped to niceness internally)
    #[arg(
        short = 'c',
        long = "cpu",
        value_name = "LEVEL",
        help = "CPU priority (0-100)"
    )]
    pub cpu: Option<u32>,

    /// I/O scheduling class: realtime | high | normal | idle
    #[arg(
        long = "io",
        value_name = "MODE",
        help = "I/O priority (realtime, high, normal, idle)"
    )]
    pub io: Option<String>,

    /// Limit process memory usage (e.g. 4G, 2.5G, 512M)
    #[arg(
        long = "max-ram",
        value_name = "SIZE",
        help = "Limit memory usage (e.g. 4G, 2.5G)"
    )]
    pub max_ram: Option<String>,

    /// Apply priority settings to an existing process by PID
    #[arg(
        long = "pid",
        value_name = "PID",
        help = "Apply settings to an existing PID"
    )]
    pub pid: Option<u32>,

    /// Quickly boost an existing process by PID or name
    #[arg(
        long = "boost",
        value_name = "PID|CMD",
        help = "Quick boost priority of a running process"
    )]
    pub boost: Option<String>,

    /// Reset a process's priority to system default (nice 0)
    #[arg(
        long = "reset",
        value_name = "PID",
        help = "Reset priority to normal (0)"
    )]
    pub reset: Option<u32>,

    /// Enable smart auto-throttling based on CPU temperature and load
    #[arg(long = "auto", help = "Enable smart auto-throttling")]
    pub auto: bool,

    /// Run the priority boost for a fixed duration, then revert
    #[arg(
        long = "time",
        value_name = "DURATION",
        help = "Time-bound boost (e.g. 10m, 2h, 30s)"
    )]
    pub time: Option<String>,

    /// Show the top 15 processes sorted by priority
    #[arg(
        long = "list",
        short = 'l',
        help = "Show top processes with priorities"
    )]
    pub list: bool,

    /// Enable verbose diagnostic output
    #[arg(long = "verbose", short = 'v', help = "Verbose output")]
    pub verbose: bool,

    /// The command (and its arguments) to launch under the configured priority
    #[arg(
        trailing_var_arg = true,
        value_name = "COMMAND",
        help = "Command to run with priority settings"
    )]
    pub command: Vec<String>,
}

impl Cli {
    /// Returns true if no actionable argument was provided.
    pub fn is_empty(&self) -> bool {
        self.nice.is_none()
            && self.cpu.is_none()
            && self.io.is_none()
            && self.max_ram.is_none()
            && self.pid.is_none()
            && self.boost.is_none()
            && self.reset.is_none()
            && !self.auto
            && self.time.is_none()
            && !self.list
            && self.command.is_empty()
    }
}
