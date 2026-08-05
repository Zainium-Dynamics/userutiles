use crate::error::{PrioError, Result};

// -- I/O Scheduling Mode ------------------------------------------------------

/// Linux I/O scheduling class exposed to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoMode {
    Realtime,
    High,
    Normal,
    Idle,
}

impl std::str::FromStr for IoMode {
    type Err = PrioError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "realtime" | "rt" => Ok(Self::Realtime),
            "high" => Ok(Self::High),
            "normal" => Ok(Self::Normal),
            "idle" => Ok(Self::Idle),
            _ => Err(PrioError::UnknownIoMode(s.to_string())),
        }
    }
}

impl std::fmt::Display for IoMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Realtime => "Realtime",
            Self::High => "High",
            Self::Normal => "Normal",
            Self::Idle => "Idle",
        })
    }
}

/// Linux ioprio bitmask encoding:
/// bits 15-13 → class (1 = RT, 2 = BE, 3 = Idle)
/// bits 2-0 → priority within class (0 = highest, 7 = lowest)
impl IoMode {
    pub fn ioprio_value(&self) -> u32 {
        let (class, data): (u32, u32) = match self {
            Self::Realtime => (1, 4), // RT class, mid priority
            Self::High => (2, 0),     // BE class, highest
            Self::Normal => (2, 4),   // BE class, mid
            Self::Idle => (3, 7),     // Idle class, lowest
        };
        (class << 13) | data
    }
}

// -- CPU Level → Niceness Conversion -----------------------------------------

/// Map a user-supplied CPU level (0–100) to a Linux niceness value (-20..+19).
///
/// The mapping is linear: cpu=0 → nice=+19, cpu=50 → nice=0, cpu=100 → nice=-20.
pub fn cpu_level_to_nice(level: u32) -> Result<i32> {
    if level > 100 {
        return Err(PrioError::InvalidCpuLevel(level));
    }
    // nice = 19 − floor(level * 39 / 100)
    let nice = 19i32 - ((level as i32 * 39) / 100);
    Ok(nice.clamp(-20, 19))
}

/// Validate that a niceness value is in the legal kernel range.
pub fn validate_nice(n: i32) -> Result<i32> {
    if (-20..=19).contains(&n) {
        Ok(n)
    } else {
        Err(PrioError::InvalidNiceness(n))
    }
}

// -- Human-readable labels ----------------------------------------------------

/// Return a short label describing a niceness level for display purposes.
pub fn nice_to_label(nice: i32) -> &'static str {
    match nice {
        i32::MIN..=-15 => "Critical",
        -14..=-5 => "High",
        -4..=4 => "Normal",
        5..=14 => "Low",
        _ => "Background",
    }
}

/// Format a niceness value with its conventional sign prefix.
pub fn format_nice(nice: i32) -> String {
    match nice.cmp(&0) {
        std::cmp::Ordering::Less => nice.to_string(),
        std::cmp::Ordering::Equal => "0".to_string(),
        std::cmp::Ordering::Greater => format!("+{}", nice),
    }
}

// -- Memory Parsing -----------------------------------------------------------

/// Parse a human-readable memory string (e.g. "4G", "2.5G", "512M") to bytes.
pub fn parse_memory(s: &str) -> crate::error::Result<u64> {
    let s = s.trim();
    let upper = s.to_ascii_uppercase();

    let (num_str, multiplier): (&str, u64) = if upper.ends_with("GB") {
        (&s[..s.len() - 2], 1 << 30)
    } else if upper.ends_with('G') {
        (&s[..s.len() - 1], 1 << 30)
    } else if upper.ends_with("MB") {
        (&s[..s.len() - 2], 1 << 20)
    } else if upper.ends_with('M') {
        (&s[..s.len() - 1], 1 << 20)
    } else if upper.ends_with("KB") {
        (&s[..s.len() - 2], 1 << 10)
    } else if upper.ends_with('K') {
        (&s[..s.len() - 1], 1 << 10)
    } else {
        (s, 1)
    };

    let num: f64 = num_str
        .parse()
        .map_err(|_| PrioError::MemoryParseError(s.to_string()))?;

    if num < 0.0 {
        return Err(PrioError::MemoryParseError(s.to_string()));
    }

    Ok((num * multiplier as f64) as u64)
}

/// Format a byte count back to a concise human-readable string.
pub fn format_memory(bytes: u64) -> String {
    const GB: u64 = 1 << 30;
    const MB: u64 = 1 << 20;

    if bytes >= GB {
        let gb = bytes as f64 / GB as f64;
        if (gb - gb.floor()).abs() < 0.05 {
            format!("{}G", gb as u64)
        } else {
            format!("{:.1}G", gb)
        }
    } else if bytes >= MB {
        format!("{}M", bytes / MB)
    } else {
        format!("{}K", bytes / 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_to_nice_boundary() {
        assert_eq!(cpu_level_to_nice(0).unwrap(), 19);
        assert_eq!(cpu_level_to_nice(50).unwrap(), 0);
        assert_eq!(cpu_level_to_nice(100).unwrap(), -20);
    }

    #[test]
    fn cpu_level_invalid() {
        assert!(cpu_level_to_nice(101).is_err());
    }

    #[test]
    fn memory_parse_round_trip() {
        assert_eq!(parse_memory("4G").unwrap(), 4 * (1 << 30));
        assert_eq!(
            parse_memory("2.5G").unwrap(),
            (2.5 * (1u64 << 30) as f64) as u64
        );
        assert_eq!(parse_memory("512M").unwrap(), 512 * (1 << 20));
    }

    #[test]
    fn nice_label_correct() {
        assert_eq!(nice_to_label(-20), "Critical");
        assert_eq!(nice_to_label(0), "Normal");
        assert_eq!(nice_to_label(19), "Background");
    }
}
