//! user lscpu — display information about the CPU architecture.
//!
//! Ported from uutils/util-linux's `lscpu`. Reads `/proc/cpuinfo` and the
//! `/sys/devices/system/cpu` sysfs tree and prints a tree of
//! field/value pairs, either as aligned text (default) or JSON (`--json`).
use std::cmp;
use std::fs;
use std::path::Path;

use usercore::Ui;

mod sysfs;

const PATH_SYS_CPU: &str = "/sys/devices/system/cpu";
const PATH_SYS_KERNEL: &str = "/sys/kernel";
const PATH_PROC_CPUINFO: &str = "/proc/cpuinfo";

const HELP: &str = "Usage: lscpu [options]\n\
Display information about the CPU architecture.\n\n\
  -B, --bytes   print sizes in bytes rather than in human-readable format\n\
  -J, --json    use JSON output format\n\
  -x, --hex     use hexadecimal masks for CPU sets (currently a no-op;\n\
                list format is always used)\n\
  -h, --help    display this help and exit\n\
      --version output version information and exit\n";

/// One row of the field tree that `lscpu` prints: a label, its value, and
/// any nested rows (e.g. `Vendor ID` -> `Model name` -> `CPU Family`).
struct CpuInfo {
    field: String,
    data: String,
    children: Vec<CpuInfo>,
}

impl CpuInfo {
    fn new(field: &str, data: &str) -> Self {
        Self {
            field: field.to_string(),
            data: data.to_string(),
            children: Vec::new(),
        }
    }

    fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }
}

struct OutputOptions {
    bytes: bool,
    json: bool,
}

/// Entry point for the `lscpu` utility.
pub fn run() -> i32 {
    let ui = Ui::new("lscpu");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("lscpu (user_utils) 0.1.0");
        return 0;
    }

    let mut out_opts = OutputOptions {
        bytes: false,
        json: false,
    };
    for a in &args {
        match a.as_str() {
            "-B" | "--bytes" => out_opts.bytes = true,
            "-J" | "--json" => out_opts.json = true,
            "-x" | "--hex" => {} // accepted, not implemented (see HELP)
            other => {
                ui.err(&format!("unknown option -- '{other}'"));
                return 1;
            }
        }
    }

    let infos = collect(
        Path::new(PATH_SYS_CPU),
        Path::new(PATH_SYS_KERNEL),
        PATH_PROC_CPUINFO,
        out_opts.bytes,
    );
    print_output(&infos, &out_opts);
    0
}

/// Gather all `lscpu` sections into the field tree. Split out from `run`
/// so tests can point `cpu_base`/`kernel_base`/`cpuinfo_path` at fixtures.
fn collect(cpu_base: &Path, kernel_base: &Path, cpuinfo_path: &str, bytes: bool) -> Vec<CpuInfo> {
    let mut infos: Vec<CpuInfo> = Vec::new();

    let mut arch_info = CpuInfo::new("Architecture", &get_architecture());
    let contents = fs::read_to_string(cpuinfo_path).unwrap_or_default();

    if let Some(addr_sizes) = find_cpuinfo_value(&contents, "address sizes") {
        arch_info.add_child(CpuInfo::new("Address sizes", &addr_sizes));
    }
    if let Some(byte_order) = sysfs::read_cpu_byte_order(kernel_base) {
        arch_info.add_child(CpuInfo::new("Byte Order", byte_order));
    }
    infos.push(arch_info);

    let cpu_topology = sysfs::CpuTopology::read(cpu_base);
    let mut cores_info = CpuInfo::new("CPU(s)", &format!("{}", cpu_topology.cpus.len()));
    cores_info.add_child(CpuInfo::new(
        "On-line CPU(s) list",
        &sysfs::read_online_cpus(cpu_base),
    ));
    infos.push(cores_info);

    if let Some(vendor) = find_cpuinfo_value(&contents, "vendor_id") {
        let mut vendor_info = CpuInfo::new("Vendor ID", &vendor);

        if let Some(model_name) = find_cpuinfo_value(&contents, "model name") {
            let mut model_name_info = CpuInfo::new("Model name", &model_name);

            if let Some(family) = find_cpuinfo_value(&contents, "cpu family") {
                model_name_info.add_child(CpuInfo::new("CPU Family", &family));
            }
            if let Some(model) = find_cpuinfo_value(&contents, "model") {
                model_name_info.add_child(CpuInfo::new("Model", &model));
            }

            let socket_count = cpu_topology.socket_count();
            let core_count = cpu_topology.core_count();
            let n_cpus = cpu_topology.cpus.len().max(1);

            model_name_info.add_child(CpuInfo::new(
                "Thread(s) per core",
                &(n_cpus / core_count.max(1)).to_string(),
            ));
            model_name_info.add_child(CpuInfo::new(
                "Core(s) per socket",
                &(core_count / socket_count.max(1)).to_string(),
            ));
            model_name_info.add_child(CpuInfo::new("Socket(s)", &socket_count.to_string()));

            if let Some(freq_boost_enabled) = sysfs::read_freq_boost_state(cpu_base) {
                let s = if freq_boost_enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                model_name_info.add_child(CpuInfo::new("Frequency boost", s));
            }

            vendor_info.add_child(model_name_info);
        }

        infos.push(vendor_info);
    }

    if let Some(cache_info) = calculate_cache_totals(&cpu_topology, bytes) {
        infos.push(cache_info);
    }

    let vulns = sysfs::read_cpu_vulnerabilities(cpu_base);
    if !vulns.is_empty() {
        let mut vuln_info = CpuInfo::new("Vulnerabilities", "");
        for vuln in vulns {
            vuln_info.add_child(CpuInfo::new(&vuln.name, &vuln.mitigation));
        }
        infos.push(vuln_info);
    }

    infos
}

