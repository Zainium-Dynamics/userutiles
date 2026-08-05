//! user lsmem — list the ranges of available memory and their online
//! status, from the `/sys/devices/system/memory` sysfs tree.
use std::fs;
use std::path::{Path, PathBuf};

use usercore::Ui;

const HELP: &str = "Usage: lsmem [options]\n\
List the ranges of available memory with their online status.\n\n\
  -a, --all             list each individual memory block, don't coalesce\n\
  -b, --bytes           print sizes in bytes rather than human-readable\n\
  -J, --json             use JSON output format\n\
  -n, --noheadings      don't print headings\n\
  -o, --output <list>   output columns\n\
      --output-all      output all columns\n\
  -P, --pairs            use key=\"value\" output format\n\
  -r, --raw             use raw output format (space separated, no alignment)\n\
  -S, --split <list>    split ranges by specified columns\n\
  -s, --sysroot <dir>   use the specified directory as system root\n\
      --summary[=when]  print summary information (never, always, or only)\n\
  -h, --help            display this help and exit\n\
      --version         output version information and exit\n";

const ALL_COLUMNS: [&str; 7] = ["RANGE", "SIZE", "STATE", "REMOVABLE", "BLOCK", "NODE", "ZONES"];
const DEFAULT_COLUMNS: [&str; 5] = ["RANGE", "SIZE", "STATE", "REMOVABLE", "BLOCK"];
/// Columns whose differing values across otherwise-adjacent blocks force
/// them apart even without `-a`/`-S` — a coalesced row can't show two
/// different states for a single displayed column.
const SPLITTABLE_COLUMNS: [&str; 4] = ["STATE", "REMOVABLE", "NODE", "ZONES"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Summary {
    WithTable,
    Never,
    Only,
}

#[derive(Default)]
struct Options {
    all: bool,
    bytes: bool,
    json: bool,
    pairs: bool,
    noheadings: bool,
    raw: bool,
    output: Option<String>,
    output_all: bool,
    split: Option<String>,
    sysroot: Option<String>,
}

/// Entry point for the `lsmem` utility.
pub fn run() -> i32 {
    let ui = Ui::new("lsmem");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut opts = Options::default();
    let mut summary = Summary::WithTable;
    let mut summary_explicit = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("lsmem (user_utils) 0.1.0");
                return 0;
            }
            "-a" | "--all" => opts.all = true,
            "-b" | "--bytes" => opts.bytes = true,
            "-J" | "--json" => opts.json = true,
            "-P" | "--pairs" => opts.pairs = true,
            "-n" | "--noheadings" => opts.noheadings = true,
            "-r" | "--raw" => opts.raw = true,
            "--output-all" => opts.output_all = true,
            "-o" | "--output" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                opts.output = Some(v.clone());
            }
            "-S" | "--split" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                opts.split = Some(v.clone());
            }
            "-s" | "--sysroot" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                opts.sysroot = Some(v.clone());
            }
            "--summary" => {
                summary = Summary::Only;
                summary_explicit = true;
            }
            s if s.starts_with("--summary=") => {
                summary = match &s["--summary=".len()..] {
                    "never" => Summary::Never,
                    "always" => Summary::WithTable,
                    "only" => Summary::Only,
                    other => {
                        ui.err(&format!("invalid --summary argument: '{other}'"));
                        return 1;
                    }
                };
                summary_explicit = true;
            }
            other => {
                ui.err(&format!("unknown option -- '{other}'"));
                return 1;
            }
        }
        i += 1;
    }

    let columns = match resolve_columns(opts.output.as_deref(), opts.output_all) {
        Ok(c) => c,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };
    let split_keys = match resolve_split_keys(&columns, opts.split.as_deref()) {
        Ok(k) => k,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    let sysroot = opts.sysroot.as_deref().unwrap_or("/");
    let sys = Path::new(sysroot).join("sys/devices/system/memory");
    let block_size = match read_block_size(&sys) {
        Ok(sz) => sz,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };

    let mut blocks = match enumerate_blocks(&sys, block_size) {
        Ok(b) => b,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };
    blocks.sort_by_key(|b| b.index);

    let rows = if opts.all { blocks.clone() } else { coalesce(&blocks, &split_keys) };

    // The summary lines are a plain-table-only convention: real lsmem
    // doesn't append them to `-J`/`-P` output unless `--summary` is given
    // explicitly (verified: `lsmem -J` alone has no trailing summary).
    let summary = if (opts.json || opts.pairs) && !summary_explicit {
        Summary::Never
    } else {
        summary
    };

    if summary != Summary::Only {
        print_table(&columns, &rows, &opts);
    }

    if summary != Summary::Never {
        if summary == Summary::WithTable {
            println!();
        }
        print_summary(&blocks, block_size, opts.bytes);
    }

    0
}

