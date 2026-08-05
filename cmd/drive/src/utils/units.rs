/// Convert bytes to a human-readable string with appropriate SI unit
pub fn bytes_to_human(bytes: u64) -> String {
    const KB: u64 = 1_000;
    const MB: u64 = 1_000 * KB;
    const GB: u64 = 1_000 * MB;
    const TB: u64 = 1_000 * GB;

    if bytes == 0 {
        return "0 B".to_string();
    }

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Convert bytes to GiB (binary gigabytes)
#[allow(dead_code)]
pub fn bytes_to_gib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_bytes() {
        assert_eq!(bytes_to_human(0), "0 B");
    }

    #[test]
    fn bytes_range() {
        assert_eq!(bytes_to_human(512), "512 B");
    }

    #[test]
    fn kilobytes() {
        assert_eq!(bytes_to_human(2_000), "2 KB");
    }

    #[test]
    fn megabytes() {
        assert_eq!(bytes_to_human(15_000_000), "15 MB");
    }

    #[test]
    fn gigabytes() {
        let s = bytes_to_human(1_000_000_000);
        assert!(s.contains("GB"), "expected GB, got {s}");
    }

    #[test]
    fn terabytes() {
        let s = bytes_to_human(1_000_000_000_000);
        assert!(s.contains("TB"), "expected TB, got {s}");
    }

    #[test]
    fn bytes_to_gib_one_gib() {
        let v = bytes_to_gib(1024 * 1024 * 1024);
        assert!((v - 1.0).abs() < 1e-9, "expected 1.0 GiB, got {v}");
    }
}
