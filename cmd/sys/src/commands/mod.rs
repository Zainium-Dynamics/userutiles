pub mod sys;

use crate::cli::{Cli, Commands};
use crate::theme::Ctx;

pub fn dispatch(ctx: &Ctx, cli: Cli) {
    match &cli.command {
        Some(Commands::Status) => sys::status::run(ctx),
        Some(Commands::Health) => sys::health::run(ctx),
        Some(Commands::Perf) => sys::perf::run(ctx),
        Some(Commands::Process) => sys::process::run(ctx),
        Some(Commands::Temp) => sys::temp::run(ctx),
        Some(Commands::Power) => sys::power::run(ctx),
        Some(Commands::Services) => sys::services::run(ctx),
        Some(Commands::Optimize) => sys::optimize::run(ctx),
        Some(Commands::Info) => sys::info::run(ctx),
        None => sys::status::run(ctx),
    }
}
