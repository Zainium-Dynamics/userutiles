use super::core::{system, temperature};
use crate::theme::{self, Ctx, Tone};
use sysinfo::{DiskExt, System, SystemExt};

pub fn run(_ctx: &Ctx) {
    let sys_info = system::collect();
    let temps = temperature::collect();

    theme::title("status");
    theme::header("System Overview");
    theme::kv_row("Hostname", &sys_info.hostname);
    theme::kv_row("OS", &sys_info.os_name);
    theme::kv_row("Kernel", &sys_info.kernel_version);
    theme::kv_row("Uptime", &system::format_uptime(sys_info.uptime_secs));
    println!();

    let cpu_bar = theme::bar(sys_info.cpu_usage as f64, 18);
    println!(
        "  {} {} {}C/{}T {}",
        theme::paint_pad(Tone::Green, "CPU", 18),
        theme::paint(Tone::Purple, &sys_info.cpu_name),
        sys_info.cpu_cores,
        sys_info.cpu_threads,
        theme::paint(Tone::Blue, &format!("{}% load", sys_info.cpu_usage as u32))
    );
    println!("  {:<18} {}", "", cpu_bar);

    let mem_bar = theme::bar(sys_info.memory_percent, 18);
    println!(
        "  {} {:.1} GiB / {:.1} GiB {}",
        theme::paint_pad(Tone::Green, "Memory", 18),
        sys_info.used_memory_gib,
        sys_info.total_memory_gib,
        theme::paint(
            Tone::Blue,
            &format!("({:.0}% used)", sys_info.memory_percent)
        )
    );
    println!("  {:<18} {}", "", mem_bar);

    let swap_bar = theme::bar(sys_info.swap_percent, 18);
    println!(
        "  {} {:.1} GiB / {:.1} GiB {}",
        theme::paint_pad(Tone::Green, "Swap", 18),
        sys_info.used_swap_gib,
        sys_info.total_swap_gib,
        theme::paint(Tone::Blue, &format!("({:.0}% used)", sys_info.swap_percent))
    );
    println!("  {:<18} {}", "", swap_bar);

    let mut sys = System::new();
    sys.refresh_disks_list();
    sys.refresh_disks();
    let (mut total_b, mut used_b) = (0u64, 0u64);
    for d in sys.disks() {
        total_b += d.total_space();
        used_b += d.total_space() - d.available_space();
    }
    let tb = 1_099_511_627_776.0_f64;
    let disk_pct = if total_b > 0 {
        (used_b as f64 / total_b as f64) * 100.0
    } else {
        0.0
    };
    let disk_bar = theme::bar(disk_pct, 18);
    println!(
        "  {} {:.1} TB / {:.1} TB {}",
        theme::paint_pad(Tone::Green, "Disk", 18),
        used_b as f64 / tb,
        total_b as f64 / tb,
        theme::paint(Tone::Blue, &format!("({:.0}% used)", disk_pct))
    );
    println!("  {:<18} {}", "", disk_bar);
    println!();

    let temp_parts: Vec<String> = temps
        .iter()
        .take(4)
        .map(|t| format!("{} {}", t.label, theme::temp_color(t.temp, t.critical)))
        .collect();
    println!(
        "  {} {}",
        theme::paint_pad(Tone::Green, "Temperatures", 18),
        temp_parts.join("  •  ")
    );

    theme::divider();
    let stressed = temps
        .iter()
        .any(|t| t.temp >= t.critical.unwrap_or(100.0) - 10.0)
        || sys_info.memory_percent > 85.0
        || sys_info.cpu_usage > 90.0;
    if stressed {
        theme::warning("System is under stress — check health report");
    } else {
        theme::success("System is stable and performing optimally");
    }
}