fn resolve_columns(output: Option<&str>, output_all: bool) -> Result<Vec<&'static str>, String> {
    let base = || -> Vec<&'static str> {
        if output_all { ALL_COLUMNS.to_vec() } else { DEFAULT_COLUMNS.to_vec() }
    };
    let Some(spec) = output else {
        return Ok(base());
    };
    let (append, list_str) = match spec.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, spec),
    };
    let mut list: Vec<&'static str> = Vec::new();
    for name in list_str.split(',') {
        let Some(&canonical) = ALL_COLUMNS.iter().find(|&&c| c == name) else {
            return Err(format!("unknown column: {name}"));
        };
        list.push(canonical);
    }
    if list.is_empty() {
        return Err(format!("unknown column: {spec}"));
    }
    if append {
        let mut columns = base();
        columns.extend(list);
        Ok(columns)
    } else {
        Ok(list)
    }
}

/// A row can't be coalesced across blocks that differ in a column it's
/// displaying, so `STATE`/`REMOVABLE` are always split keys, and
/// `NODE`/`ZONES` become split keys too whenever they're part of the
/// selected output columns (matches real `lsmem` — including `NODE`/`ZONES`
/// via `-o`/`--output-all` splits ranges even without an explicit `-S`).
/// `-S/--split` adds any further named columns explicitly.
fn resolve_split_keys(columns: &[&'static str], split: Option<&str>) -> Result<Vec<&'static str>, String> {
    let mut keys: Vec<&'static str> = vec!["STATE", "REMOVABLE"];
    for &optional in &["NODE", "ZONES"] {
        if columns.contains(&optional) && !keys.contains(&optional) {
            keys.push(optional);
        }
    }
    if let Some(spec) = split {
        for name in spec.split(',') {
            let Some(&canonical) = SPLITTABLE_COLUMNS.iter().find(|&&c| c == name) else {
                return Err(format!("unknown --split column: {name}"));
            };
            if !keys.contains(&canonical) {
                keys.push(canonical);
            }
        }
    }
    Ok(keys)
}

struct Block {
    index: u64,
    start: u64,
    len: u64,
    state: String,
    removable: bool,
    node: Option<u64>,
    zones: String,
    first_index: u64,
    last_index: u64,
}

impl Clone for Block {
    fn clone(&self) -> Self {
        Block {
            index: self.index,
            start: self.start,
            len: self.len,
            state: self.state.clone(),
            removable: self.removable,
            node: self.node,
            zones: self.zones.clone(),
            first_index: self.first_index,
            last_index: self.last_index,
        }
    }
}

fn read_block_size(sys: &Path) -> Result<u64, String> {
    let raw = fs::read_to_string(sys.join("block_size_bytes"))
        .map_err(|e| format!("cannot read block_size_bytes: {e}"))?;
    let trimmed = raw.trim().trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(trimmed, 16).map_err(|_| format!("invalid block_size_bytes: '{raw}'"))
}

fn enumerate_blocks(sys: &Path, block_size: u64) -> Result<Vec<Block>, String> {
    let entries = fs::read_dir(sys).map_err(|e| format!("cannot read {}: {e}", sys.display()))?;

    let mut blocks = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read directory entry: {e}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(idx_str) = name.strip_prefix("memory") else {
            continue;
        };
        let Ok(index) = idx_str.parse::<u64>() else {
            continue;
        };
        let dir: PathBuf = entry.path();

        let state = read_state(&dir);
        let removable = fs::read_to_string(dir.join("removable"))
            .ok()
            .map(|s| s.trim() == "1")
            .unwrap_or(false);
        let node = read_node(&dir);
        let zones = fs::read_to_string(dir.join("valid_zones"))
            .map(|s| capitalize_zones(s.trim()))
            .unwrap_or_default();

        blocks.push(Block {
            index,
            start: index * block_size,
            len: block_size,
            state,
            removable,
            node,
            zones,
            first_index: index,
            last_index: index,
        });
    }
    Ok(blocks)
}

/// The kernel reports `valid_zones` words lowercase for the no-zone case
/// (`"none"`) but already-capitalized for real zone names (`"DMA32"`,
/// `"Normal"`); real `lsmem` title-cases every word for display
/// (`"none"` -> `"None"`), verified against the real binary.
fn capitalize_zones(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a block's NUMA node from its `nodeN` symlink (e.g.
/// `memory0/node0 -> ../../node/node0`), matching real `lsmem`'s NODE
/// column. `None` on systems/blocks with no NUMA node info.
fn read_node(dir: &Path) -> Option<u64> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(rest) = name.strip_prefix("node") {
            if let Ok(n) = rest.parse::<u64>() {
                return Some(n);
            }
        }
    }
    None
}

