//! user blkid — locate/print block device attributes.
//!
//! Probes filesystem/swap superblocks via `usercore::blkprobe` (this
//! workspace's from-scratch libblkid stand-in — see that module's docs).
use std::fs;
use std::path::{Path, PathBuf};

use usercore::blkprobe::{probe_path, Probe};
use usercore::Ui;

/// Every device name in `/proc/partitions` (skips the two-line header),
/// as full `/dev/<name>` paths.
fn list_devices() -> Vec<PathBuf> {
    let text = fs::read_to_string("/proc/partitions").unwrap_or_default();
    text.lines()
        .skip(2)
        .filter_map(|l| {
            let name = l.split_whitespace().nth(3)?;
            Some(PathBuf::from(format!("/dev/{name}")))
        })
        .collect()
}

fn print_tag_format(device: &Path, probe: &Probe) {
    let mut out = format!("{}:", device.display());
    if let Some(label) = &probe.label {
        out.push_str(&format!(" LABEL=\"{label}\""));
    }
    if let Some(uuid) = &probe.uuid {
        out.push_str(&format!(" UUID=\"{uuid}\""));
    }
    out.push_str(&format!(" TYPE=\"{}\"", probe.fstype));
    println!("{out}");
}

fn field_value(probe: &Probe, field: &str) -> Option<String> {
    match field {
        "TYPE" => Some(probe.fstype.clone()),
        "UUID" => probe.uuid.clone(),
        "LABEL" => probe.label.clone(),
        _ => None,
    }
}

/// Find the first device whose probed `field` equals `value` — the
/// shared logic behind both `blkid -L`/`-U` and the standalone `findfs`
/// utility.
pub fn find_by_field(field: &str, value: &str) -> Option<PathBuf> {
    for device in list_devices() {
        if let Ok(Some(probe)) = probe_path(&device) {
            if field_value(&probe, field).as_deref() == Some(value) {
                return Some(device);
            }
        }
    }
    None
}

fn print_help() {
    print!(
        "Usage: blkid [DEVICE...]\n\
 blkid -L LABEL\n\
 blkid -U UUID\n\
 blkid -o value -s TAG DEVICE\n\
 Locate/print block device attributes (TYPE, UUID, LABEL).\n\
 -L, --label LABEL find the device with this label\n\
 -U, --uuid UUID find the device with this UUID\n\
 -s, --match-tag TAG only look up this tag (with -o value)\n\
 -o, --output value print only the bare tag value, no quotes\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `blkid` utility. Parses `std::env::args()` and
/// either looks up a device by `-L`/`-U`, prints one bare tag value for a
/// given device (`-s TAG -o value DEVICE`), or lists `LABEL`/`UUID`/`TYPE`
/// for the given devices (or every device in `/proc/partitions`, with
/// none given) in `blkid`'s classic `dev: TAG="val"...` format.
///
/// Returns 0 if at least one device matched/printed, 2 if nothing did
/// (matching real `blkid`'s not-found exit code), 4 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("blkid");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut find_label: Option<String> = None;
    let mut find_uuid: Option<String> = None;
    let mut match_tag: Option<String> = None;
    let mut value_only = false;
    let mut devices: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("blkid (user_utils) 0.1.0");
                return 0;
            }
            "-L" | "--label" => {
                i += 1;
                match args.get(i) {
                    Some(v) => find_label = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 'L'");
                        return 4;
                    }
                }
            }
            "-U" | "--uuid" => {
                i += 1;
                match args.get(i) {
                    Some(v) => find_uuid = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 'U'");
                        return 4;
                    }
                }
            }
            "-s" | "--match-tag" => {
                i += 1;
                match args.get(i) {
                    Some(v) => match_tag = Some(v.clone()),
                    None => {
                        ui.err("option requires an argument -- 's'");
                        return 4;
                    }
                }
            }
            "-o" | "--output" => {
                i += 1;
                match args.get(i).map(String::as_str) {
                    Some("value") => value_only = true,
                    Some(other) => {
                        ui.err(&format!("unsupported -o mode '{other}'"));
                        return 4;
                    }
                    None => {
                        ui.err("option requires an argument -- 'o'");
                        return 4;
                    }
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 4;
            }
            other => devices.push(other.to_string()),
        }
        i += 1;
    }

    if let Some(label) = find_label {
        return match find_by_field("LABEL", &label) {
            Some(dev) => {
                println!("{}", dev.display());
                0
            }
            None => 2,
        };
    }
    if let Some(uuid) = find_uuid {
        return match find_by_field("UUID", &uuid) {
            Some(dev) => {
                println!("{}", dev.display());
                0
            }
            None => 2,
        };
    }

    if value_only {
        let Some(tag) = match_tag else {
            ui.err("-o value requires -s TAG");
            return 4;
        };
        let Some(dev) = devices.first() else {
            ui.err("-o value requires a DEVICE argument");
            return 4;
        };
        return match probe_path(Path::new(dev)) {
            Ok(Some(probe)) => match field_value(&probe, &tag) {
                Some(v) => {
                    println!("{v}");
                    0
                }
                None => 2,
            },
            Ok(None) => 2,
            Err(e) => {
                ui.err(&format!("{dev}: {e}"));
                2
            }
        };
    }

    let targets: Vec<PathBuf> = if devices.is_empty() {
        list_devices()
    } else {
        devices.iter().map(PathBuf::from).collect()
    };

    let mut found_any = false;
    for dev in &targets {
        match probe_path(dev) {
            Ok(Some(probe)) => {
                print_tag_format(dev, &probe);
                found_any = true;
            }
            Ok(None) => {}
            Err(e) => {
                if !devices.is_empty() {
                    ui.err(&format!("{}: {e}", dev.display()));
                }
            }
        }
    }
    if found_any {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_value_extracts_known_tags() {
        let p = Probe {
            fstype: "ext4".to_string(),
            uuid: Some("abc".to_string()),
            label: Some("root".to_string()),
        };
        assert_eq!(field_value(&p, "TYPE"), Some("ext4".to_string()));
        assert_eq!(field_value(&p, "UUID"), Some("abc".to_string()));
        assert_eq!(field_value(&p, "LABEL"), Some("root".to_string()));
        assert_eq!(field_value(&p, "NOPE"), None);
    }

    #[test]
    fn list_devices_does_not_panic() {
        // /proc/partitions may not exist in a restricted test sandbox;
        // this must degrade to an empty list, not error out.
        assert!(list_devices().len() < 10_000);
    }

    #[test]
    fn find_by_field_none_for_impossible_value() {
        assert!(find_by_field("UUID", "00000000-user-blkid-test-nonexistent").is_none());
    }
}
