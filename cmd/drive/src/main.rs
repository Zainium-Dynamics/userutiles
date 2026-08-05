mod cli;
mod config;
mod core;
mod error;
mod ui;
mod utils;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run()
}