/// Reads a block's online state: prefers the `state` attribute (present on
/// newer kernels, one of "online"/"offline"/"going-offline"); falls back to
/// the boolean `online` attribute on older kernels.
fn read_state(dir: &Path) -> String {
    if let Ok(s) = fs::read_to_string(dir.join("state")) {
        return s.trim().to_string();
    }
    match fs::read_to_string(dir.join("online")) {
        Ok(s) if s.trim() == "1" => "online".to_string(),
        Ok(_) => "offline".to_string(),
        Err(_) => "unknown".to_string(),
    }
}

/// Merges adjacent blocks that share every column in `split_keys` into
/// ranges, matching lsmem's default (non `-a`) coalesced output.
fn coalesce(blocks: &[Block], split_keys: &[&str]) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    for b in blocks {
        if let Some(last) = out.last_mut() {
            let contiguous = last.start + last.len == b.start;
            let keys_match = split_keys.iter().all(|&key| match key {
                "STATE" => last.state == b.state,
                "REMOVABLE" => last.removable == b.removable,
                "NODE" => last.node == b.node,
                "ZONES" => last.zones == b.zones,
                _ => true,
            });
            if contiguous && keys_match {
                last.len += b.len;
                last.last_index = b.index;
                continue;
            }
        }
        out.push(b.clone());
    }
    out
}

fn cell(col: &str, row: &Block, bytes: bool) -> String {
    match col {
        "RANGE" => format!("0x{:016x}-0x{:016x}", row.start, row.start + row.len - 1),
        "SIZE" => {
            if bytes {
                row.len.to_string()
            } else {
                human_size(row.len)
            }
        }
        "STATE" => row.state.clone(),
        "REMOVABLE" => if row.removable { "yes" } else { "no" }.to_string(),
        "BLOCK" => {
            if row.first_index == row.last_index {
                row.first_index.to_string()
            } else {
                format!("{}-{}", row.first_index, row.last_index)
            }
        }
        "NODE" => row.node.map(|n| n.to_string()).unwrap_or_default(),
        "ZONES" => row.zones.clone(),
        _ => String::new(),
    }
}

