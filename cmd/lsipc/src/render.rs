//! Cell-value resolution and output rendering (table / export / newline /
//! raw / json / pretty) for `lsipc`.
use std::collections::HashMap;

use crate::columns::column_title;
use crate::model::{MsgEntry, SemElement, SemEntry, ShmEntry};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeFormat {
    Short,
    Full,
    Iso,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Table,
    Export,
    NewLine,
    Raw,
    Json,
    Pretty,
}

/// A view over one of the three entry kinds, so a single `cell_value`
/// function can serve all of them via the columns they share (`KEY`, `ID`,
/// `PERMS`, ...) plus their kind-specific columns.
pub(crate) enum Entry<'a> {
    Shm(&'a ShmEntry),
    Sem(&'a SemEntry),
    Msg(&'a MsgEntry),
}

struct Common {
    key: i32,
    id: i32,
    perms: u32,
    uid: u32,
    gid: u32,
    cuid: u32,
    cgid: u32,
    ctime: i64,
}

impl Entry<'_> {
    fn common(&self) -> Common {
        match self {
            Entry::Shm(e) => Common {
                key: e.key,
                id: e.id,
                perms: e.perms,
                uid: e.uid,
                gid: e.gid,
                cuid: e.cuid,
                cgid: e.cgid,
                ctime: e.ctime,
            },
            Entry::Sem(e) => Common {
                key: e.key,
                id: e.id,
                perms: e.perms,
                uid: e.uid,
                gid: e.gid,
                cuid: e.cuid,
                cgid: e.cgid,
                ctime: e.ctime,
            },
            Entry::Msg(e) => Common {
                key: e.key,
                id: e.id,
                perms: e.perms,
                uid: e.uid,
                gid: e.gid,
                cuid: e.cuid,
                cgid: e.cgid,
                ctime: e.ctime,
            },
        }
    }
}

/// Caches `uid`/`gid` -> name lookups across a whole render pass.
#[derive(Default)]
pub(crate) struct NameCache {
    users: HashMap<u32, Option<String>>,
    groups: HashMap<u32, Option<String>>,
}

impl NameCache {
    fn user(&mut self, uid: u32) -> Option<String> {
        self.users
            .entry(uid)
            .or_insert_with(|| user_name(uid))
            .clone()
    }
    fn group(&mut self, gid: u32) -> Option<String> {
        self.groups
            .entry(gid)
            .or_insert_with(|| group_name(gid))
            .clone()
    }
}

fn user_name(uid: u32) -> Option<String> {
    // SAFETY: `getpwuid` returns either null (handled below) or a pointer
    // into a libc-owned static buffer valid until the next passwd-database
    // call on this thread; we read `pw_name` out of it once, synchronously.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            None
        } else {
            Some(
                std::ffi::CStr::from_ptr((*pw).pw_name)
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }
}

fn group_name(gid: u32) -> Option<String> {
    // SAFETY: same reasoning as `user_name`, for the group database.
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            None
        } else {
            Some(
                std::ffi::CStr::from_ptr((*gr).gr_name)
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }
}

pub(crate) fn cell_value(
    entry: &Entry,
    col: &str,
    bytes: bool,
    numeric_perms: bool,
    time_format: TimeFormat,
    now: i64,
    names: &mut NameCache,
) -> Option<String> {
    let common = entry.common();
    match col {
        "KEY" => Some(format!("{:#010x}", common.key)),
        "ID" => Some(common.id.to_string()),
        "PERMS" => Some(describe_permissions(common.perms, numeric_perms)),
        "CUID" => Some(common.cuid.to_string()),
        "CGID" => Some(common.cgid.to_string()),
        "UID" => Some(common.uid.to_string()),
        "GID" => Some(common.gid.to_string()),
        "OWNER" => Some(names.user(common.uid).unwrap_or_else(|| common.uid.to_string())),
        "CUSER" => names.user(common.cuid),
        "USER" => names.user(common.uid),
        "CGROUP" => names.group(common.cgid),
        "GROUP" => names.group(common.gid),
        "CTIME" => format_time(time_format, now, common.ctime),

        _ => match entry {
            Entry::Shm(e) => shm_field(e, col, bytes, time_format, now),
            Entry::Sem(e) => sem_field(e, col, time_format, now),
            Entry::Msg(e) => msg_field(e, col, bytes, time_format, now),
        },
    }
}

fn shm_field(e: &ShmEntry, col: &str, bytes: bool, time_format: TimeFormat, now: i64) -> Option<String> {
    match col {
        "SIZE" => Some(size_desc(e.size, bytes)),
        "NATTCH" => Some(e.nattch.to_string()),
        // Unlike other blank fields (e.g. ATTACH/DETACH with no timestamp
        // yet), STATUS is always `Some`, even when empty — verified
        // against the real binary, which still prints a blank `Status:`
        // line in the `-i` pretty view for a segment with no status flags
        // set, rather than omitting the line entirely.
        "STATUS" => Some(shm_status(e.perms)),
        "ATTACH" => format_time(time_format, now, e.atime),
        "DETACH" => format_time(time_format, now, e.dtime),
        "COMMAND" => pid_command_line(e.cpid),
        "CPID" => Some(e.cpid.to_string()),
        "LPID" => Some(e.lpid.to_string()),
        _ => None,
    }
}

fn sem_field(e: &SemEntry, col: &str, time_format: TimeFormat, now: i64) -> Option<String> {
    match col {
        "NSEMS" => Some(e.nsems.to_string()),
        "OTIME" => format_time(time_format, now, e.otime),
        _ => None,
    }
}

fn msg_field(e: &MsgEntry, col: &str, bytes: bool, time_format: TimeFormat, now: i64) -> Option<String> {
    match col {
        "USEDBYTES" => Some(size_desc(e.cbytes, bytes)),
        "MSGS" => Some(e.qnum.to_string()),
        "SEND" => format_time(time_format, now, e.stime),
        "RECV" => format_time(time_format, now, e.rtime),
        "LSPID" => Some(e.lspid.to_string()),
        "LRPID" => Some(e.lrpid.to_string()),
        _ => None,
    }
}

const SHM_DEST: u32 = 0o1000;
const SHM_LOCKED: u32 = 0o2000;

fn shm_status(perms: u32) -> String {
    let mut parts = Vec::new();
    if perms & SHM_DEST != 0 {
        parts.push("dest");
    }
    if perms & SHM_LOCKED != 0 {
        parts.push("locked");
    }
    if perms & (libc::SHM_HUGETLB as u32) != 0 {
        parts.push("hugetlb");
    }
    if perms & (libc::SHM_NORESERVE as u32) != 0 {
        parts.push("noreserve");
    }
    parts.join(",")
}

/// Reads `/proc/<pid>/cmdline`, joining NUL-separated args with spaces
/// (matches real `lsipc`'s "creator command" column).
fn pid_command_line(pid: i32) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    let content = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let trimmed = content.strip_suffix(&[0]).unwrap_or(&content);
    if trimmed.is_empty() {
        return None;
    }
    let joined: Vec<u8> = trimmed.iter().map(|&b| if b == 0 { b' ' } else { b }).collect();
    Some(String::from_utf8_lossy(&joined).into_owned())
}

