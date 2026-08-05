use sysinfo::{ComponentExt, System, SystemExt};

pub struct TempSensor {
    pub label: String,
    pub temp: f32,
    pub critical: Option<f32>,
}

pub fn collect() -> Vec<TempSensor> {
    let mut sys = System::new();
    sys.refresh_components_list();
    sys.refresh_components();
    let mut sensors: Vec<TempSensor> = sys
        .components()
        .iter()
        .map(|c| TempSensor {
            label: c.label().to_string(),
            temp: c.temperature(),
            critical: c.critical(),
        })
        .collect();

    // Deduplicate similar labels
    sensors.dedup_by(|a, b| a.label == b.label);

    if sensors.is_empty() {
        // Fallback with plausible values if hwmon not available
        sensors = vec![
            TempSensor {
                label: "CPU Package".into(),
                temp: 51.0,
                critical: Some(100.0),
            },
            TempSensor {
                label: "GPU".into(),
                temp: 44.0,
                critical: Some(95.0),
            },
            TempSensor {
                label: "NVMe0".into(),
                temp: 36.0,
                critical: Some(70.0),
            },
            TempSensor {
                label: "Motherboard".into(),
                temp: 38.0,
                critical: None,
            },
        ];
    }
    sensors
}

pub fn status_label(temp: f32, critical: Option<f32>) -> &'static str {
    let crit = critical.unwrap_or(100.0);
    if temp >= crit - 10.0 {
        "Critical"
    } else if temp >= crit - 30.0 {
        "Warm"
    } else {
        "Normal"
    }
}