fn calculate_cache_totals(topo: &sysfs::CpuTopology, bytes: bool) -> Option<CpuInfo> {
    use std::collections::HashMap;

    let all_caches: Vec<_> = topo.cpus.iter().flat_map(|cpu| &cpu.caches).collect();
    if all_caches.is_empty() {
        return None;
    }

    let mut by_level: HashMap<String, Vec<&sysfs::CpuCache>> = HashMap::new();
    for cache in all_caches {
        let type_suffix = match cache.typ {
            sysfs::CacheType::Instruction => "i",
            sysfs::CacheType::Data => "d",
            sysfs::CacheType::Unified => "",
        };
        let level_key = format!("L{}{}", cache.level, type_suffix);
        by_level.entry(level_key).or_default().push(cache);
    }

    let mut cache_info = CpuInfo::new("Caches (sum of all)", "");
    let mut levels: Vec<_> = by_level.into_iter().collect();
    for (level, caches) in levels.iter_mut() {
        caches.sort_by(|a, b| a.shared_cpu_map.cmp(&b.shared_cpu_map));
        caches.dedup_by_key(|c| c.shared_cpu_map.clone());

        let count = caches.len();
        let size_total = caches
            .iter()
            .fold(0_u64, |acc, c| acc + c.size.size_bytes());
        let size = sysfs::CacheSize::new(size_total);

        cache_info.add_child(CpuInfo::new(
            level,
            &format!(
                "{} ({} instances)",
                if bytes {
                    size.raw()
                } else {
                    size.human_readable()
                },
                count
            ),
        ));
    }
    cache_info.children.sort_by(|a, b| a.field.cmp(&b.field));

    Some(cache_info)
}

fn print_output(infos: &[CpuInfo], out_opts: &OutputOptions) {
    if out_opts.json {
        println!("{}", to_json(infos));
        return;
    }

    fn indentation(depth: usize) -> usize {
        depth * 2
    }

    fn get_max_field_width(info: &CpuInfo, depth: usize) -> usize {
        let max_child_width = info
            .children
            .iter()
            .map(|entry| get_max_field_width(entry, depth + 1))
            .max()
            .unwrap_or(0);
        let own_width = indentation(depth) + info.field.len();
        cmp::max(own_width, max_child_width)
    }

    fn print_entries(entries: &[CpuInfo], depth: usize, max_field_width: usize) {
        for entry in entries {
            let margin = indentation(depth);
            let padding = max_field_width.saturating_sub(margin + entry.field.len());
            println!(
                "{}{}:{} {}",
                " ".repeat(margin),
                entry.field,
                " ".repeat(padding),
                entry.data
            );
            print_entries(&entry.children, depth + 1, max_field_width);
        }
    }

    let max_field_width = infos
        .iter()
        .map(|info| get_max_field_width(info, 0))
        .max()
        .unwrap_or(0);

    print_entries(infos, 0, max_field_width);
}

/// Minimal hand-rolled JSON serializer for the `CpuInfo` tree (avoids
/// pulling in `serde`/`serde_json`, which this workspace doesn't
/// otherwise depend on). Matches the shape upstream `lscpu --json`
/// produces: `{"lscpu": [{"field": ..., "data": ..., "children": [...]}]}`.
fn to_json(infos: &[CpuInfo]) -> String {
    fn escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }

    fn entry_json(info: &CpuInfo) -> String {
        let mut s = format!(
            "{{\"field\": \"{}\", \"data\": \"{}\"",
            escape(&info.field),
            escape(&info.data)
        );
        if !info.children.is_empty() {
            let children: Vec<String> = info.children.iter().map(entry_json).collect();
            s.push_str(&format!(", \"children\": [{}]", children.join(", ")));
        }
        s.push('}');
        s
    }

    let entries: Vec<String> = infos.iter().map(entry_json).collect();
    format!("{{\n  \"lscpu\": [{}]\n}}", entries.join(", "))
}

