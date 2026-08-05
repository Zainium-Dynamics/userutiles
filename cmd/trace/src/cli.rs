use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "trace",
    about = "Process & syscall inspector for Zainium OS",
    long_about = "trace — Zainium Dynamics process analysis tool.\n\
                  Real-time process stats, syscall summary, memory/CPU/network.",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Trace a process by name
    #[arg(short = 'p', long)]
    pub process: Option<String>,

    /// Trace a process by PID
    #[arg(long)]
    pub pid: Option<u32>,

    /// Enable live tracing output
    #[arg(short, long)]
    pub live: bool,

    /// Output format: table (default) or toml
    #[arg(short, long)]
    pub format: Option<String>,

    /// Machine-readable TOML output (same as --format toml)
    #[arg(long)]
    pub toml: bool,

    /// Save output to directory
    #[arg(short, long)]
    pub output: Option<String>,

    /// Verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show system information
    Info,

    /// List running processes
    Processes,
}

impl Cli {
    pub fn validate(&self) -> crate::utils::TraceResult<()> {
        if self.pid.is_none() && self.process.is_none() && self.command.is_none() {
            return Err(crate::utils::TraceError::ConfigError(
                "Must specify either --process <name> or --pid <pid>".to_string(),
            ));
        }

        if let Some(format) = &self.format {
            match format.as_str() {
                "table" | "toml" => {}
                "json" | "yaml" => {
                    return Err(crate::utils::TraceError::ConfigError(
                        "JSON/YAML removed — use --format toml (user_utils is TOML-only)".into(),
                    ));
                }
                _ => {
                    return Err(crate::utils::TraceError::ConfigError(format!(
                        "Invalid format: {format}. Must be table or toml"
                    )));
                }
            }
        }

        Ok(())
    }

    pub fn get_output_format(&self) -> crate::output::OutputFormat {
        if self.toml {
            return crate::output::OutputFormat::Toml;
        }
        match self.format.as_deref() {
            Some("toml") => crate::output::OutputFormat::Toml,
            _ => crate::output::OutputFormat::Table,
        }
    }
}
