use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sys",
    about = "ZainiumOS System Inspector & Manager",
    long_about = "sys — Real-time hardware monitoring, process management,\nand system optimization for ZainiumOS.",
    version = "0.1.0",
    disable_version_flag = false
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Machine-readable TOML output (user_utils uses .toml only — never JSON)
    #[arg(long, global = true)]
    pub toml: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Full system overview dashboard
    Status,
    /// Complete hardware health report
    Health,
    /// Real-time performance monitor
    Perf,
    /// Advanced process management
    Process,
    /// Temperature & thermal status
    Temp,
    /// Power, battery & energy info
    Power,
    /// System services control
    Services,
    /// Auto system optimization
    Optimize,
    /// Deep system information
    Info,
}