fn find_cpuinfo_value(contents: &str, key: &str) -> Option<String> {
    for line in contents.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim() == key {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// Best-effort compile-time architecture name (matches upstream's
/// simplification: it does not detect running a 32-bit binary under a
/// 64-bit kernel).
fn get_architecture() -> String {
    if cfg!(target_arch = "x86") {
        "x86".to_string()
    } else if cfg!(target_arch = "x86_64") {
        "x86_64".to_string()
    } else if cfg!(target_arch = "aarch64") {
        "aarch64".to_string()
    } else {
        "Unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn bytes_flag_reaches_cache_totals_formatting() {
        // Regression test for the bug where `-B/--bytes` was parsed but
        // never threaded through to cache-size formatting, so `-B` output
        // always stayed human-readable (e.g. "32K") instead of a bare
        // integer byte count.
        let topo = sysfs::CpuTopology {
            cpus: vec![sysfs::Cpu {
                pkg_id: 0,
                core_id: 0,
                caches: vec![sysfs::CpuCache {
                    typ: sysfs::CacheType::Unified,
                    level: 2,
                    size: sysfs::CacheSize::new(32 * 1024),
                    shared_cpu_map: "0".to_string(),
                }],
            }],
        };

        let human = calculate_cache_totals(&topo, false).unwrap();
        assert!(!human.children[0].data.starts_with("32768"), "{:?}", human.children[0].data);

        let bytes = calculate_cache_totals(&topo, true).unwrap();
        assert!(bytes.children[0].data.starts_with("32768"), "{:?}", bytes.children[0].data);
    }

    #[test]
    fn find_cpuinfo_value_extracts_field() {
        let contents = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Foo CPU\n";
        assert_eq!(
            find_cpuinfo_value(contents, "vendor_id"),
            Some("GenuineIntel".to_string())
        );
        assert_eq!(
            find_cpuinfo_value(contents, "model name"),
            Some("Foo CPU".to_string())
        );
        assert_eq!(find_cpuinfo_value(contents, "nonexistent"), None);
    }

    #[test]
    fn cpu_info_tree_and_json_roundtrip() {
        let mut root = CpuInfo::new("Architecture", "x86_64");
        root.add_child(CpuInfo::new("Byte Order", "Little Endian"));
        let json = to_json(&[root]);
        assert!(json.contains("\"field\": \"Architecture\""));
        assert!(json.contains("\"data\": \"x86_64\""));
        assert!(json.contains("Byte Order"));
    }

    #[test]
    fn collect_from_synthetic_fixture() {
        let dir = std::env::temp_dir().join(format!("user-lscpu-collect-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cpu_base = dir.join("cpu");
        let kernel_base = dir.join("kernel");
        write(&kernel_base.join("cpu_byteorder"), "little\n");
        write(&cpu_base.join("online"), "0\n");
        write(&cpu_base.join("cpu0/topology/physical_package_id"), "0\n");
        write(&cpu_base.join("cpu0/topology/core_id"), "0\n");
        write(&cpu_base.join("cpu0/cache/index0/type"), "Data\n");
        write(&cpu_base.join("cpu0/cache/index0/level"), "1\n");
        write(&cpu_base.join("cpu0/cache/index0/size"), "32K\n");
        write(&cpu_base.join("cpu0/cache/index0/shared_cpu_map"), "1\n");
        write(&cpu_base.join("vulnerabilities/meltdown"), "Not affected\n");

        let cpuinfo = dir.join("cpuinfo");
        write(
            &cpuinfo,
            "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Test CPU\ncpu family\t: 6\nmodel\t\t: 1\naddress sizes\t: 39 bits physical\n",
        );

        let infos = collect(&cpu_base, &kernel_base, cpuinfo.to_str().unwrap(), false);
        assert!(!infos.is_empty());
        assert_eq!(infos[0].field, "Architecture");
        assert_eq!(infos[1].field, "CPU(s)");
        assert_eq!(infos[1].data, "1");

        let vendor = infos.iter().find(|i| i.field == "Vendor ID").unwrap();
        assert_eq!(vendor.data, "GenuineIntel");

        let vulns = infos.iter().find(|i| i.field == "Vulnerabilities");
        assert!(vulns.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_against_real_sandbox_sysfs() {
        let cpu_base = Path::new(PATH_SYS_CPU);
        if !cpu_base.exists() {
            return;
        }
        let infos = collect(cpu_base, Path::new(PATH_SYS_KERNEL), PATH_PROC_CPUINFO, false);
        assert!(!infos.is_empty());
    }
}
