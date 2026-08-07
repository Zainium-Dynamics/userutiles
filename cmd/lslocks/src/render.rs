//! Column definitions and output rendering (text table / raw / JSON) for
//! `lslocks`.
use crate::Lock;

pub(crate) const ALL_COLUMNS: [&str; 13] = [
    "COMMAND", "PID", "TYPE", "SIZE", "INODE", "MAJ:MIN", "MODE", "M", "START", "END", "PATH",
    "BLOCKER", "HOLDERS",
];

pub(crate) const DEFAULT_COLUMNS: [&str; 9] = [
    "COMMAND", "PID", "TYPE", "SIZE", "MODE", "M", "START", "END", "PATH",
];

/// `(name, type, description)` for `-H/--list-columns`, in `ALL_COLUMNS` order.
const COLUMN_REFERENCE: [(&str, &str, &str); 13] = [
    (
        "COMMAND",
        "string",
        "command of the process holding the lock",
    ),
    ("PID", "integer", "PID of the process holding the lock"),
    ("TYPE", "string", "kind of lock"),
    (
        "SIZE",
        "string|number",
        "size of the lock, use <number> if --bytes is given",
    ),
    ("INODE", "integer", "inode number"),
    ("MAJ:MIN", "string", "major:minor device number"),
    ("MODE", "string", "lock access mode"),
    (
        "M",
        "boolean",
        "mandatory state of the lock: 0 (none), 1 (set)",
    ),
    ("START", "integer", "relative byte offset of the lock"),
    ("END", "integer", "ending offset of the lock"),
    ("PATH", "string", "path of the locked file"),
    ("BLOCKER", "integer", "PID of the process blocking the lock"),
    ("HOLDERS", "string", "holders of the lock"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Text,
    Raw,
    Json,
}

/// A single formatted cell, kept typed until the final render step so JSON
/// output can emit real numbers/booleans/arrays instead of stringifying
/// everything.
enum Cell {
    Str(Option<String>),
    Int(Option<i64>),
    Bool(bool),
    /// `HOLDERS`: one `"pid,command,fd"` entry per process holding a lock
    /// matching this one. Real `lslocks -J` emits this as a JSON array;
    /// text/raw modes join entries with `"; "` here (a simplification —
    /// real util-linux wraps each holder onto its own line within the
    /// table cell via libsmartcols, which this fixed-width renderer does
    /// not attempt to reproduce).
    List(Vec<String>),
}

impl Cell {
    fn display(&self) -> String {
        match self {
            Cell::Str(s) => s.clone().unwrap_or_default(),
            Cell::Int(i) => i.map(|v| v.to_string()).unwrap_or_default(),
            Cell::Bool(b) => if *b { "1" } else { "0" }.to_string(),
            Cell::List(items) => items.join("; "),
        }
    }

    fn json(&self) -> String {
        match self {
            Cell::Str(None) | Cell::Int(None) => "null".to_string(),
            Cell::Str(Some(s)) => json_string(s),
            Cell::Int(Some(i)) => i.to_string(),
            Cell::Bool(b) => b.to_string(),
            Cell::List(items) => {
                if items.is_empty() {
                    "[]".to_string()
                } else {
                    // Matches the real `lslocks -J` array layout: one
                    // holder per line, indented under the enclosing cell.
                    let rendered: Vec<String> = items
                        .iter()
                        .map(|s| format!("             {}", json_string(s)))
                        .collect();
                    format!("[\n{}\n         ]", rendered.join(",\n"))
                }
            }
        }
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
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Raw-mode field escaping: whitespace inside a value would otherwise be
/// ambiguous with the space-separated field format, so it's `\xNN`-escaped,
/// matching real `lslocks -r`/libsmartcols raw output.
fn raw_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("\\x20"),
            '\t' => out.push_str("\\x09"),
            '\n' => out.push_str("\\x0a"),
            c => out.push(c),
        }
    }
    out
}

fn column_width(col: &str) -> usize {
    match col {
        "COMMAND" => 15,
        "PID" => 7,
        "TYPE" => 6,
        "SIZE" => 6,
        "INODE" => 8,
        "MAJ:MIN" => 7,
        "MODE" => 6,
        "M" => 1,
        "START" => 10,
        "END" => 10,
        "PATH" => 4,
        "BLOCKER" => 7,
        "HOLDERS" => 7,
        _ => 4,
    }
}

fn column_right_aligned(col: &str) -> bool {
    matches!(
        col,
        "PID" | "SIZE" | "INODE" | "M" | "START" | "END" | "BLOCKER"
    )
}

