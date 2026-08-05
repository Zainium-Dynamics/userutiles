mod cli;
mod config;
mod core;
mod error;
mod ui;
mod utils;

use std::process as proc;

use clap::Parser;

use crate::cli::Cli;
use crate::config::Config;
use crate::core::scheduler;
use crate::error::{PrioError, Result};
use crate::ui::display;
use crate::utils::priority::{cpu_level_to_nice, parse_memory, validate_nice, IoMode};
use crate::utils::process::{find_by_pid, find_process, is_root};
use crate::utils::timebound::{parse_duration, schedule_reset};

fn main() {
    if let Err(e) = run() {
        display::print_prio_error(&e);
        proc::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load();

    if cli.is_empty() {
        Cli::parse_from(["prio", "--help"]);
        return Ok(());
    }

    display::print_verbose_banner(cli.verbose);

    // -- --list ----------------------------------------------------------------
    if cli.list {
        let procs = crate::utils::process::get_top_processes(cfg.list.max_processes);
        display::print_process_list(&procs);
        return Ok(());
    }

    // -- --reset <PID> ---------------------------------------------------------
    if let Some(pid) = cli.reset {
        let info = find_by_pid(pid)?;
        let old = info.nice;
        scheduler::reset_pid(pid)?;
        display::print_reset(pid, old);
        return Ok(());
    }

    // -- --boost <PID|CMD> -----------------------------------------------------
    if let Some(ref target) = cli.boost {
        let info = find_process(target)?;
        display::print_boost_start();
        let old_nice = scheduler::boost_pid(info.pid, cfg.defaults.boost_nice)?;
        display::print_boost(&display::BoostArgs {
            pid: info.pid,
            name: &info.name,
            old_nice,
            new_nice: cfg.defaults.boost_nice,
        });
        return Ok(());
    }

    // -- --pid <PID> (apply to existing) --------------------------------------
    if let Some(pid) = cli.pid {
        let nice = resolve_nice(&cli)?;
        let info = find_by_pid(pid)?;
        let io = cli.io.as_deref().map(|s| s.parse::<IoMode>()).transpose()?;

        if cli.verbose {
            display::print_verbose_pid_check(pid, is_root());
        }

        scheduler::apply_to_pid(pid, nice, io.as_ref())?;

        display::print_apply(&display::ApplyArgs {
            pid,
            name: &info.name,
            nice,
            io: cli.io.as_deref(),
        });

        handle_timebound(&cli, pid, info.nice)?;
        return Ok(());
    }

    // -- Spawn a new command ---------------------------------------------------
    if cli.command.is_empty() {
        return Err(PrioError::SpawnError(
            "no command given — provide a COMMAND or use --pid".into(),
        ));
    }

    let nice = resolve_nice(&cli)?;
    let io = cli.io.as_deref().map(|s| s.parse::<IoMode>()).transpose()?;
    let max_ram = cli.max_ram.as_deref().map(parse_memory).transpose()?;
    let dur = cli.time.as_deref().map(parse_duration).transpose()?;

    let cmd_str = cli.command.join(" ");

    // -- Auto mode display -----------------------------------------------------
    if cli.auto {
        display::print_auto_mode(&cmd_str);
    }

    // -- Full-power display ----------------------------------------------------
    let is_full_power = io.is_some() && max_ram.is_some() && dur.is_some();
    if is_full_power {
        let io_str = io
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "Normal".to_string());
        let fp = display::FullPowerArgs {
            command: &cmd_str,
            nice,
            cpu: cli.cpu.unwrap_or(nice_to_cpu(nice)),
            io_mode: &io_str,
            max_ram: max_ram.unwrap(),
            duration: dur.unwrap(),
            pid: 0, // filled after spawn
        };
        // Print headers before spawn so the user sees the config immediately.
        print_full_power_pre(&fp);
    } else if !cli.auto {
        display::print_basic(&display::BasicArgs {
            command: &cmd_str,
            nice,
            cpu: cli.cpu,
            pid: None,
        });
    }

    let spawn_cfg = scheduler::SpawnConfig {
        command: cli.command.clone(),
        nice,
        io_mode: io,
        max_ram,
        auto: cli.auto,
        verbose: cli.verbose,
    };

    let mut child = scheduler::spawn(&spawn_cfg)?;
    let pid = child.id();

    // -- Update display with real PID ------------------------------------------
    if is_full_power {
        display::print_full_power_success(pid, dur.unwrap());
    } else if !cli.auto {
        println!("  {:<14} {}",
            colored::Colorize::bright_green("PID :"),
            colored::Colorize::bright_magenta(&*pid.to_string())
        );
        println!();
        display::print_basic_success();
    }

    // -- Auto monitor ----------------------------------------------------------
    let _monitor = if cli.auto {
        Some(crate::core::monitor::AutoMonitor::start(
            pid, nice, &cfg.auto,
        ))
    } else {
        None
    };

    // -- Time-bound revert -----------------------------------------------------
    if let Some(d) = dur {
        if cli.verbose {
            display::print_waiting_for_exit(d);
        }
        schedule_reset(pid, 0, d);
    }

    // -- Wait ------------------------------------------------------------------
    // Register Ctrl-C handler: on SIGINT, kill child and exit cleanly.
    ctrlc::set_handler(move || {
        // SAFETY: `kill(2)` takes only plain integers (`pid_t`, signal
        // number) and performs no pointer dereferencing, so the call
        // cannot cause UB regardless of whether `pid` still refers to a
        // live process — a stale PID just yields ESRCH, which is ignored
        // here since the handler only wants to *try* to stop the child
        // before the process exits.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
        proc::exit(130);
    })
    .ok();

    let status = child.wait()?;
    proc::exit(status.code().unwrap_or(0));
}

