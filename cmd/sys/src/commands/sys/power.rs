use super::core::power;
use crate::theme::{self, Ctx, Tone};

pub fn run(_ctx: &Ctx) {
    theme::title("power");
    let info = power::collect();

    theme::header("Power Management");

    let source = if info.ac_online {
        theme::paint(Tone::Green, "AC Adapter")
    } else {
        theme::paint(Tone::Blue, "Battery")
    };
    theme::row_plain("Power Source", &source);

    if info.battery_present {
        if let Some(pct) = info.battery_percent {
            let bat_bar = theme::bar(pct as f64, 18);
            println!("  {} {}% {}",
                theme::paint_pad(Tone::Green, "Battery", 18),
                pct,
                bat_bar
            );
        }
        if let Some(status) = &info.battery_status {
            theme::kv_row("Battery Status", status);
        }
    } else {
        println!("  {} Not present",
            theme::paint_pad(Tone::Green, "Battery", 18)
        );
    }

    println!();
    if let Some(freq) = info.cpu_freq_ghz {
        theme::kv_row("CPU Frequency", &format!("{:.2} GHz", freq));
    }

    if let Some(gov) = &info.governor {
        let gov_tone = match gov.as_str() {
            "performance" => Tone::Green,
            "powersave" => Tone::Blue,
            "schedutil" => Tone::Purple,
            _ => Tone::Blue,
        };
        theme::row_plain("Power Profile", &theme::paint(gov_tone, gov));
        let suggestion = match gov.as_str() {
            "performance" => Some("Balanced (saves energy)"),
            "powersave" => Some("Performance (if plugged in)"),
            _ => None,
        };
        if let Some(s) = suggestion {
            println!("  {} {}", theme::paint_pad(Tone::Green, "Suggested", 18), s);
        }
    }

    println!();
    theme::divider();
    if info.ac_online {
        theme::success("System is on AC power — performance mode available");
    } else {
        theme::warning("On battery — consider switching to power-save mode");
    }
}
