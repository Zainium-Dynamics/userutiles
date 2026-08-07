use crate::theme::{self, Ctx, Tone};
use sysinfo::{ProcessExt, System, SystemExt};

pub fn run(_ctx: &Ctx) {
    theme::title("process");
    let mut sys = System::new_all();
    sys.refresh_all();

    theme::header("Process Manager");
    let mut processes: Vec<_> = sys.processes().values().collect();
    // `total_cmp` (not `partial_cmp().unwrap()`) so a NaN `cpu_usage()`
    // reading — which sysinfo can report transiently right after a
    // process starts — sorts deterministically instead of panicking.
    processes.sort_by(|a, b| b.cpu_usage().total_cmp(&a.cpu_usage()));

    println!(
        "  {} {} {} {} {}",
        theme::paint_pad(Tone::Green, "PID", 8),
        theme::paint_pad(Tone::Green, "Name", 28),
        theme::paint_pad(Tone::Green, "CPU%", 10),
        theme::paint_pad(Tone::Green, "Memory", 12),
        theme::paint(Tone::Green, "Status")
    );
    println!("  {}", theme::paint(Tone::Green, &"─".repeat(68)));

    for proc in processes.iter().take(20) {
        let mem_mb = proc.memory() / 1_048_576;
        let cpu = proc.cpu_usage();
        let cpu_tone = if cpu > 50.0 {
            Tone::Red
        } else if cpu > 20.0 {
            Tone::Blue
        } else {
            Tone::Green
        };
        let status = format!("{:?}", proc.status());
        println!(
            "  {} {} {} {} {}",
            format!("{:<8}", proc.pid()),
            theme::paint_pad(Tone::Purple, proc.name(), 28),
            theme::paint_pad(cpu_tone, &format!("{:.1}%", cpu), 10),
            theme::paint_pad(Tone::Blue, &format!("{} MB", mem_mb), 12),
            status
        );
    }

    println!();
    println!("  Total: {} processes running", sys.processes().len());
    theme::success("Process snapshot complete");
}
