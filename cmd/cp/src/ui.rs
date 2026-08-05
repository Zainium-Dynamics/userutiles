// ui.rs — Terminal output helpers, Zainium cyber-tech aesthetic.
//
// Color mapping (contributing.md):
// Headings → Bright Cyan
// Labels → Soft Green
// Values → Bright Magenta / Purple
// Warning → Bright Yellow
// Success (✓) → Bright Green
// Error (✖) → Bright Red
// Standard output → Default white

use colored::Colorize;

/// Print a top-level operation heading.
#[inline]
pub fn heading(s: &str) {
    println!("{}", s.bright_cyan());
}

/// Print an aligned key-value pair.
/// E.g. "  Source        : /home/ali/file.txt"
pub fn kv(label: &str, value: &str) {
    println!("  {:14}: {}", label.green(), value.bright_magenta());
}

/// Print a success line with leading tick.
#[inline]
pub fn ok(msg: &str) {
    println!("{} {}", "✓".bright_green(), msg.bright_green());
}

/// Print a non-fatal warning.
#[inline]
pub fn warn(msg: &str) {
    eprintln!("{} {}", "⚠".bright_yellow(), msg.bright_yellow());
}

/// Print an informational note (yellow).
#[inline]
pub fn info(msg: &str) {
    println!("  {}", msg.yellow());
}

/// Print a non-fatal per-source error (keeps processing remaining sources).
#[inline]
pub fn err(msg: &str) {
    eprintln!("{} {}", "✖".bright_red(), msg.bright_red());
}

/// Print an error and exit with code 1. Only used for top-level usage
/// errors detected before any copying has started.
#[inline]
pub fn fatal(msg: &str) -> ! {
    eprintln!("{} {}", "✖".bright_red(), msg.bright_red());
    std::process::exit(1);
}
