// lib.rs — Re-export modules; multicall entry is `run() -> i32`.

pub mod cli;
pub mod error;
pub mod metadata;
pub mod ops;
pub mod progress;
pub mod run;
pub mod trash;
pub mod ui;
pub mod undo;
pub mod verify;

pub use run::run as execute;

/// Multicall / binary entry (program name forced to `mv`).
pub fn run() -> i32 {
    let mut args: Vec<String> = std::env::args().collect();
    if let Some(a0) = args.first_mut() {
        *a0 = "mv".into();
    }
    execute(args);
    0
}