fn describe_permissions(perms: u32, numeric: bool) -> String {
    if numeric {
        format!("{:04o}", perms & 0o7777)
    } else {
        ascii_mode(perms)
    }
}

/// Renders permission bits the same way `ls -l`/real `lsipc` do: a 9-char
/// `rwxrwxrwx`-style string reflecting owner/group/other read/write/execute
/// plus setuid/setgid/sticky, e.g. `rw-r--r--`.
fn ascii_mode(perms: u32) -> String {
    let mut s = String::with_capacity(9);
    let triplets = [
        (0o400, 0o200, 0o100, 0o4000, 's', 'S'),
        (0o040, 0o020, 0o010, 0o2000, 's', 'S'),
        (0o004, 0o002, 0o001, 0o1000, 't', 'T'),
    ];
    for (r, w, x, set_bit, set_x, set_no_x) in triplets {
        s.push(if perms & r != 0 { 'r' } else { '-' });
        s.push(if perms & w != 0 { 'w' } else { '-' });
        s.push(match (perms & set_bit != 0, perms & x != 0) {
            (false, false) => '-',
            (false, true) => 'x',
            (true, false) => set_no_x,
            (true, true) => set_x,
        });
    }
    s
}

fn size_desc(size: u64, bytes: bool) -> String {
    if bytes {
        size.to_string()
    } else {
        human_size(size)
    }
}

