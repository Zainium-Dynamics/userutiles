//! theme.rs — Unified 5-tone ANSI theme for Zex sys driver.
//!
//! Identical palette to install/search/refresh drivers:
//!   Green  (\x1b[32m) — action words, labels, section markers
//!   Purple (\x1b[35m) — values, names, commands
//!   Blue   (\x1b[36m) — numbers, versions, IDs
//!   Black  (no code)  — dim/secondary text
//!   Red    (\x1b[31m) — errors, critical alerts

use std::io::IsTerminal;

#[derive(Clone, Copy)]
pub enum Tone {
    Green,
    Purple,
    Blue,
    Black,
    Red,
}

fn color_on() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Ok(v) = std::env::var("ZEX_UI_COLOR") {
        return matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }
    std::env::var_os("ZEX_FORCE_COLOR").is_some() || std::io::stdout().is_terminal()
}

fn code(t: Tone) -> Option<&'static str> {
    match t {
        Tone::Green => Some("32"),
        Tone::Purple => Some("35"),
        Tone::Blue => Some("36"),
        Tone::Red => Some("31"),
        Tone::Black => None,
    }
}

/// Wrap `s` in the ANSI SGR code for `t`, unless color is disabled (see
/// [`color_on`]) or `t` is [`Tone::Black`] (which has no dedicated code).
pub fn paint(t: Tone, s: &str) -> String {
    if !color_on() {
        return s.to_string();
    }
    match code(t) {
        Some(c) => format!("\x1b[{}m{}\x1b[0m", c, s),
        None => s.to_string(),
    }
}

/// Paint with left-padding applied BEFORE ANSI wrapping (correct column alignment).
pub fn paint_pad(t: Tone, s: &str, width: usize) -> String {
    paint(t, &format!("{:<width$}", s))
}

#[allow(dead_code)]
pub fn line(parts: &[(Tone, &str)]) -> String {
    parts.iter().map(|(t, s)| paint(*t, s)).collect()
}

pub fn kv_row(label: &str, value: &str) {
    println!(
        "  {} {}",
        paint_pad(Tone::Green, label, 18),
        paint(Tone::Purple, value)
    );
}

#[allow(dead_code)]
pub fn kv_row_blue(label: &str, value: &str) {
    println!(
        "  {} {}",
        paint_pad(Tone::Green, label, 18),
        paint(Tone::Blue, value)
    );
}

pub fn row_plain(label: &str, value: &str) {
    println!("  {} {}", paint_pad(Tone::Green, label, 18), value);
}

pub fn title(subtitle: &str) {
    println!();
    println!(
        "  {} {}",
        paint(Tone::Green, "ZainiumOS System Inspector"),
        paint(Tone::Black, &format!("— {}", subtitle))
    );
    println!("  {}", paint(Tone::Green, &"═".repeat(52)));
}

pub fn header(t: &str) {
    println!();
    println!("  {}", paint(Tone::Green, t));
    println!("  {}", paint(Tone::Black, &"─".repeat(52)));
}

/// Render a `width`-cell filled/empty progress bar for `percent` (0-100),
/// tinted red/purple/green by how full it is. `percent` is clamped to
/// `[0, 100]` first so a caller passing a slightly-out-of-range float
/// (e.g. `100.4` from a rounding computation upstream) can't produce a bar
/// longer or shorter than `width` cells.
pub fn bar(percent: f64, width: usize) -> String {
    let percent = percent.clamp(0.0, 100.0);
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let s = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    if percent >= 85.0 {
        paint(Tone::Red, &s)
    } else if percent >= 60.0 {
        paint(Tone::Purple, &s)
    } else {
        paint(Tone::Green, &s)
    }
}

/// Format `temp` as `"NN°C"`, tinted red/purple/green by how close it is
/// to `crit` (the critical threshold, defaulting to 100°C if unknown).
pub fn temp_color(temp: f32, crit: Option<f32>) -> String {
    let crit = crit.unwrap_or(100.0);
    let s = format!("{:.0}°C", temp);
    if temp >= crit - 10.0 {
        paint(Tone::Red, &s)
    } else if temp >= crit - 30.0 {
        paint(Tone::Purple, &s)
    } else {
        paint(Tone::Green, &s)
    }
}

pub fn success(msg: &str) {
    println!();
    println!("  {} {}", paint(Tone::Green, "✓"), paint(Tone::Green, msg));
    println!();
}

pub fn warning(msg: &str) {
    println!();
    println!("  {} {}", paint(Tone::Red, "⚠"), paint(Tone::Red, msg));
    println!();
}

pub fn divider() {
    println!("  {}", paint(Tone::Black, &"─".repeat(52)));
}

#[allow(dead_code)]
pub struct Ctx {
    pub verbose: bool,
    /// When true, commands emit machine-readable TOML instead of the TUI.
    pub toml: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_empty() {
        std::env::set_var("NO_COLOR", "1");
        let b = bar(0.0, 10);
        assert_eq!(b, "░░░░░░░░░░");
    }

    #[test]
    fn test_bar_full() {
        std::env::set_var("NO_COLOR", "1");
        let b = bar(100.0, 10);
        assert_eq!(b, "██████████");
    }

    #[test]
    fn test_paint_no_color() {
        std::env::set_var("NO_COLOR", "1");
        assert_eq!(paint(Tone::Green, "hello"), "hello");
        assert_eq!(paint(Tone::Red, "error"), "error");
    }

    #[test]
    fn test_bar_clamps_out_of_range_percent() {
        std::env::set_var("NO_COLOR", "1");
        // Regression: an out-of-range percent (e.g. from an upstream
        // rounding computation) must not produce a bar longer/shorter
        // than `width` cells.
        assert_eq!(bar(150.0, 10), "██████████");
        assert_eq!(bar(-20.0, 10), "░░░░░░░░░░");
    }

    #[test]
    fn test_temp_color_no_color_formats_celsius() {
        std::env::set_var("NO_COLOR", "1");
        assert_eq!(temp_color(42.4, Some(90.0)), "42°C");
    }
}