// -- Helper: resolve effective nice -------------------------------------------

fn resolve_nice(cli: &Cli) -> Result<i32> {
    match (cli.nice, cli.cpu) {
        (Some(n), _) => validate_nice(n),
        (None, Some(c)) => cpu_level_to_nice(c),
        _ => Ok(0),
    }
}

/// Reverse-map a nice value back to an approximate CPU% for display only.
fn nice_to_cpu(nice: i32) -> u32 {
    let clamped = nice.clamp(-20, 19) as f64;
    ((19.0 - clamped) * 100.0 / 39.0).round() as u32
}

/// Print the pre-spawn portion of the full-power UI (without PID).
fn print_full_power_pre(args: &display::FullPowerArgs) {
    use colored::Colorize;
    println!();
    println!("{}", "Setting enhanced priority...".bright_cyan().bold());
    println!();
    print_kv("Command", args.command.bright_magenta().to_string());
    print_kv(
        "Niceness",
        crate::utils::priority::format_nice(args.nice)
            .as_str()
            .bright_red()
            .bold()
            .to_string(),
    );
    print_kv("CPU", format!("{}%", args.cpu).bright_magenta().to_string());
    print_kv("I/O", args.io_mode.bright_magenta().to_string());
    print_kv(
        "Max RAM",
        crate::utils::priority::format_memory(args.max_ram)
            .bright_magenta()
            .to_string(),
    );
    print_kv(
        "Duration",
        crate::utils::timebound::format_duration(args.duration)
            .bright_magenta()
            .to_string(),
    );
    println!();
}

fn print_kv(key: &str, val: String) {
    use colored::Colorize;
    println!("  {:<14} {}", format!("{} :", key).bright_green(), val);
}

fn handle_timebound(cli: &Cli, pid: u32, original_nice: i32) -> Result<()> {
    if let Some(ref dur_str) = cli.time {
        let d = parse_duration(dur_str)?;
        if cli.verbose {
            display::print_waiting_for_exit(d);
        }
        schedule_reset(pid, original_nice, d);
    }
    Ok(())
}
