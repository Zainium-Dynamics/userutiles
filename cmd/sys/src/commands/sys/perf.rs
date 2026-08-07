use super::core::{cpu, memory};
use crate::theme::{self, Ctx, Tone};
use sysinfo::{ProcessExt, System, SystemExt};

pub fn run(_ctx: &Ctx) {
    theme::title("perf");
    let cpu_info = cpu::collect();
    let mem_info = memory::collect();
    let mut sys = System::new_all();
    sys.refresh_all();

    theme::header("Live Performance");

    let cpu_bar = theme::bar(cpu_info.usage as f64, 20);
    println!(
        "  {} {:>5}% {}",
        theme::paint_pad(Tone::Green, "CPU Usage", 16),
        format!("{:.0}", cpu_info.usage),
        cpu_bar
    );

    let mem_bar = theme::bar(mem_info.percent, 20);
    println!(
        "  {} {:>5}% {}",
        theme::paint_pad(Tone::Green, "Memory", 16),
        format!("{:.0}", mem_info.percent),
        mem_bar
    );

    let swap_bar = theme::bar(mem_info.swap_percent, 20);
    println!(
        "  {} {:>5}% {}",
        theme::paint_pad(Tone::Green, "Swap", 16),
        format!("{:.0}", mem_info.swap_percent),
        swap_bar
    );

    if cpu_info.frequency_mhz > 0 {
        println!(
            "  {} {}",
            theme::paint_pad(Tone::Green, "CPU Freq", 16),
            theme::paint(
                Tone::Purple,
                &format!("{:.2} GHz", cpu_info.frequency_mhz as f64 / 1000.0)
            )
        );
    }

    println!();
    theme::header("Per-Core Usage");
    for (i, &usage) in cpu_info.per_core.iter().enumerate() {
        let core_bar = theme::bar(usage as f64, 16);
        println!(
            "  Core {:<4} {:>5}% {}",
            format!("{}", i),
            format!("{:.0}", usage),
            core_bar
        );
        if i >= 7 {
            println!("  … {} more cores...", cpu_info.per_core.len() - 8);
            break;
        }
    }

    println!();
    theme::header("Top Processes");
    let mut processes: Vec<_> = sys.processes().values().collect();
    // `total_cmp` (not `partial_cmp().unwrap()`) so a NaN `cpu_usage()`
    // reading — which sysinfo can report transiently right after a
    // process starts — sorts deterministically instead of panicking.
    processes.sort_by(|a, b| b.cpu_usage().total_cmp(&a.cpu_usage()));

    println!(
        "  {} {} {}",
        theme::paint_pad(Tone::Green, "Process", 30),
        theme::paint_pad(Tone::Green, "CPU%", 10),
        theme::paint(Tone::Green, "Memory")
    );
    println!("  {}", theme::paint(Tone::Green, &"─".repeat(52)));

    for proc in processes.iter().take(8) {
        let mem_mb = proc.memory() / 1_048_576;
        println!(
            "  {} {} {}",
            theme::paint_pad(Tone::Purple, proc.name(), 30),
            theme::paint_pad(Tone::Blue, &format!("{:.1}%", proc.cpu_usage()), 10),
            format!("{} MB", mem_mb)
        );
    }

    theme::success("Live snapshot — run 'sys perf' again to refresh");
}
