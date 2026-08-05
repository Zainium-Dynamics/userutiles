//! Sysfs-reading layer for `lscpu`.
//!
//! Every reader takes an explicit base directory rather than hard-coding
//! `/sys/devices/system/cpu` so tests can point at a synthetic fixture
//! directory shaped like the real sysfs tree (see `chcpu`'s `SysCpu` for
//! the same pattern).
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// One CPU vulnerability/mitigation entry, e.g. `Spectre_v1` -> `Mitigation: ...`.
pub struct CpuVulnerability {
    pub name: String,
    pub mitigation: String,
}

/// The set of online CPUs and their topology/cache info.
pub struct CpuTopology {
    pub cpus: Vec<Cpu>,
}

/// One online CPU: its package/core id and the caches it reports.
#[derive(Debug)]
pub struct Cpu {
    pub pkg_id: usize,
    pub core_id: usize,
    pub caches: Vec<CpuCache>,
}

/// One cache level reported under `cpuN/cache/indexM`.
#[derive(Debug)]
pub struct CpuCache {
    pub typ: CacheType,
    pub level: usize,
    pub size: CacheSize,
    pub shared_cpu_map: String,
}

/// A cache/memory size in bytes, with human-readable formatting.
#[derive(Debug, Clone, Copy)]
pub struct CacheSize(u64);

#[derive(Debug)]
pub enum CacheType {
    Data,
    Instruction,
    Unified,
}

impl CpuTopology {
    /// Build the CPU topology by reading `online` and each `cpuN/topology/*`
    /// and `cpuN/cache/*` entry under `cpu_base` (normally
    /// `/sys/devices/system/cpu`).
    pub fn read(cpu_base: &Path) -> Self {
        let mut out: Vec<Cpu> = vec![];
        let online_cpus = parse_cpu_list(&read_online_cpus(cpu_base));

        for cpu_index in online_cpus {
            let cpu_dir = cpu_base.join(format!("cpu{cpu_index}"));

            let pkg_id = fs::read_to_string(cpu_dir.join("topology/physical_package_id"))
                .ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(0);

            let core_id = fs::read_to_string(cpu_dir.join("topology/core_id"))
                .ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(0);

            let caches = read_cpu_caches(&cpu_dir);

            out.push(Cpu {
                pkg_id,
                core_id,
                caches,
            });
        }
        Self { cpus: out }
    }

    /// Number of distinct physical sockets (unique `physical_package_id`s).
    pub fn socket_count(&self) -> usize {
        let physical_sockets: HashSet<_> = self.cpus.iter().map(|cpu| cpu.pkg_id).collect();
        physical_sockets.len().max(1)
    }

    /// Number of distinct cores (unique `core_id`s).
    pub fn core_count(&self) -> usize {
        let core_ids: HashSet<_> = self.cpus.iter().map(|cpu| cpu.core_id).collect();
        core_ids.len().max(1)
    }
}

impl CacheSize {
    pub fn new(size: u64) -> Self {
        Self(size)
    }

    /// Parse a kernel cache-size string like `32K`, `1M`, `512` (bytes).
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some(num) = s.strip_suffix('K') {
            num.trim().parse::<u64>().ok().map(|n| Self(n * 1024))
        } else if let Some(num) = s.strip_suffix('M') {
            num.trim()
                .parse::<u64>()
                .ok()
                .map(|n| Self(n * 1024 * 1024))
        } else if let Some(num) = s.strip_suffix('G') {
            num.trim()
                .parse::<u64>()
                .ok()
                .map(|n| Self(n * 1024 * 1024 * 1024))
        } else {
            s.parse::<u64>().ok().map(Self)
        }
    }

    pub fn size_bytes(&self) -> u64 {
        self.0
    }

    pub fn raw(&self) -> String {
        format!("{}", self.0)
    }

    pub fn human_readable(&self) -> String {
        let (unit, denominator) = match self.0 {
            x if x < 1024_u64.pow(1) => ("B", 1024_u64.pow(0)),
            x if x < 1024_u64.pow(2) => ("KiB", 1024_u64.pow(1)),
            x if x < 1024_u64.pow(3) => ("MiB", 1024_u64.pow(2)),
            x if x < 1024_u64.pow(4) => ("GiB", 1024_u64.pow(3)),
            x if x < 1024_u64.pow(5) => ("TiB", 1024_u64.pow(4)),
            _ => return format!("{} bytes", self.0),
        };
        let scaled_size = self.0 / denominator;
        format!("{scaled_size} {unit}")
    }
}

