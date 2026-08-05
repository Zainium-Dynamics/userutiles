//! user sys — ZainiumOS System Inspector & Manager.

mod cli;
mod commands;
mod theme;

use clap::Parser;
use cli::Cli;
use theme::Ctx;

/// Entry point for standalone binary and multicall (`user_utils sys …`).
pub fn run() -> i32 {
    let cli = Cli::parse();
    let ctx = Ctx {
        verbose: cli.verbose,
        toml: cli.toml,
    };
    commands::dispatch(&ctx, cli);
    0
}
