use super::core::system;
use crate::theme::{self, Ctx, Tone};
use sysinfo::{DiskExt, NetworkExt, NetworksExt, System, SystemExt};

pub fn run(_ctx: &Ctx) {
    theme::title("info");
    let sys_info = system::collect();

    theme::header("Deep System Information");

    println!("  {}",
        theme::paint(
            Tone::Green,
            "-- Operating System -----------------------------"
        )
    );
    theme::kv_row("Hostname", &sys_info.hostname);
    theme::kv_row("OS", &sys_info.os_name);
    theme::kv_row("Kernel", &sys_info.kernel_version);
    theme::kv_row("Uptime", &system::format_uptime(sys_info.uptime_secs));
    let arch = std::env::consts::ARCH;
    if !arch.is_empty() {
        theme::kv_row("Architecture", arch);
    }

    println!();
    println!("  {}",
        theme::paint(
            Tone::Green,
            "-- Processor ------------------------------------"
        )
    );
    theme::kv_row("Model", &sys_info.cpu_name);
    theme::kv_row(
        "Cores",
        &format!(
            "{} physical / {} logical",
            sys_info.cpu_cores, sys_info.cpu_threads
        ),
    );
    theme::kv_row("Usage", &format!("{:.1}%", sys_info.cpu_usage));

    println!();
    println!("  {}",
        theme::paint(
            Tone::Green,
            "-- Memory ---------------------------------------"
        )
    );
    theme::kv_row(
        "Total RAM",
        &format!("{:.2} GiB", sys_info.total_memory_gib),
    );
    theme::kv_row(
        "Used RAM",
        &format!(
            "{:.2} GiB ({:.0}%)",
            sys_info.used_memory_gib, sys_info.memory_percent
        ),
    );
    theme::kv_row("Total Swap", &format!("{:.2} GiB", sys_info.total_swap_gib));
    theme::kv_row(
        "Used Swap",
        &format!(
            "{:.2} GiB ({:.0}%)",
            sys_info.used_swap_gib, sys_info.swap_percent
        ),
    );

    println!();
    println!("  {}",
        theme::paint(
            Tone::Green,
            "-- Storage --------------------------------------"
        )
    );
    let mut sys = System::new();
    sys.refresh_disks_list();
    sys.refresh_disks();
    for disk in sys.disks() {
        let gb = 1_073_741_824.0_f64;
        let total = disk.total_space() as f64 / gb;
        let avail = disk.available_space() as f64 / gb;
        let used = total - avail;
        let fs = String::from_utf8_lossy(disk.file_system());
        println!("  {} {} — {:.1} / {:.1} GiB used {}",
            theme::paint_pad(Tone::Purple, &disk.name().to_string_lossy(), 18),
            theme::paint(Tone::Green, &disk.mount_point().to_string_lossy()),
            used,
            total,
            fs.as_ref()
        );
    }

    println!();
    println!("  {}",
        theme::paint(
            Tone::Green,
            "-- Network Interfaces ---------------------------"
        )
    );
    sys.refresh_networks_list();
    sys.refresh_networks();
    for (name, data) in sys.networks().iter().take(6) {
        let rx_mb = data.total_received() / 1_048_576;
        let tx_mb = data.total_transmitted() / 1_048_576;
        println!("  {} RX {} MB TX {} MB",
            theme::paint_pad(Tone::Purple, name, 18),
            theme::paint(Tone::Green, &rx_mb.to_string()),
            theme::paint(Tone::Blue, &tx_mb.to_string())
        );
    }

    println!();
    println!("  {}",
        theme::paint(
            Tone::Green,
            "-- ZainiumOS ------------------------------------"
        )
    );
    theme::kv_row("Package Mgr", "user_utils 0.1.0");
    theme::kv_row("Init System", "systemd");
    theme::kv_row(
        "Shell",
        std::env::var("SHELL")
            .unwrap_or_else(|_| "/bin/zsh".into())
            .as_str(),
    );
    theme::kv_row(
        "Terminal",
        std::env::var("TERM")
            .unwrap_or_else(|_| "xterm-256color".into())
            .as_str(),
    );

    theme::success("Full system inspection complete");
}