/// Real util-linux 2^n scaling + one-decimal rounding, up to `E` (exabyte).
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

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Formats a `time_t`-like Unix timestamp using the real C library's
/// `localtime_r`, matching real `lsipc(1)`'s three `--time-format` modes.
/// Returns `None` for a zero timestamp (util-linux's convention for
/// "never", e.g. a shared memory segment that's never been detached).
fn format_time(format: TimeFormat, now: i64, time: i64) -> Option<String> {
    if time == 0 {
        return None;
    }
    let tm = local_time(time)?;
    let now_tm = local_time(now)?;

    Some(match format {
        TimeFormat::Short => {
            if tm.tm_yday == now_tm.tm_yday && tm.tm_year == now_tm.tm_year {
                format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
            } else if tm.tm_year == now_tm.tm_year {
                format!("{}{:02}", MONTHS[tm.tm_mon as usize], tm.tm_mday)
            } else {
                format!(
                    "{}-{}{:02}",
                    tm.tm_year + 1900,
                    MONTHS[tm.tm_mon as usize],
                    tm.tm_mday
                )
            }
        }
        TimeFormat::Full => format!(
            "{} {} {:2} {:02}:{:02}:{:02} {}",
            WEEKDAYS[tm.tm_wday as usize],
            MONTHS[tm.tm_mon as usize],
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec,
            tm.tm_year + 1900
        ),
        TimeFormat::Iso => {
            let tz_minutes = if tm.tm_isdst < 0 { 0 } else { tm.tm_gmtoff / 60 };
            let tz_hours = tz_minutes / 60;
            let tz_minutes = (tz_minutes % 60).abs();
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{:+03}:{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min,
                tm.tm_sec,
                tz_hours,
                tz_minutes
            )
        }
    })
}

fn local_time(time: i64) -> Option<libc::tm> {
    // SAFETY: `localtime_r` writes into `tm` (zero-initialized first) and
    // returns either a pointer to that same `tm` or null on error; we only
    // read `tm` after checking the return value is non-null.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let time = time as libc::time_t;
        if libc::localtime_r(&time, &mut tm).is_null() {
            None
        } else {
            Some(tm)
        }
    }
}

pub(crate) fn now_unix() -> i64 {
    // SAFETY: `gettimeofday` with a non-null `tv` pointer and null `tz`
    // (the only supported form on Linux) always succeeds.
    unsafe {
        let mut tv: libc::timeval = std::mem::zeroed();
        libc::gettimeofday(&mut tv, std::ptr::null_mut());
        tv.tv_sec as i64
    }
}

