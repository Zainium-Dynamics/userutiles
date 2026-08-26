//! user sfdisk — scriptable partition table tool.
//!
//! Script format is a simplified version of real `sfdisk`'s own dump
//! format (`sfdisk -d` output is valid input here, modulo the metadata
//! lines this parser ignores): an optional `label: dos|gpt` line,
//! optional ignored metadata (`label-id:`, `device:`, `unit:`,
//! `first-lba:`, `last-lba:`, `sector-size:`), then one partition per
//! line as `[ignored-prefix :] start=N, size=N, type=T[, bootable][,
//! name="..."]`.
use std::io::{self, Read};
use std::path::Path;

use usercore::ptable::{self, Label, Partition, PartitionTable};
use usercore::Ui;

fn parse_script(text: &str) -> (Label, Vec<Partition>) {
    let mut label = Label::Dos;
    let mut partitions = Vec::new();
    let mut number = 1;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("label:") {
            if rest.trim() == "gpt" {
                label = Label::Gpt;
            }
            continue;
        }
        let is_ignored_metadata = [
            "label-id:",
            "device:",
            "unit:",
            "first-lba:",
            "last-lba:",
            "sector-size:",
        ]
        .iter()
        .any(|meta| line.starts_with(meta));
        if is_ignored_metadata {
            continue;
        }

        // Drop any "<name> : " prefix real sfdisk -d dumps prepend —
        // find the actual field list by locating "start=".
        let fields_str = match line.find("start=") {
            Some(idx) => &line[idx..],
            None => continue,
        };

        let mut start_lba = 0u64;
        let mut size_lba = 0u64;
        let mut part_type = String::new();
        let mut bootable = false;
        let mut name: Option<String> = None;

        for field in fields_str.split(',') {
            let field = field.trim();
            if field == "bootable" {
                bootable = true;
                continue;
            }
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "start" => start_lba = value.parse().unwrap_or(0),
                "size" => size_lba = value.parse().unwrap_or(0),
                "type" => {
                    part_type = match label {
                        Label::Dos => ptable::resolve_mbr_type(value).unwrap_or_default(),
                        Label::Gpt => ptable::resolve_gpt_type(value).unwrap_or_default(),
                    }
                }
                "name" => name = Some(value.to_string()),
                _ => {}
            }
        }

        if part_type.is_empty() {
            part_type = match label {
                Label::Dos => "83".to_string(),
                Label::Gpt => ptable::resolve_gpt_type("L").unwrap(),
            };
        }

        partitions.push(Partition {
            number,
            start_lba,
            size_lba,
            part_type,
            bootable,
            name,
        });
        number += 1;
    }

    (label, partitions)
}

fn dump_script(device: &Path, table: &PartitionTable) -> String {
    let label_str = match table.label {
        Label::Dos => "dos",
        Label::Gpt => "gpt",
    };
    let mut out = format!(
        "label: {label_str}\ndevice: {}\nunit: sectors\n\n",
        device.display()
    );
    for p in &table.partitions {
        out.push_str(&format!(
            "{}{} : start={:>12}, size={:>12}, type={}",
            device.display(),
            p.number,
            p.start_lba,
            p.size_lba,
            p.part_type
        ));
        if p.bootable {
            out.push_str(", bootable");
        }
        if let Some(name) = &p.name {
            out.push_str(&format!(", name=\"{name}\""));
        }
        out.push('\n');
    }
    out
}

fn print_help() {
    print!(
        "Usage: sfdisk DEVICE < script\n\
 sfdisk -d DEVICE\n\
 sfdisk -l DEVICE\n\
 Write (from a script on stdin) or dump the partition table of DEVICE.\n\
 -d, --dump dump the current table as a script, to stdout\n\
 -l, --list list the current table, human-readable\n\
 --help display this help and exit\n\
 --version output version information and exit\n"
    );
}