/// Longest path/holders text to keep on one line before truncating with a
/// `...` marker. Real `lslocks` only truncates PATH/HOLDERS to the
/// terminal width when connected to an interactive terminal — piped output
/// (the common case, and the one that matters for pipeline correctness) is
/// left untruncated. This uses the same static cap rather than probing the
/// real terminal width via an ioctl, but only applies it under the same
/// condition (stdout is a tty), matching observed real `lslocks(1)` behavior
/// for the non-tty case (verified: piped output is never truncated).
const TRUNCATE_WIDTH: usize = 100;

/// `is_tty` is threaded in explicitly (rather than queried here) so this
/// stays a pure, unit-testable function; [`render_text`] passes the real
/// `stdout().is_terminal()` reading.
fn truncate(s: &str, notruncate: bool, is_tty: bool) -> String {
    if notruncate || !is_tty || s.chars().count() <= TRUNCATE_WIDTH {
        return s.to_string();
    }
    let mut out: String = s.chars().take(TRUNCATE_WIDTH.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

fn cell_for(
    col: &str,
    lock: &Lock,
    all_proc_locks: &[Lock],
    pid_locks: &[Lock],
    bytes: bool,
) -> Cell {
    match col {
        "COMMAND" => Cell::Str(lock.command_name.clone()),
        "PID" => Cell::Int(Some(lock.process_id as i64)),
        "TYPE" => Cell::Str(Some(lock.kind.clone())),
        "SIZE" => match lock.size {
            // Real lslocks leaves SIZE blank for a zero-byte file (e.g. a
            // pure advisory lock file with no data), in both human and
            // `--bytes` modes — not "0"/"0B". Matches empirically-verified
            // behavior against the system `lslocks(1)` binary.
            None | Some(0) => {
                if bytes {
                    Cell::Int(None)
                } else {
                    Cell::Str(None)
                }
            }
            Some(sz) => {
                if bytes {
                    Cell::Int(Some(sz as i64))
                } else {
                    Cell::Str(Some(human_size(sz)))
                }
            }
        },
        "INODE" => Cell::Int(Some(lock.inode as i64)),
        "MAJ:MIN" => Cell::Str(Some(format!("{}:{}", lock.major, lock.minor))),
        "MODE" => Cell::Str(Some(if lock.blocked {
            format!("{}*", lock.mode)
        } else {
            lock.mode.clone()
        })),
        "M" => Cell::Bool(lock.mandatory),
        "START" => Cell::Int(Some(lock.start as i64)),
        "END" => Cell::Int(Some(lock.end as i64)),
        "PATH" => Cell::Str(lock.path.clone()),
        "BLOCKER" => {
            let blocker = if lock.blocked && lock.id != -1 {
                all_proc_locks
                    .iter()
                    .find(|l| l.id == lock.id && !l.blocked)
                    .map(|l| l.process_id)
            } else {
                None
            };
            Cell::Int(blocker.map(|p| p as i64))
        }
        "HOLDERS" => {
            let holders: Vec<String> = pid_locks
                .iter()
                .filter(|l| {
                    l.start == lock.start
                        && l.end == lock.end
                        && l.inode == lock.inode
                        && l.major == lock.major
                        && l.minor == lock.minor
                        && l.mandatory == lock.mandatory
                        && l.blocked == lock.blocked
                        && l.kind == lock.kind
                        && l.mode == lock.mode
                })
                .map(|l| {
                    format!(
                        "{},{},{}",
                        l.process_id,
                        l.command_name.as_deref().unwrap_or(""),
                        l.file_descriptor
                    )
                })
                .collect();
            Cell::List(holders)
        }
        _ => Cell::Str(None),
    }
}

/// Human-readable size using the real util-linux 2^n scaling algorithm
/// (base-1024 units up to E, one decimal place when not an exact multiple).
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

pub(crate) fn render_locks(
    mode: OutputMode,
    columns: &[&str],
    bytes: bool,
    noheadings: bool,
    notruncate: bool,
    proc_locks: &[Lock],
    pid_locks: &[Lock],
) {
    match mode {
        OutputMode::Text => render_text(
            columns, bytes, noheadings, notruncate, proc_locks, pid_locks,
        ),
        OutputMode::Raw => render_raw(columns, bytes, noheadings, proc_locks, pid_locks),
        OutputMode::Json => render_json(columns, bytes, proc_locks, pid_locks),
    }
}

fn render_text(
    columns: &[&str],
    bytes: bool,
    noheadings: bool,
    notruncate: bool,
    proc_locks: &[Lock],
    pid_locks: &[Lock],
) {
    use std::io::IsTerminal;
    let is_tty = std::io::stdout().is_terminal();

    if !noheadings {
        let header: Vec<String> = columns
            .iter()
            .map(|c| pad(c, column_width(c), column_right_aligned(c)))
            .collect();
        println!("{}", header.join(" ").trim_end());
    }

    for lock in proc_locks.iter().rev() {
        let row: Vec<String> = columns
            .iter()
            .map(|c| {
                let cell = cell_for(c, lock, proc_locks, pid_locks, bytes);
                let text = truncate(&cell.display(), notruncate, is_tty);
                pad(&text, column_width(c), column_right_aligned(c))
            })
            .collect();
        println!("{}", row.join(" ").trim_end());
    }
}

fn pad(s: &str, width: usize, right_align: bool) -> String {
    if right_align {
        format!("{s:>width$}")
    } else {
        format!("{s:<width$}")
    }
}

fn render_raw(
    mode_columns: &[&str],
    bytes: bool,
    noheadings: bool,
    proc_locks: &[Lock],
    pid_locks: &[Lock],
) {
    if !noheadings {
        println!("{}", mode_columns.join(" "));
    }
    for lock in proc_locks.iter().rev() {
        let row: Vec<String> = mode_columns
            .iter()
            .map(|c| raw_escape(&cell_for(c, lock, proc_locks, pid_locks, bytes).display()))
            .collect();
        println!("{}", row.join(" "));
    }
}

fn render_json(columns: &[&str], bytes: bool, proc_locks: &[Lock], pid_locks: &[Lock]) {
    let mut out = String::new();
    out.push_str("{\n   \"locks\": [\n");
    for (i, lock) in proc_locks.iter().rev().enumerate() {
        out.push_str(if i == 0 { "      {\n" } else { ",{\n" });
        for (j, col) in columns.iter().enumerate() {
            let cell = cell_for(col, lock, proc_locks, pid_locks, bytes);
            out.push_str("         \"");
            out.push_str(&json_key(col));
            out.push_str("\": ");
            out.push_str(&cell.json());
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

fn json_key(col: &str) -> String {
    col.to_lowercase()
}

pub(crate) fn render_column_reference(mode: OutputMode) {
    match mode {
        OutputMode::Json => {
            let mut out = String::new();
            out.push_str("{\n   \"lslocks-columns\": [\n");
            for (i, (name, ty, desc)) in COLUMN_REFERENCE.iter().enumerate() {
                out.push_str(if i == 0 { "      {\n" } else { ",{\n" });
                out.push_str(&format!(
                    "         \"holder\": {},\n         \"type\": {},\n         \"description\": {}\n",
                    json_string(name),
                    json_string(&format!("<{ty}>")),
                    json_string(desc)
                ));
                out.push_str("      }");
            }
            out.push_str("\n   ]\n}");
            println!("{out}");
        }
        OutputMode::Raw => {
            for (name, ty, desc) in COLUMN_REFERENCE {
                println!("{name} <{ty}> {}", desc.replace(' ', "\\x20"));
            }
        }
        OutputMode::Text => {
            for (name, ty, desc) in COLUMN_REFERENCE {
                let typed = format!("<{ty}>");
                println!("{name:>7} {typed:<16}{desc}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_matches_util_linux_rounding() {
        assert_eq!(human_size(1397415936), "1.3G");
        assert_eq!(human_size(163840), "160K");
        assert_eq!(human_size(1024), "1K");
        assert_eq!(human_size(512), "512B");
        assert_eq!(human_size(0), "0B");
    }

    #[test]
    fn raw_escape_replaces_whitespace() {
        assert_eq!(raw_escape("a b"), "a\\x20b");
        assert_eq!(raw_escape("Profile Groups"), "Profile\\x20Groups");
        assert_eq!(raw_escape("noSpace"), "noSpace");
    }

    #[test]
    fn json_string_escapes_quotes_and_backslashes() {
        assert_eq!(json_string("he said \"hi\""), "\"he said \\\"hi\\\"\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn cell_holders_json_is_array() {
        let c = Cell::List(vec!["1,foo,3".to_string(), "2,bar,4".to_string()]);
        assert_eq!(
            c.json(),
            "[\n             \"1,foo,3\",\n             \"2,bar,4\"\n         ]"
        );
        assert_eq!(Cell::List(vec![]).json(), "[]");
    }

    #[test]
    fn truncate_adds_ellipsis_only_on_a_tty() {
        let long = "a".repeat(150);
        // Not a tty: never truncated, regardless of --notruncate.
        assert_eq!(truncate(&long, false, false), long);
        // A tty: truncated unless --notruncate is given.
        let short = truncate(&long, false, true);
        assert_eq!(short.chars().count(), TRUNCATE_WIDTH);
        assert!(short.ends_with("..."));
        assert_eq!(truncate(&long, true, true), long);
    }
}
