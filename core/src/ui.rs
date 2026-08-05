//! Zainium cyber-tech terminal UI helpers.
//!
//! Data tools (cat, head, …) keep stdout clean. Status/errors go to stderr
//! with this palette so shells and pipelines stay correct.

use std::io::{self, IsTerminal, Write};

use colored::Colorize;

/// Shared UI printer. Colour is auto-disabled when stderr is not a TTY
/// or when `NO_COLOR` is set.
pub struct Ui {
    color: bool,
    prog: String,
}

impl Ui {
    pub fn new(prog: impl Into<String>) -> Self {
        let color = stderr_wants_color();
        Self {
            color,
            prog: prog.into(),
        }
    }

    pub fn with_color(prog: impl Into<String>, color: bool) -> Self {
        Self {
            color,
            prog: prog.into(),
        }
    }

    pub fn prog(&self) -> &str {
        &self.prog
    }

    /// Top-level heading (cyan).
    pub fn heading(&self, s: &str) {
        if self.color {
            eprintln!("{}", s.bright_cyan().bold());
        } else {
            eprintln!("{s}");
        }
    }

    /// Key–value row: green label, magenta value.
    pub fn kv(&self, label: &str, value: &str) {
        if self.color {
            eprintln!(
                "  {:<14} {}",
                format!("{label}  :").bright_green(),
                value.bright_magenta()
            );
        } else {
            eprintln!("  {label:<14}: {value}");
        }
    }

    pub fn ok(&self, msg: &str) {
        if self.color {
            eprintln!("  {} {}", "✓".bright_green().bold(), msg.bright_green());
        } else {
            eprintln!("  ✓ {msg}");
        }
    }

    pub fn warn(&self, msg: &str) {
        if self.color {
            eprintln!("  {} {}", "⚠".bright_yellow().bold(), msg.bright_yellow());
        } else {
            eprintln!("  ⚠ {msg}");
        }
    }

    pub fn err(&self, msg: &str) {
        if self.color {
            eprintln!(
                "{}: {} {}",
                self.prog.bright_red(),
                "✖".bright_red().bold(),
                msg.bright_red()
            );
        } else {
            eprintln!("{}: ✖ {msg}", self.prog);
        }
    }

    pub fn info(&self, msg: &str) {
        if self.color {
            eprintln!("  {}", msg.truecolor(160, 220, 255));
        } else {
            eprintln!("  {msg}");
        }
    }

    pub fn separator(&self) {
        if self.color {
            eprintln!("{}", "  ─────────────────────────────────────────".dimmed());
        } else {
            eprintln!("  -----------------------------------------");
        }
    }

    /// Print usage banner for a utility.
    pub fn usage_banner(&self, summary: &str) {
        eprintln!();
        if self.color {
            eprintln!(
                "  {} — {}",
                self.prog.bold().bright_cyan(),
                summary.truecolor(160, 220, 255)
            );
        } else {
            eprintln!("  {} — {summary}", self.prog);
        }
        eprintln!();
    }
}

fn stderr_wants_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if matches!(
        std::env::var("TERM").as_deref(),
        Ok("dumb") | Ok("") | Err(_)
    ) {
        return io::stderr().is_terminal()
            && std::env::var_os("TERM")
                .map(|t| !t.is_empty())
                .unwrap_or(false);
    }
    io::stderr().is_terminal()
}

/// Flush stdout; map broken pipe to success (pipeline-friendly).
pub fn flush_stdout() -> io::Result<()> {
    let mut out = io::stdout().lock();
    match out.flush() {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

/// Write all bytes to stdout; treat broken pipe as success for exit code 0.
pub fn write_stdout(bytes: &[u8]) -> io::Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(bytes) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}