/// Entry point for the `sfdisk` utility. Parses `std::env::args()` and
/// either dumps `DEVICE`'s current table as a re-readable script (`-d`),
/// lists it human-readably (`-l`), or (with neither) reads a script from
/// stdin and writes a new MBR/GPT table to `DEVICE`.
///
/// Returns 0 on success, 1 on a usage, read, or write error.
pub fn run() -> i32 {
    let ui = Ui::new("sfdisk");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut dump = false;
    let mut list = false;
    let mut device: Option<String> = None;
    for arg in &args {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            "--version" => {
                println!("sfdisk (user_utils) 0.1.0");
                return 0;
            }
            "-d" | "--dump" => dump = true,
            "-l" | "--list" => list = true,
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("unrecognized option '{s}'"));
                return 1;
            }
            other => device = Some(other.to_string()),
        }
    }

    let Some(device) = device else {
        ui.err("usage: sfdisk [-d|-l] DEVICE");
        return 1;
    };
    let path = Path::new(&device);

    if dump || list {
        return match ptable::read_table(path) {
            Ok(Some(table)) => {
                if dump {
                    print!("{}", dump_script(path, &table));
                } else {
                    for p in &table.partitions {
                        println!(
                            "{}{}: start={} size={} type={}",
                            path.display(),
                            p.number,
                            p.start_lba,
                            p.size_lba,
                            p.part_type
                        );
                    }
                }
                0
            }
            Ok(None) => {
                ui.err(&format!("{device}: no partition table found"));
                1
            }
            Err(e) => {
                ui.err(&format!("{device}: {e}"));
                1
            }
        };
    }

    let mut script = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut script) {
        ui.err(&format!("failed to read script from stdin: {e}"));
        return 1;
    }
    let (label, partitions) = parse_script(&script);
    if partitions.is_empty() {
        ui.err("no partitions found in script");
        return 1;
    }

    let result = match label {
        Label::Dos => ptable::write_mbr(path, &partitions),
        Label::Gpt => ptable::write_gpt(path, &partitions),
    };
    match result {
        Ok(()) => {
            println!(
                "New {} table written to {device}, {} partitions.",
                match label {
                    Label::Dos => "dos",
                    Label::Gpt => "gpt",
                },
                partitions.len()
            );
            0
        }
        Err(e) => {
            ui.err(&format!("{device}: {e}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_script_reads_dos_partitions() {
        let script = "\
label: dos

/dev/sda1 : start=2048, size=200000, type=83, bootable
/dev/sda2 : start=202048, size=100000, type=82
";
        let (label, partitions) = parse_script(script);
        assert_eq!(label, Label::Dos);
        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions[0].start_lba, 2048);
        assert_eq!(partitions[0].part_type, "83");
        assert!(partitions[0].bootable);
        assert_eq!(partitions[1].part_type, "82");
        assert!(!partitions[1].bootable);
    }

    #[test]
    fn parse_script_reads_gpt_with_name_and_aliases() {
        let script = "\
label: gpt

/dev/sda1 : start=2048, size=200000, type=L, name=\"root\"
/dev/sda2 : start=202048, size=100000, type=S, name=\"swap\"
";
        let (label, partitions) = parse_script(script);
        assert_eq!(label, Label::Gpt);
        assert_eq!(
            partitions[0].part_type,
            "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
        );
        assert_eq!(partitions[0].name.as_deref(), Some("root"));
        assert_eq!(
            partitions[1].part_type,
            "0657FD6D-A4AB-43C4-84E5-0933C84B4F4F"
        );
    }

    #[test]
    fn dump_and_reparse_round_trips() {
        let table = PartitionTable {
            label: Label::Dos,
            partitions: vec![Partition {
                number: 1,
                start_lba: 2048,
                size_lba: 1000,
                part_type: "83".to_string(),
                bootable: true,
                name: None,
            }],
        };
        let dumped = dump_script(Path::new("/dev/sda"), &table);
        let (label, partitions) = parse_script(&dumped);
        assert_eq!(label, Label::Dos);
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions[0].start_lba, 2048);
        assert_eq!(partitions[0].size_lba, 1000);
        assert!(partitions[0].bootable);
    }

    #[test]
    fn parse_script_ignores_metadata_lines() {
        let script = "\
label: gpt
label-id: ABC-123
device: /dev/sda
unit: sectors
first-lba: 34
last-lba: 1000
sector-size: 512

/dev/sda1 : start=2048, size=1000, type=L
";
        let (_, partitions) = parse_script(script);
        assert_eq!(partitions.len(), 1);
    }
}
