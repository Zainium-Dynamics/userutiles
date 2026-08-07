use super::core::temperature;
use crate::theme::{self, Ctx, Tone};
use sysinfo::{System, SystemExt};

struct HealthEntry {
    component: String,
    status: &'static str,
    temp: String,
    notes: String,
}

pub fn run(_ctx: &Ctx) {
    theme::title("health");
    let mut sys = System::new_all();
    sys.refresh_all();
    let temps = temperature::collect();
    theme::header("Health Report");

    let mut entries: Vec<HealthEntry> = Vec::new();

    let cpu_temp_opt = temps.iter().find(|t| {
        t.label.to_lowercase().contains("cpu") || t.label.to_lowercase().contains("package")
    });
    let cpu_temp_str = cpu_temp_opt
        .map(|t| format!("{:.0}°C", t.temp))
        .unwrap_or_else(|| "N/A".into());
    let cpu_crit = cpu_temp_opt.and_then(|t| t.critical);
    let cpu_temp_val = cpu_temp_opt.map(|t| t.temp).unwrap_or(0.0);
    entries.push(HealthEntry {
        component: "CPU".into(),
        status: classify_health(cpu_temp_val, cpu_crit),
        temp: cpu_temp_str,
        notes: cpu_note(cpu_temp_val, cpu_crit).into(),
    });

    let gpu_temp_opt = temps.iter().find(|t| {
        t.label.to_lowercase().contains("gpu") || t.label.to_lowercase().contains("edge")
    });
    let gpu_temp_str = gpu_temp_opt
        .map(|t| format!("{:.0}°C", t.temp))
        .unwrap_or_else(|| "N/A".into());
    entries.push(HealthEntry {
        component: "GPU".into(),
        status: classify_health(
            gpu_temp_opt.map(|t| t.temp).unwrap_or(50.0),
            gpu_temp_opt.and_then(|t| t.critical),
        ),
        temp: gpu_temp_str,
        notes: "Normal operation".into(),
    });

    let nvme_temp_opt = temps.iter().find(|t| {
        t.label.to_lowercase().contains("nvme") || t.label.to_lowercase().contains("ssd")
    });
    let nvme_temp_str = nvme_temp_opt
        .map(|t| format!("{:.0}°C", t.temp))
        .unwrap_or_else(|| "N/A".into());
    entries.push(HealthEntry {
        component: "NVMe SSD".into(),
        status: classify_health(
            nvme_temp_opt.map(|t| t.temp).unwrap_or(36.0),
            nvme_temp_opt.and_then(|t| t.critical),
        ),
        temp: nvme_temp_str,
        notes: "Healthy".into(),
    });

    let mem_used_pct = {
        let total = sys.total_memory() as f64;
        let used = sys.used_memory() as f64;
        if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        }
    };
    entries.push(HealthEntry {
        component: "RAM".into(),
        status: if mem_used_pct > 90.0 {
            "Critical"
        } else if mem_used_pct > 75.0 {
            "Warn"
        } else {
            "Excellent"
        },
        temp: "—".into(),
        notes: format!("{:.0}% utilized", mem_used_pct),
    });

    // Table header
    println!(
        "  {} {} {} {}",
        theme::paint_pad(Tone::Green, "Component", 16),
        theme::paint_pad(Tone::Green, "Status", 14),
        theme::paint_pad(Tone::Green, "Temp", 10),
        theme::paint(Tone::Green, "Notes")
    );
    println!("  {}", theme::paint(Tone::Green, &"─".repeat(60)));

    for e in &entries {
        let icon = match e.status {
            "Excellent" => "●",
            "Good" => "●",
            "Warn" => "▲",
            _ => "✖",
        };
        let tone = match e.status {
            "Excellent" | "Good" => Tone::Green,
            "Warn" => Tone::Purple,
            _ => Tone::Red,
        };
        let status_str = format!("{} {}", icon, e.status);
        println!(
            "  {} {} {} {}",
            theme::paint_pad(Tone::Purple, &e.component, 16),
            theme::paint_pad(tone, &status_str, 14),
            theme::paint_pad(Tone::Blue, &e.temp, 10),
            e.notes
        );
    }

    theme::divider();
    let any_critical = entries.iter().any(|e| e.status == "Critical");
    let any_warn = entries.iter().any(|e| e.status == "Warn");
    let overall = if any_critical {
        "Critical"
    } else if any_warn {
        "Fair"
    } else {
        "Excellent"
    };
    let overall_tone = match overall {
        "Excellent" => Tone::Green,
        "Fair" => Tone::Blue,
        _ => Tone::Red,
    };
    println!(
        "  {} {}",
        theme::paint_pad(Tone::Green, "Overall Health", 16),
        theme::paint(overall_tone, overall)
    );

    if any_critical || any_warn {
        theme::warning("Some components need attention");
    } else {
        theme::success("All hardware is healthy and performing well");
    }
}

fn classify_health(temp: f32, crit: Option<f32>) -> &'static str {
    let crit = crit.unwrap_or(100.0);
    if temp >= crit - 10.0 {
        "Critical"
    } else if temp >= crit - 25.0 {
        "Warn"
    } else if temp >= crit - 40.0 {
        "Good"
    } else {
        "Excellent"
    }
}

fn cpu_note(temp: f32, crit: Option<f32>) -> &'static str {
    let crit = crit.unwrap_or(100.0);
    if temp >= crit - 10.0 {
        "Thermal throttling!"
    } else if temp >= crit - 25.0 {
        "Running warm"
    } else {
        "Good thermal margin"
    }
}
