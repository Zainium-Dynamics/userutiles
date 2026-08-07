use anyhow::Result;
use colored::*;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::ui::display::{print_header, print_success};

#[derive(Debug)]
struct DeviceHealth {
    name: String,
    temp_celsius: Option<u32>,
    smart_passed: Option<bool>,
    power_on_hours: Option<u64>,
    wear_level: Option<u8>,
}

pub fn run_health(device: Option<&str>, as_toml: bool) -> Result<()> {
    let devices = collect_health(device)?;

    if as_toml {
        #[derive(serde::Serialize)]
        struct Row<'a> {
            device: &'a str,
            temperature_celsius: Option<u32>,
            smart_passed: Option<bool>,
            power_on_hours: Option<u64>,
            wear_level_percent: Option<u8>,
        }
        #[derive(serde::Serialize)]
        struct Out<'a> {
            devices: Vec<Row<'a>>,
        }
        let out = Out {
            devices: devices
                .iter()
                .map(|d| Row {
                    device: &d.name,
                    temperature_celsius: d.temp_celsius,
                    smart_passed: d.smart_passed,
                    power_on_hours: d.power_on_hours,
                    wear_level_percent: d.wear_level,
                })
                .collect(),
        };
        println!("{}", toml::to_string_pretty(&out)?);
        return Ok(());
    }

    print_header("Health Status Report");
    println!();

    println!(
        "  {:<14} {:<16} {:<8} {:<8} {:<12} {:<10} {}",
        "DEVICE".bold().cyan(),
        "STATUS".bold().cyan(),
        "TEMP".bold().cyan(),
        "WEAR".bold().cyan(),
        "POWER-ON".bold().cyan(),
        "SMART".bold().cyan(),
        "OVERALL".bold().cyan(),
    );
    println!("  {}", "─".repeat(82).truecolor(50, 50, 60));

    let mut warnings: Vec<String> = Vec::new();

    for dev in &devices {
        let (status_dot, overall) = health_status(dev);
        let temp_str = format_temp(dev.temp_celsius);
        let wear_str = dev
            .wear_level
            .map(|w| format!("{w}%"))
            .unwrap_or_else(|| "-".to_string());
        let hours_str = dev
            .power_on_hours
            .map(|h| format!("{h}h"))
            .unwrap_or_else(|| "-".to_string());
        let smart_str = match dev.smart_passed {
            Some(true) => "PASSED".bright_green().to_string(),
            Some(false) => "FAILED".bright_red().to_string(),
            None => "N/A".dimmed().to_string(),
        };

        println!(
            "  {:<14} {:<25} {:<17} {:<8} {:<12} {:<19} {}",
            dev.name.bright_blue(),
            status_dot,
            temp_str,
            wear_str.bright_magenta(),
            hours_str.bright_magenta(),
            smart_str,
            overall,
        );

        if let Some(t) = dev.temp_celsius {
            if t >= 45 {
                warnings.push(format!(
                    "{} temperature is elevated ({}°C) — improve cooling",
                    dev.name, t
                ));
            }
        }
        if dev.smart_passed == Some(false) {
            warnings.push(format!(
                "{} SMART self-test FAILED — back up data immediately",
                dev.name
            ));
        }
        if let Some(w) = dev.wear_level {
            if w >= 80 {
                warnings.push(format!(
                    "{} wear level is {}% — consider replacement",
                    dev.name, w
                ));
            }
        }
    }

    println!();

    if warnings.is_empty() {
        print_success("All drives are healthy");
    } else {
        println!("  {} Recommendations:", "⚠".bright_yellow());
        for w in &warnings {
            println!("  {} {}", "-".bright_yellow(), w.bright_yellow());
        }
    }

    println!();
    Ok(())
}

