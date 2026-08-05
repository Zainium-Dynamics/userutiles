use crate::tracer::TraceData;
use colored::Colorize;

pub fn format_table(data: &TraceData) -> crate::utils::TraceResult<String> {
    let mut output = String::new();

    // Header with process name and PID
    output.push_str(&format!(
        "\n{}\n\n",
        format!("Trace started on process: {} (PID: {})", 
                data.process.name.cyan().bold(),
                data.process.pid.to_string().cyan().bold())
    ));

    // Process Information Section
    output.push_str(&format!("{}\n", "Process Information".bold()));
    output.push_str(&format_row("PID", &data.process.pid.to_string(), 15));
    output.push_str(&format_row("Name", &data.process.name, 15));
    output.push_str(&format_row("Status", &data.process.status, 15));
    output.push_str(&format_row("Uptime", &data.process.uptime, 15));
    output.push_str(&format_row("Memory", &format!("{} MB (RSS)", data.memory.rss_mb), 15));
    output.push_str(&format_row("CPU Usage", &format!("{:.1}%", data.process.cpu_percent), 15));

    // Syscall Summary Section
    output.push_str(&format!("\n{}\n", "Syscall Summary".bold()));
    let stats = data.syscalls.get_stats();
    let total = data.syscalls.total_syscalls();
    let unique = data.syscalls.unique_syscalls();
    
    let top_3 = if stats.is_empty() {
        "—".to_string()
    } else {
        stats
            .iter()
            .take(3)
            .map(|s| format!("{} ({})", s.name, format_count(s.count)))
            .collect::<Vec<_>>()
            .join(", ")
    };

    output.push_str(&format_row("Total syscalls", &format_count(total), 18));
    output.push_str(&format_row("Unique syscalls", &format_count(unique as u64), 18));
    output.push_str(&format_row("Top 3 syscalls", &top_3, 18));

    // Network Activity Section
    output.push_str(&format!("\n{}\n", "Network Activity".bold()));
    output.push_str(&format_row("Connections", &format!("{} active", data.network.active_connections), 15));
    output.push_str(&format_row("Bytes sent", &format_bytes(data.network.bytes_sent), 15));
    output.push_str(&format_row("Bytes received", &format_bytes(data.network.bytes_received), 15));

    // Footer
    output.push_str(&format!(
        "\n{}\n",
        "✓ Trace completed successfully.".green().bold()
    ));

    Ok(output)
}

/// Format a row with key : value alignment
fn format_row(key: &str, value: &str, key_width: usize) -> String {
    format!("  {:<key_width$} : {}\n", key, value, key_width = key_width)
}

fn format_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if size == 0.0 {
        "0 B".to_string()
    } else if size < 10.0 {
        format!("{:.1} {}", size, UNITS[unit_idx])
    } else {
        format!("{:.0} {}", size, UNITS[unit_idx])
    }
}
