//! Unified-binary entry point for the `trigger` CLI.

use clap::Parser;
use log::info;
use std::process::ExitCode;

use crate::trigger::ListTarget;

#[derive(Parser)]
#[command(
    name = "trigger",
    version,
    about = "Trigger — Universal application and script runner for Zainium OS\n\nEXAMPLES:\n trigger code Launch VS Code\n trigger main.py Run Python script\n trigger list apps List discovered applications\n trigger list handlers List script handlers"
)]
struct Args {
    /// Target to launch or command to run (flag form for compatibility)
    #[arg(long = "trigger", num_args = 1..)]
    trigger_flag: Option<Vec<String>>,

    /// Target to launch or command to run
    #[arg(num_args = 0..)]
    trigger: Vec<String>,

    /// Show detected type and resolved path without executing
    #[arg(long)]
    dry_run: bool,

    /// Show info-level logs and success diagnostics
    #[arg(short, long)]
    verbose: bool,
}

/// Run the `trigger` subcommand. `args` must begin with `argv[0]` (e.g. `"trigger"`).
pub fn run(args: Vec<String>) -> ExitCode {
    crate::sandbox::apply();

    let cli = Args::parse_from(args);

    let default_level = if cli.verbose { "info" } else { "warn" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format_timestamp(None)
        .init();

    let trigger_args = cli.trigger_flag.unwrap_or(cli.trigger);

    if !trigger_args.is_empty() {
        match trigger_args[0].as_str() {
            "list" => {
                if trigger_args.len() < 2 {
                    eprintln!("Error: 'list' requires a target (apps, handlers)");
                    return ExitCode::from(2);
                }
                let target = match trigger_args[1].as_str() {
                    "apps" => ListTarget::Apps,
                    "handlers" => ListTarget::Handlers,
                    _ => {
                        eprintln!(
                            "Error: Invalid list target '{}'. Use: apps, handlers",
                            trigger_args[1]
                        );
                        return ExitCode::from(2);
                    }
                };
                return crate::trigger::list(target);
            }
            _ => {
                return match crate::trigger::run(&trigger_args, cli.dry_run) {
                    Ok(_) => {
                        info!("Execution completed successfully");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{}", e);
                        info!("Execution failed: {}", e);
                        ExitCode::from(e.exit_code())
                    }
                };
            }
        }
    }

    eprintln!("Error: No target specified. Use --help for usage information.");
    ExitCode::from(2)
}
