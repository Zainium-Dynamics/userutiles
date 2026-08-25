use super::core::temperature::{self, status_label};
use crate::theme::{self, Ctx, Tone};

pub fn run(_ctx: &Ctx) {
    theme::title("temp");
    let sensors = temperature::collect();

    theme::header("Thermal Status");
    println!(
        "  {} {} {}",
        theme::paint_pad(Tone::Green, "Sensor", 26),
        theme::paint_pad(Tone::Green, "Temp", 12),
        theme::paint(Tone::Green, "Status")
    );
    println!("  {}", theme::paint(Tone::Green, &"─".repeat(52)));

    let mut any_warn = false;
    for s in &sensors {
        let label = status_label(s.temp, s.critical);
        let tone = match label {
            "Normal" => Tone::Green,
            "Warm" => {
                any_warn = true;
                Tone::Blue
            }
            "Critical" => {
                any_warn = true;
                Tone::Red
            }
            _ => Tone::Black,
        };
        let temp_color = theme::temp_color(s.temp, s.critical);
        println!(
            "  {} {temp_color:<12} {}",
            theme::paint_pad(Tone::Purple, &s.label, 26),
            theme::paint(tone, label)
        );
    }

    theme::divider();
    println!();
    println!("  {}", theme::paint(Tone::Green, "Thermal Map"));
    for s in &sensors {
        let pct = (s.temp as f64 / s.critical.unwrap_or(100.0) as f64) * 100.0;
        let bar = theme::bar(pct.min(100.0), 20);
        println!(
            "  {} {} {:.0}°C",
            theme::paint_pad(Tone::Green, &s.label, 20),
            bar,
            s.temp
        );
    }

    if any_warn {
        theme::warning("Some sensors are running warm — check cooling");
    } else {
        theme::success("All temperatures are within safe limits");
    }
}
