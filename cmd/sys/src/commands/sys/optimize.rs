use super::core::memory;
use crate::theme::{self, Ctx, Tone};

pub fn run(_ctx: &Ctx) {
    theme::title("optimize");
    theme::header("System Optimizer");

    let mem = memory::collect();

    println!(
        "  {} Scanning for optimization opportunities...",
        theme::paint(Tone::Green, "→")
    );

    let mut actions: Vec<(&str, bool, &str)> = Vec::new();

    if mem.swap_percent > 60.0 {
        actions.push((
            "High swap usage detected",
            true,
            "consider adding RAM or reducing load",
        ));
    }
    if mem.percent > 80.0 {
        actions.push(("Memory pressure high", true, "close unused applications"));
    } else {
        actions.push(("Memory usage healthy", false, "no action needed"));
    }
    actions.push((
        "Drop page cache",
        false,
        "frees ~200MB–2GB depending on workload",
    ));
    actions.push((
        "Review startup services",
        false,
        "disable unused services to reduce boot time",
    ));

    println!();
    theme::header("Optimization Report");

    for (action, warn, note) in &actions {
        let (icon, tone) = if *warn {
            ("⚠", Tone::Red)
        } else {
            ("✓", Tone::Green)
        };
        println!(
            "  {} {}",
            theme::paint(tone, icon),
            theme::paint(Tone::Purple, action)
        );
        println!("  {}", note);
        println!();
    }

    println!(
        "  {} Available Optimizations:",
        theme::paint(Tone::Green, "→")
    );
    println!();
    println!(
        "  {} {}",
        theme::paint(Tone::Blue, "1."),
        theme::paint(Tone::Green, "Drop disk cache:")
    );
    println!("  sudo sh -c 'sync; echo 3 > /proc/sys/vm/drop_caches'");
    println!();
    println!(
        "  {} {}",
        theme::paint(Tone::Blue, "2."),
        theme::paint(Tone::Green, "Set CPU governor to balanced:")
    );
    println!("  sudo cpupower frequency-set -g schedutil");
    println!();
    println!(
        "  {} {}",
        theme::paint(Tone::Blue, "3."),
        theme::paint(Tone::Green, "Kill zombie processes:")
    );
    println!("  sys process | grep zombie");

    theme::divider();
    theme::success("Optimization analysis complete — review suggestions above");
}