/// Read the online-CPU list string (e.g. `0-3`) from `cpu_base/online`.
pub fn read_online_cpus(cpu_base: &Path) -> String {
    fs::read_to_string(cpu_base.join("online"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn read_cpu_caches(cpu_dir: &Path) -> Vec<CpuCache> {
    let mut caches: Vec<CpuCache> = vec![];
    let cache_dir = match fs::read_dir(cpu_dir.join("cache")) {
        Ok(d) => d,
        Err(_) => return caches,
    };

    let mut cache_paths: Vec<PathBuf> = cache_dir
        .flatten()
        .filter(|x| x.path().is_dir())
        .map(|x| x.path())
        .collect();
    cache_paths.sort();

    for cache_path in cache_paths {
        let type_string = match fs::read_to_string(cache_path.join("type")) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let c_type = match type_string.trim() {
            "Unified" => CacheType::Unified,
            "Data" => CacheType::Data,
            "Instruction" => CacheType::Instruction,
            _ => continue,
        };

        let c_level = match fs::read_to_string(cache_path.join("level"))
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
        {
            Some(l) => l,
            None => continue,
        };

        let size_string = match fs::read_to_string(cache_path.join("size")) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let c_size = match CacheSize::parse(&size_string) {
            Some(s) => s,
            None => continue,
        };

        let shared_cpu_map = fs::read_to_string(cache_path.join("shared_cpu_map"))
            .unwrap_or_default()
            .trim()
            .to_string();

        caches.push(CpuCache {
            level: c_level,
            size: c_size,
            typ: c_type,
            shared_cpu_map,
        });
    }

    caches
}

/// Whether frequency boost is enabled, from `cpu_base/cpufreq/boost`.
pub fn read_freq_boost_state(cpu_base: &Path) -> Option<bool> {
    fs::read_to_string(cpu_base.join("cpufreq/boost"))
        .map(|content| content.trim() == "1")
        .ok()
}

/// Read all reported CPU vulnerabilities/mitigations from
/// `cpu_base/vulnerabilities/*`, sorted by file name.
pub fn read_cpu_vulnerabilities(cpu_base: &Path) -> Vec<CpuVulnerability> {
    let mut out: Vec<CpuVulnerability> = vec![];

    if let Ok(dir) = fs::read_dir(cpu_base.join("vulnerabilities")) {
        let mut files: Vec<_> = dir
            .flatten()
            .map(|x| x.path())
            .filter(|x| !x.is_dir())
            .collect();

        files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for file in files {
            if let Ok(content) = fs::read_to_string(&file) {
                let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.is_empty() {
                    continue;
                }
                let mut chars = name.chars();
                let capitalized = match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => continue,
                };

                out.push(CpuVulnerability {
                    name: capitalized.replace('_', " "),
                    mitigation: content.trim().to_string(),
                });
            }
        }
    }

    out
}

/// Read the CPU byte order from `kernel_base/cpu_byteorder` (normally
/// `/sys/kernel/cpu_byteorder`).
pub fn read_cpu_byte_order(kernel_base: &Path) -> Option<&'static str> {
    if let Ok(byte_order) = fs::read_to_string(kernel_base.join("cpu_byteorder")) {
        match byte_order.trim() {
            "big" => return Some("Big Endian"),
            "little" => return Some("Little Endian"),
            _ => {}
        }
    }
    None
}

/// Parse a kernel-style CPU list (`1,3-6,8`) into individual indices.
pub fn parse_cpu_list(list: &str) -> Vec<usize> {
    let mut out: Vec<usize> = vec![];

    if list.trim().is_empty() {
        return out;
    }

    for part in list.trim().split(',') {
        if let Some((lo, hi)) = part.split_once('-') {
            if let (Ok(lo), Ok(hi)) = (lo.parse::<usize>(), hi.parse::<usize>()) {
                for idx in lo..=hi {
                    out.push(idx);
                }
            }
        } else if let Ok(idx) = part.parse::<usize>() {
            out.push(idx);
        }
    }

    out
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
    fn test_parse_cache_size() {
        assert_eq!(CacheSize::parse("512").unwrap().size_bytes(), 512);
        assert_eq!(CacheSize::parse("1K").unwrap().size_bytes(), 1024);
        assert_eq!(CacheSize::parse("1M").unwrap().size_bytes(), 1024 * 1024);
        assert_eq!(
            CacheSize::parse("32M").unwrap().size_bytes(),
            32 * 1024 * 1024
        );
    }

    #[test]
    fn test_human_readable() {
        assert_eq!(CacheSize::new(1023).human_readable(), "1023 B");
        assert_eq!(CacheSize::new(1024).human_readable(), "1 KiB");
        assert_eq!(CacheSize::new(1024 * 1024).human_readable(), "1 MiB");
        assert_eq!(CacheSize::new(1023).raw(), "1023");
    }

    #[test]
    fn test_parse_cpu_list() {
        assert_eq!(parse_cpu_list(""), Vec::<usize>::new());
        assert_eq!(parse_cpu_list("1-3"), vec![1, 2, 3]);
        assert_eq!(parse_cpu_list("1,2,3"), vec![1, 2, 3]);
        assert_eq!(parse_cpu_list("1,3-6,8"), vec![1, 3, 4, 5, 6, 8]);
    }

    #[test]
    fn reads_synthetic_cpu_tree() {
        let dir = std::env::temp_dir().join(format!(
            "user-lscpu-test-{}-{}",
            std::process::id(),
            "synthetic"
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        write(&dir.join("online"), "0-1\n");
        for cpu in 0..2 {
            let base = dir.join(format!("cpu{cpu}"));
            write(&base.join("topology/physical_package_id"), "0\n");
            write(&base.join("topology/core_id"), &format!("{cpu}\n"));
            write(&base.join("cache/index0/type"), "Data\n");
            write(&base.join("cache/index0/level"), "1\n");
            write(&base.join("cache/index0/size"), "32K\n");
            write(&base.join("cache/index0/shared_cpu_map"), "1\n");
        }
        write(&dir.join("cpufreq/boost"), "1\n");
        write(&dir.join("vulnerabilities/spectre_v1"), "Mitigation: foo\n");

        let topo = CpuTopology::read(&dir);
        assert_eq!(topo.cpus.len(), 2);
        assert_eq!(topo.socket_count(), 1);
        assert_eq!(topo.core_count(), 2);
        assert_eq!(topo.cpus[0].caches.len(), 1);
        assert_eq!(topo.cpus[0].caches[0].level, 1);
        assert_eq!(topo.cpus[0].caches[0].size.size_bytes(), 32 * 1024);

        assert_eq!(read_online_cpus(&dir), "0-1");
        assert_eq!(read_freq_boost_state(&dir), Some(true));

        let vulns = read_cpu_vulnerabilities(&dir);
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0].name, "Spectre v1");
        assert_eq!(vulns[0].mitigation, "Mitigation: foo");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_real_sandbox_sysfs_if_present() {
        let real = Path::new("/sys/devices/system/cpu");
        if !real.exists() {
            return;
        }
        let topo = CpuTopology::read(real);
        // Just confirm this doesn't panic and produces at least one CPU
        // if the sandbox exposes any online CPUs at all.
        let online = read_online_cpus(real);
        if !online.is_empty() {
            assert!(!topo.cpus.is_empty());
        }
    }
}
