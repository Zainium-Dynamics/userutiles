//! user_trace — process & syscall inspector for Zainium OS (Zainium Dynamics).

pub mod cli;
pub mod detector;
pub mod output;
pub mod sandbox;
pub mod tracer;
pub mod utils;

pub use cli::Cli;
pub use detector::{KernelInfo, OsInfo};
pub use output::OutputFormat;
pub use sandbox::{drop_privileges, verify_permissions};
pub use tracer::TraceData;
pub use utils::{TraceError, TraceResult};

use clap::Parser;
use colored::Colorize;
use log::info;

/// Multicall / binary entry point.
pub fn run() -> i32 {
    user_seccomp::apply();
    let cli = Cli::parse();

    utils::init_logger(cli.verbose);

    if let Err(e) = cli.validate() {
        // Subcommands info/processes skip the process/pid requirement — validate already allows that.
        eprintln!("{} {}", "✖".red(), e);
        return 1;
    }

    if let Err(e) = sandbox::verify_permissions() {
        eprintln!("{} {}", "✖".red(), e);
        return 1;
    }

    if let Some(cmd) = &cli.command {
        return match cmd {
            cli::Commands::Info => match handle_info() {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{} {}", "✖".red(), e);
                    1
                }
            },
            cli::Commands::Processes => match handle_processes() {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{} {}", "✖".red(), e);
                    1
                }
            },
        };
    }

    let target_pid = match (&cli.process, &cli.pid) {
        (Some(name), _) => match tracer::ProcessInfo::from_name(name) {
            Ok(proc) => proc.pid,
            Err(_) => {
                eprintln!(
                    "\n{} Process '{}' not found or not running.\n",
                    "✖".red(),
                    name.cyan()
                );
                eprintln!("   Quick fixes:");
                eprintln!("   - Check process name spelling");
                eprintln!("   - Use PID instead: {} --pid 1234", "trace".bold());
                eprintln!(
                    "   - List running processes: {} processes\n",
                    "trace".bold()
                );
                return 1;
            }
        },
        (_, Some(pid)) => {
            if !tracer::ProcessInfo::exists(*pid) {
                eprintln!(
                    "\n{} Process with PID {} not found.\n",
                    "✖".red(),
                    pid.to_string().cyan()
                );
                eprintln!("   Quick fixes:");
                eprintln!("   - Verify the PID is correct");
                eprintln!("   - List running processes: {} processes", "trace".bold());
                eprintln!("   - Check your permissions (may need elevate)\n");
                return 1;
            }
            *pid
        }
        _ => {
            eprintln!("\n{} No process specified.\n", "✖".red());
            eprintln!("   Usage:");
            eprintln!(
                "   - {} --process <name>  (trace by process name)",
                "trace".bold()
            );
            eprintln!(
                "   - {} --pid <PID>       (trace by process ID)",
                "trace".bold()
            );
            eprintln!(
                "   - {} info              (show system info)",
                "trace".bold()
            );
            eprintln!(
                "   - {} processes         (list running processes)\n",
                "trace".bold()
            );
            return 1;
        }
    };

    match TraceData::collect(target_pid) {
        Ok(data) => {
            let output_format = cli.get_output_format();
            match output_format.format(&data) {
                Ok(formatted) => {
                    println!("{formatted}");

                    if let Some(output_dir) = &cli.output {
                        let filename =
                            utils::generate_filename(&data.process.name, output_format.extension());
                        match utils::write_output_file(output_dir, &filename, &formatted) {
                            Ok(path) => {
                                println!("Output saved: {}", path.green());
                            }
                            Err(e) => {
                                eprintln!("\n{} Failed to save output: {}", "⚠".yellow(), e);
                            }
                        }
                    }
                    info!("Trace completed successfully for PID {target_pid}");
                    0
                }
                Err(e) => {
                    eprintln!("\n{} Failed to format output: {}\n", "✖".red(), e);
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("\n{} Failed to collect trace data: {}\n", "✖".red(), e);
            1
        }
    }
}

fn handle_info() -> TraceResult<()> {
    println!("\n{}\n", "System Information".green().bold().underline());

    match OsInfo::detect() {
        Ok(os) => {
            println!("OS Name: {}", os.name);
            println!("OS Version: {}", os.version);
            println!("Distro: {}", os.distro);
        }
        Err(e) => {
            eprintln!("Failed to detect OS: {e}");
        }
    }

    match KernelInfo::detect() {
        Ok(kernel) => {
            println!("Kernel Version: {}", kernel.version);
            println!("Architecture: {}", kernel.arch);
        }
        Err(e) => {
            eprintln!("Failed to detect kernel: {e}");
        }
    }

    println!();
    Ok(())
}

fn handle_processes() -> TraceResult<()> {
    println!("\n{}\n", "Running Processes".green().bold().underline());

    if let Ok(procs) = procfs::process::all_processes() {
        for proc in procs.take(20).flatten() {
            if let Ok(stat) = proc.stat() {
                println!("  {} - {} (UID: {})", proc.pid(), stat.comm, proc.uid()?);
            }
        }
        println!("\n(Showing first 20 processes)\n");
        Ok(())
    } else {
        Err(TraceError::IoError("Failed to read processes".to_string()))
    }
}