/// One row of already-resolved `(column, value)` cells, in column order.
pub(crate) type Row = Vec<(&'static str, Option<String>)>;

pub(crate) fn render_rows(mode: OutputMode, columns: &[&'static str], noheadings: bool, rows: &[Row], shell: bool) {
    match mode {
        OutputMode::Table => render_table(columns, noheadings, rows, shell),
        OutputMode::Export => render_export(rows, ' ', shell),
        OutputMode::NewLine => render_export(rows, '\n', shell),
        OutputMode::Raw => render_raw(columns, noheadings, rows),
        OutputMode::Json => render_json(rows),
        OutputMode::Pretty => unreachable!("pretty view is rendered separately"),
    }
}

fn column_header(col: &str, shell: bool) -> String {
    if shell {
        col.replace('%', "").replace(':', "_").to_uppercase()
    } else {
        col.to_string()
    }
}

/// Matches the real column flags exactly (`COLUMN_INFOS` in the reference's
/// `column.rs`): most numeric/permission/time columns are right-aligned;
/// `KEY`/`ID` and the free-text name/description columns are left-aligned.
fn column_right_aligned(col: &str) -> bool {
    matches!(
        col,
        "OWNER"
            | "PERMS"
            | "CUID"
            | "CGID"
            | "UID"
            | "GID"
            | "CTIME"
            | "USEDBYTES"
            | "SEND"
            | "RECV"
            | "LSPID"
            | "LRPID"
            | "SIZE"
            | "NATTCH"
            | "ATTACH"
            | "DETACH"
            | "CPID"
            | "LPID"
            | "NSEMS"
            | "OTIME"
            | "USED"
            | "USE%"
            | "LIMIT"
    )
}

fn column_width(col: &str) -> usize {
    match col {
        "KEY" => 10,
        "ID" => 6,
        "OWNER" | "CUSER" | "USER" | "GROUP" | "CGROUP" => 7,
        "PERMS" => 9,
        "CUID" | "CGID" | "UID" | "GID" => 4,
        "CTIME" | "ATTACH" | "DETACH" | "SEND" | "RECV" | "OTIME" => 8,
        "SIZE" | "USEDBYTES" => 6,
        "NATTCH" | "MSGS" | "NSEMS" => 6,
        "STATUS" => 6,
        "COMMAND" => 15,
        "CPID" | "LPID" | "LSPID" | "LRPID" => 7,
        "RESOURCE" => 8,
        "DESCRIPTION" => 40,
        "USED" | "LIMIT" => 8,
        "USE%" => 6,
        _ => 6,
    }
}

fn pad(s: &str, width: usize, right_align: bool) -> String {
    if right_align {
        format!("{s:>width$}")
    } else {
        format!("{s:<width$}")
    }
}

fn render_table(columns: &[&'static str], noheadings: bool, rows: &[Row], shell: bool) {
    if !noheadings {
        let header: Vec<String> = columns
            .iter()
            .map(|c| pad(&column_header(c, shell), column_width(c), column_right_aligned(c)))
            .collect();
        println!("{}", header.join(" ").trim_end());
    }
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|(col, val)| pad(val.as_deref().unwrap_or(""), column_width(col), column_right_aligned(col)))
            .collect();
        println!("{}", cells.join(" ").trim_end());
    }
}

fn render_export(rows: &[Row], separator: char, shell: bool) {
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|(col, val)| format!("{}=\"{}\"", column_header(col, shell), val.as_deref().unwrap_or("")))
            .collect();
        let joiner = separator.to_string();
        println!("{}", cells.join(&joiner));
    }
}

fn render_raw(columns: &[&'static str], noheadings: bool, rows: &[Row]) {
    if !noheadings {
        println!("{}", columns.join(" "));
    }
    for row in rows {
        let cells: Vec<String> = row.iter().map(|(_, val)| val.clone().unwrap_or_default()).collect();
        println!("{}", cells.join(" "));
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

/// `table_name` matches real `lsipc -J`'s per-kind JSON top-level key.
pub(crate) fn render_json_named(table_name: &str, columns: &[&'static str], rows: &[Row]) {
    let _ = columns;
    let mut out = String::new();
    out.push_str("{\n   ");
    out.push_str(&json_string(table_name));
    out.push_str(": [\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str(if i == 0 { "      {\n" } else { ",{\n" });
        for (j, (col, val)) in row.iter().enumerate() {
            out.push_str("         ");
            out.push_str(&json_string(&col.to_lowercase()));
            out.push_str(": ");
            out.push_str(&val.as_deref().map(json_string).unwrap_or_else(|| "null".to_string()));
            if j + 1 < row.len() {
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

fn render_json(rows: &[Row]) {
    render_json_named("", &[], rows);
}

/// Renders the `-i`/`--id` single-resource "pretty" view: one
/// `Title:  value` line per selected column, followed by an `Elements:`
/// sub-table for semaphore sets.
pub(crate) fn render_pretty(row: &Row, elements: Option<&[SemElement]>) {
    for (col, val) in row {
        let Some(val) = val else { continue };
        let title = column_title(col);
        let label = format!("{title}:");
        println!("{label:<36}{val:<36}");
    }

    if let Some(elements) = elements {
        if !elements.is_empty() {
            println!("Elements:");
            println!();
            println!(
                "{:>6} {:>5} {:>6} {:>6} {:>3} COMMAND",
                "SEMNUM", "VALUE", "NCOUNT", "ZCOUNT", "PID"
            );
            for e in elements {
                let cmd = pid_command_line(e.pid).unwrap_or_default();
                println!(
                    "{:>6} {:>5} {:>6} {:>6} {:>3} {cmd}",
                    e.semnum, e.val, e.ncount, e.zcount, e.pid
                );
            }
        }
    }
}