fn collect_health(filter: Option<&str>) -> Result<Vec<DeviceHealth>> {
    let block_dir = Path::new("/sys/block");
    if !block_dir.exists() {
        return Err(anyhow::anyhow!("Cannot access /sys/block"));
    }

    let mut devices = Vec::new();

    for entry in fs::read_dir(block_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
            continue;
        }
        if let Some(f) = filter {
            let clean = f.trim_start_matches("/dev/");
            if name != clean {
                continue;
            }
        }

        let temp = read_hwmon_temp(&name);
        let (smart, poh, wear) = read_smart_data(&name);

        devices.push(DeviceHealth {
            name,
            temp_celsius: temp,
            smart_passed: smart,
            power_on_hours: poh,
            wear_level: wear,
        });
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

fn read_hwmon_temp(name: &str) -> Option<u32> {
    // Try /sys/class/block/<name>/device/hwmon/hwmonX/temp1_input
    let paths = [
        format!("/sys/class/block/{name}/device/hwmon"),
        format!("/sys/class/nvme/{name}/hwmon"),
    ];
    for base in &paths {
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                for i in 1..=4u32 {
                    let p = entry.path().join(format!("temp{i}_input"));
                    if let Ok(v) = fs::read_to_string(&p) {
                        if let Ok(mv) = v.trim().parse::<u64>() {
                            return Some((mv / 1000) as u32);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Returns (smart_passed, power_on_hours, wear_level)
fn read_smart_data(name: &str) -> (Option<bool>, Option<u64>, Option<u8>) {
    let Ok(smartctl) = which::which("smartctl") else {
        return (None, None, None);
    };

    // Text mode only — user_utils never depends on JSON (including smartctl --json).
    let out = Command::new(&smartctl)
        .args(["-A", "-H", &format!("/dev/{name}")])
        .output();

    let Ok(output) = out else {
        return (None, None, None);
    };

    let text = String::from_utf8_lossy(&output.stdout);
    parse_smartctl_text(&text)
}

fn parse_smartctl_text(text: &str) -> (Option<bool>, Option<u64>, Option<u8>) {
    let smart_passed = if text.contains("PASSED")
        || text.contains("SMART overall-health self-assessment test result: PASSED")
    {
        Some(true)
    } else if text.contains("FAILED") {
        Some(false)
    } else if text.contains("OK") {
        Some(true)
    } else {
        None
    };

    let mut power_on_hours = None;
    let mut wear = None;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("power_on_hours") || lower.contains("power on hours") {
            if let Some(n) = line
                .split_whitespace()
                .rev()
                .find_map(|t| t.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
            {
                power_on_hours = Some(n);
            }
        }
        if lower.contains("percentage_used") || lower.contains("percentage used") {
            if let Some(n) = line.split_whitespace().rev().find_map(|t| {
                t.trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<u64>()
                    .ok()
            }) {
                wear = Some(n.min(100) as u8);
            }
        }
    }

    (smart_passed, power_on_hours, wear)
}

fn health_status(dev: &DeviceHealth) -> (String, String) {
    let crit = dev.smart_passed == Some(false);
    let warn_temp = dev.temp_celsius.map(|t| t >= 45).unwrap_or(false);
    let high_temp = dev.temp_celsius.map(|t| t >= 55).unwrap_or(false);
    let high_wear = dev.wear_level.map(|w| w >= 80).unwrap_or(false);

    if crit || high_temp {
        (
            format!("● {}", "Critical".bright_red()),
            "Immediate attention".bright_red().to_string(),
        )
    } else if warn_temp || high_wear {
        (
            format!("● {}", "Warning".bright_yellow()),
            "Attention needed".bright_yellow().to_string(),
        )
    } else if dev.temp_celsius.map(|t| t >= 35).unwrap_or(false) {
        (
            format!("● {}", "Fair".truecolor(255, 165, 0)),
            "Normal".truecolor(255, 165, 0).to_string(),
        )
    } else {
        (
            format!("● {}", "Excellent".bright_green()),
            "Optimal".bright_green().to_string(),
        )
    }
}

fn format_temp(temp: Option<u32>) -> String {
    match temp {
        Some(t) if t >= 55 => format!("{t}°C").bright_red().to_string(),
        Some(t) if t >= 45 => format!("{t}°C").bright_yellow().to_string(),
        Some(t) => format!("{t}°C").bright_magenta().to_string(),
        None => "-".dimmed().to_string(),
    }
}

// --- Unit tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dev(temp: Option<u32>, smart: Option<bool>, wear: Option<u8>) -> DeviceHealth {
        DeviceHealth {
            name: "test0".to_string(),
            temp_celsius: temp,
            smart_passed: smart,
            power_on_hours: None,
            wear_level: wear,
        }
    }

    #[test]
    fn excellent_when_cool_and_smart_ok() {
        let dev = make_dev(Some(30), Some(true), Some(5));
        let (_, overall) = health_status(&dev);
        assert!(overall.contains("Optimal"), "got: {overall}");
    }

    #[test]
    fn warning_when_temp_elevated() {
        let dev = make_dev(Some(48), Some(true), None);
        let (_, overall) = health_status(&dev);
        assert!(overall.contains("Attention"), "got: {overall}");
    }

    #[test]
    fn critical_when_smart_failed() {
        let dev = make_dev(Some(30), Some(false), None);
        let (_, overall) = health_status(&dev);
        assert!(overall.contains("attention"), "got: {overall}");
    }

    #[test]
    fn critical_when_temp_very_high() {
        let dev = make_dev(Some(60), Some(true), None);
        let (_, overall) = health_status(&dev);
        assert!(overall.contains("attention"), "got: {overall}");
    }

    #[test]
    fn warning_when_wear_high() {
        let dev = make_dev(Some(30), Some(true), Some(85));
        let (_, overall) = health_status(&dev);
        assert!(overall.contains("Attention"), "got: {overall}");
    }

    #[test]
    fn fair_when_temp_slightly_elevated() {
        let dev = make_dev(Some(38), Some(true), None);
        let (_, overall) = health_status(&dev);
        assert!(overall.contains("Normal"), "got: {overall}");
    }

    #[test]
    fn format_temp_none_is_dash() {
        let s = format_temp(None);
        // strip ANSI for assertion
        assert!(s.contains('-'));
    }

    #[test]
    fn format_temp_normal_range() {
        let s = format_temp(Some(32));
        assert!(s.contains("32°C"));
    }
}
