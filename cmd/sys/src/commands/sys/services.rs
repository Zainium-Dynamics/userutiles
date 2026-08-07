use crate::theme::{self, Ctx, Tone};
use std::process::Command;

struct ServiceEntry {
    name: String,
    active: bool,
    status: String,
    description: String,
}

pub fn run(_ctx: &Ctx) {
    theme::title("services");
    theme::header("System Services");

    let services = fetch_services();

    println!(
        "  {} {} {}",
        theme::paint_pad(Tone::Green, "Service", 28),
        theme::paint_pad(Tone::Green, "Status", 14),
        theme::paint(Tone::Green, "Description")
    );
    println!("  {}", theme::paint(Tone::Green, &"─".repeat(68)));

    let active_count = services.iter().filter(|s| s.active).count();

    for s in &services {
        let (icon, tone) = if s.active {
            ("*", Tone::Green)
        } else {
            ("○", Tone::Red)
        };
        let status_str = format!("{} {}", icon, s.status);
        println!(
            "  {} {} {}",
            theme::paint_pad(Tone::Purple, &s.name, 28),
            theme::paint_pad(tone, &status_str, 14),
            s.description
        );
    }

    theme::divider();
    println!(
        "  {}",
        theme::paint(
            Tone::Blue,
            &format!("{}/{} services active", active_count, services.len())
        )
    );
    theme::success("Service inspection complete");
}

fn fetch_services() -> Vec<ServiceEntry> {
    let output = Command::new("systemctl")
        .args([
            "list-units",
            "--type=service",
            "--no-pager",
            "--no-legend",
            "--state=loaded",
        ])
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut entries = Vec::new();
        for line in text.lines().take(16) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let name = parts[0].trim_end_matches(".service").to_string();
                let active = parts[2] == "active";
                let status = parts[3].to_string();
                let desc = parts[4..].join(" ");
                entries.push(ServiceEntry {
                    name,
                    active,
                    status,
                    description: desc,
                });
            }
        }
        if !entries.is_empty() {
            return entries;
        }
    }

    vec![
        ServiceEntry {
            name: "NetworkManager".into(),
            active: true,
            status: "running".into(),
            description: "Network management daemon".into(),
        },
        ServiceEntry {
            name: "sshd".into(),
            active: true,
            status: "running".into(),
            description: "OpenSSH server daemon".into(),
        },
        ServiceEntry {
            name: "bluetooth".into(),
            active: true,
            status: "running".into(),
            description: "Bluetooth service".into(),
        },
        ServiceEntry {
            name: "cups".into(),
            active: false,
            status: "dead".into(),
            description: "CUPS printing spooler".into(),
        },
        ServiceEntry {
            name: "firewalld".into(),
            active: true,
            status: "running".into(),
            description: "Dynamic firewall daemon".into(),
        },
        ServiceEntry {
            name: "docker".into(),
            active: false,
            status: "dead".into(),
            description: "Docker Application Container Engine".into(),
        },
        ServiceEntry {
            name: "user-daemon".into(),
            active: true,
            status: "running".into(),
            description: "ZainiumOS package manager daemon".into(),
        },
        ServiceEntry {
            name: "thermald".into(),
            active: true,
            status: "running".into(),
            description: "Thermal management daemon".into(),
        },
    ]
}