fn column_width(col: &str) -> usize {
    match col {
        "RANGE" => 39,
        "SIZE" => 6,
        "STATE" => 10,
        "REMOVABLE" => 10,
        "BLOCK" => 5,
        "NODE" => 4,
        "ZONES" => 6,
        _ => 6,
    }
}

fn column_right_aligned(col: &str) -> bool {
    matches!(col, "SIZE" | "BLOCK" | "NODE" | "ZONES")
}

fn print_table(columns: &[&'static str], rows: &[Block], opts: &Options) {
    if opts.json {
        print_json(columns, rows, opts.bytes);
        return;
    }
    if opts.pairs {
        for row in rows {
            let cells: Vec<String> =
                columns.iter().map(|&c| format!("{c}=\"{}\"", cell(c, row, opts.bytes))).collect();
            println!("{}", cells.join(" "));
        }
        return;
    }

    if !opts.noheadings {
        if opts.raw {
            println!("{}", columns.join(" "));
        } else {
            let header: Vec<String> = columns
                .iter()
                .map(|&c| pad(c, column_width(c), column_right_aligned(c)))
                .collect();
            println!("{}", header.join(" ").trim_end());
        }
    }

    for row in rows {
        if opts.raw {
            let cells: Vec<String> = columns.iter().map(|&c| cell(c, row, opts.bytes)).collect();
            println!("{}", cells.join(" "));
        } else {
            let cells: Vec<String> = columns
                .iter()
                .map(|&c| pad(&cell(c, row, opts.bytes), column_width(c), column_right_aligned(c)))
                .collect();
            println!("{}", cells.join(" ").trim_end());
        }
    }
}

fn pad(s: &str, width: usize, right: bool) -> String {
    if right {
        format!("{s:>width$}")
    } else {
        format!("{s:<width$}")
    }
}

fn print_json(columns: &[&'static str], rows: &[Block], bytes: bool) {
    let mut out = String::new();
    out.push_str("{\n   \"memory\": [\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str(if i == 0 { "      {\n" } else { ",{\n" });
        for (j, &col) in columns.iter().enumerate() {
            out.push_str("         \"");
            out.push_str(&col.to_lowercase());
            out.push_str("\": ");
            out.push_str(&json_value(col, row, bytes));
            if j + 1 < columns.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("      }");
    }
    out.push('\n');
    out.push_str("   ]\n}");
    println!("{out}");
}

fn json_value(col: &str, row: &Block, bytes: bool) -> String {
    match col {
        "SIZE" if bytes => row.len.to_string(),
        "REMOVABLE" => row.removable.to_string(),
        "NODE" => row.node.map(|n| n.to_string()).unwrap_or_else(|| "null".to_string()),
        _ => json_string(&cell(col, row, bytes)),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn print_summary(blocks: &[Block], block_size: u64, bytes: bool) {
    let total_online: u64 = blocks.iter().filter(|b| b.state == "online").map(|b| b.len).sum();
    let total_offline: u64 = blocks.iter().filter(|b| b.state != "online").map(|b| b.len).sum();

    let fmt = |n: u64| if bytes { n.to_string() } else { human_size(n) };
    summary_line("Memory block size:", &fmt(block_size));
    summary_line("Total online memory:", &fmt(total_online));
    summary_line("Total offline memory:", &fmt(total_offline));
}

/// Every summary line right-aligns its value to a fixed total width of 38
/// columns, verified against the real binary across all three (differently
/// long) labels.
fn summary_line(label: &str, value: &str) {
    let width = 38usize.saturating_sub(label.len());
    println!("{label}{value:>width$}");
}

/// Real util-linux 2^n scaling + one-decimal rounding, up to `E` (exabyte)
/// — fixes the earlier version's naive fallback, which only handled exact
/// multiples cleanly and used a narrower unit set.
fn human_size(bytes: u64) -> String {
    const UNITS: [char; 7] = ['B', 'K', 'M', 'G', 'T', 'P', 'E'];
    let mut exp = 0usize;
    let mut n = bytes;
    while n >= 1024 && exp < UNITS.len() - 1 {
        n /= 1024;
        exp += 1;
    }
    if exp == 0 {
        return format!("{bytes}B");
    }
    let scale = 1u64 << (10 * exp);
    let whole = bytes / scale;
    let remainder = bytes % scale;
    if remainder == 0 {
        format!("{whole}{}", UNITS[exp])
    } else {
        let tenths = (remainder * 10 + scale / 2) / scale;
        if tenths >= 10 {
            format!("{}{}", whole + 1, UNITS[exp])
        } else {
            format!("{whole}.{tenths}{}", UNITS[exp])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(index: u64, start: u64, len: u64, state: &str, removable: bool) -> Block {
        Block {
            index,
            start,
            len,
            state: state.into(),
            removable,
            node: Some(0),
            zones: "Normal".into(),
            first_index: index,
            last_index: index,
        }
    }

    #[test]
    fn human_size_formats_exact_units() {
        assert_eq!(human_size(1u64 << 30), "1G");
        assert_eq!(human_size(128 * (1u64 << 20)), "128M");
        assert_eq!(human_size(512), "512B");
    }

    #[test]
    fn human_size_rounds_non_power_of_two() {
        // Matches real util-linux rounding, e.g. 3.1G/4.9G-style values.
        assert_eq!(human_size(3_355_443_200), "3.1G");
        assert_eq!(human_size(163_840), "160K");
    }

    #[test]
    fn coalesce_merges_contiguous_matching_blocks() {
        let blocks = vec![
            block(0, 0, 100, "online", false),
            block(1, 100, 100, "online", false),
            block(2, 200, 100, "offline", false),
        ];
        let merged = coalesce(&blocks, &["STATE", "REMOVABLE"]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].len, 200);
        assert_eq!(merged[0].last_index, 1);
        assert_eq!(merged[1].state, "offline");
    }

    #[test]
    fn coalesce_does_not_merge_differing_state() {
        let blocks = vec![block(0, 0, 100, "online", false), block(1, 100, 100, "offline", false)];
        assert_eq!(coalesce(&blocks, &["STATE", "REMOVABLE"]).len(), 2);
    }

    #[test]
    fn coalesce_splits_on_zones_when_requested() {
        let mut blocks = vec![block(0, 0, 100, "online", false), block(1, 100, 100, "online", false)];
        blocks[1].zones = "DMA32".into();
        // Without ZONES as a split key, these merge (same state/removable).
        assert_eq!(coalesce(&blocks, &["STATE", "REMOVABLE"]).len(), 1);
        // With ZONES as a split key (as happens when ZONES is a selected
        // output column), differing zones keep them apart.
        assert_eq!(coalesce(&blocks, &["STATE", "REMOVABLE", "ZONES"]).len(), 2);
    }

    #[test]
    fn read_block_size_parses_hex_from_fixture() {
        let dir = std::env::temp_dir().join(format!("user-lsmem-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("block_size_bytes"), "8000000\n").unwrap();
        assert_eq!(read_block_size(&dir).unwrap(), 0x8000000);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_columns_defaults_and_output_all() {
        assert_eq!(resolve_columns(None, false).unwrap(), DEFAULT_COLUMNS.to_vec());
        assert_eq!(resolve_columns(None, true).unwrap(), ALL_COLUMNS.to_vec());
        assert_eq!(resolve_columns(Some("RANGE,NODE"), false).unwrap(), vec!["RANGE", "NODE"]);
    }

    #[test]
    fn resolve_split_keys_adds_node_zones_when_selected_as_columns() {
        let keys = resolve_split_keys(&["RANGE", "SIZE", "ZONES"], None).unwrap();
        assert!(keys.contains(&"ZONES"));
        assert!(!keys.contains(&"NODE"));
    }
}
