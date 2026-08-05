//! user lsipc — show information on System V IPC facilities (shared memory,
//! semaphores, message queues) currently in use on the system, read from
//! `/proc/sysvipc/*` and `/proc/sys/kernel/*`.
mod columns;
mod model;
mod render;

use columns::{ColumnFlags, resolve_columns};
use render::{Entry, NameCache, OutputMode, Row, TimeFormat, cell_value, now_unix, render_pretty, render_rows};
use usercore::Ui;

const HELP: &str = "Usage: lsipc [options]\n\
Show information on IPC facilities currently employed in the system.\n\n\
  -b, --bytes           print SIZE in bytes rather than in human readable format\n\
  -c, --creator         show creator and owner\n\
  -e, --export          display in an export-able output format\n\
  -g, --global          info about system-wide usage\n\
  -i, --id <id>         print details on resource identified by id\n\
  -J, --json            use the JSON output format\n\
  -l, --list            force list output format\n\
  -m, --shmems          shared memory segments\n\
  -n, --newline         display each piece of information on a new line\n\
      --noheadings      don't print headings\n\
      --notruncate      don't truncate output\n\
  -o, --output <list>   define the columns to output\n\
  -P, --numeric-perms   print numeric permissions\n\
  -q, --queues          message queues\n\
  -r, --raw             display in raw mode\n\
  -s, --semaphores      semaphores\n\
  -t, --time            show attach, detach and change times\n\
      --time-format <type>  display dates in short, full or iso format\n\
  -y, --shell           use column names to be usable as shell variable identifiers\n\
  -h, --help            display this help and exit\n\
      --version         output version information and exit\n";

#[derive(Default)]
struct Options {
    bytes: bool,
    creator: bool,
    export: bool,
    global: bool,
    id: Option<i32>,
    json: bool,
    list: bool,
    newline: bool,
    noheadings: bool,
    notruncate: bool,
    numeric_perms: bool,
    output: Option<String>,
    queues: bool,
    raw: bool,
    semaphores: bool,
    shell: bool,
    shmems: bool,
    time: bool,
    time_format: Option<String>,
}

pub fn run() -> i32 {
    let ui = Ui::new("lsipc");
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut opts = Options::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("lsipc (user_utils) 0.1.0");
                return 0;
            }
            "-b" | "--bytes" => opts.bytes = true,
            "-c" | "--creator" => opts.creator = true,
            "-e" | "--export" => opts.export = true,
            "-g" | "--global" => opts.global = true,
            "-J" | "--json" => opts.json = true,
            "-l" | "--list" => opts.list = true,
            "-m" | "--shmems" => opts.shmems = true,
            "-n" | "--newline" => opts.newline = true,
            "--noheadings" => opts.noheadings = true,
            "--notruncate" => opts.notruncate = true,
            "-P" | "--numeric-perms" => opts.numeric_perms = true,
            "-q" | "--queues" => opts.queues = true,
            "-r" | "--raw" => opts.raw = true,
            "-s" | "--semaphores" => opts.semaphores = true,
            "-t" | "--time" => opts.time = true,
            "-y" | "--shell" => opts.shell = true,
            "-i" | "--id" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                match value.parse::<i32>() {
                    Ok(id) => opts.id = Some(id),
                    Err(_) => {
                        ui.err(&format!("invalid id argument: '{value}'"));
                        return 1;
                    }
                }
            }
            "-o" | "--output" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                opts.output = Some(value.clone());
            }
            "--time-format" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    ui.err(&format!("option '{}' requires an argument", args[i - 1]));
                    return 1;
                };
                opts.time_format = Some(value.clone());
            }
            s if s.starts_with('-') && !s.starts_with("--") && s.len() > 1 => {
                for c in s[1..].chars() {
                    match c {
                        'b' => opts.bytes = true,
                        'c' => opts.creator = true,
                        'e' => opts.export = true,
                        'g' => opts.global = true,
                        'J' => opts.json = true,
                        'l' => opts.list = true,
                        'm' => opts.shmems = true,
                        'n' => opts.newline = true,
                        'P' => opts.numeric_perms = true,
                        'q' => opts.queues = true,
                        'r' => opts.raw = true,
                        's' => opts.semaphores = true,
                        't' => opts.time = true,
                        'y' => opts.shell = true,
                        other => {
                            ui.err(&format!("invalid option -- '{other}'"));
                            return 1;
                        }
                    }
                }
            }
            other => {
                ui.err(&format!("unknown option -- '{other}'"));
                return 1;
            }
        }
        i += 1;
    }

    let kind_count = [opts.shmems, opts.queues, opts.semaphores]
        .iter()
        .filter(|&&b| b)
        .count();
    if kind_count > 1 {
        ui.err("only one of -m/--shmems, -q/--queues, -s/--semaphores may be given");
        return 1;
    }
    let out_count = [opts.export, opts.json, opts.list, opts.newline, opts.raw]
        .iter()
        .filter(|&&b| b)
        .count();
    if out_count > 1 {
        ui.err("only one of -e/-J/-l/-n/-r may be given");
        return 1;
    }
    if opts.global && opts.id.is_some() {
        ui.err("-g/--global and -i/--id are mutually exclusive");
        return 1;
    }
    if (opts.creator || opts.id.is_some() || opts.time) && kind_count == 0 {
        ui.err("-c/--creator, -i/--id, and -t/--time require -m, -q, or -s");
        return 1;
    }

    let time_format = match opts.time_format.as_deref() {
        None => {
            if opts.id.is_some() {
                TimeFormat::Full
            } else {
                TimeFormat::Short
            }
        }
        Some("short") => TimeFormat::Short,
        Some("full") => TimeFormat::Full,
        Some("iso") => TimeFormat::Iso,
        Some(other) => {
            ui.err(&format!("invalid time format: {other}"));
            return 1;
        }
    };

    let output_mode = if opts.export {
        OutputMode::Export
    } else if opts.json {
        OutputMode::Json
    } else if opts.newline {
        OutputMode::NewLine
    } else if opts.raw {
        OutputMode::Raw
    } else if opts.id.is_some() {
        // `-l/--list` and the plain default both render as the aligned
        // table (verified empirically against the real binary: `-l`
        // produces byte-identical output to no output-format flag at all).
        OutputMode::Pretty
    } else {
        OutputMode::Table
    };
    let _ = opts.list; // recognized, but doesn't change rendering (see above)

    let no_kind_given = kind_count == 0;
    // Real `lsipc(1)` defaults to the global summary when no IPC kind is
    // selected (verified against the system binary); the uutils reference
    // instead falls through to `describe()` for every kind with an empty
    // column set, which is a no-op/degenerate table. Matching the real
    // tool's behavior here since it's the far more useful default.
    let effective_global = opts.global || no_kind_given;

    let flags = ColumnFlags {
        queues: opts.queues,
        shmems: opts.shmems,
        semaphores: opts.semaphores,
        global: effective_global,
        creator: opts.creator,
        time: opts.time,
    };

    let (columns, warning) = match resolve_columns(
        opts.output.as_deref(),
        &flags,
        output_mode == OutputMode::Pretty,
    ) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }

    let clock = Clock {
        format: time_format,
        now: now_unix(),
    };
    let mut names = NameCache::default();

    if let Some(id) = opts.id {
        return describe_single(&ui, &columns, &opts, id, output_mode, &clock, &mut names);
    }

    if effective_global {
        let rows = match global_rows(&opts, no_kind_given, &columns) {
            Ok(v) => v,
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        };
        render_output(output_mode, "ipclimits", &columns, opts.noheadings, &rows, opts.shell);
        return 0;
    }

    // Exactly one kind flag is guaranteed set here (no_kind_given would
    // have taken the `effective_global` branch above otherwise).
    let table_name = if opts.queues {
        "messages"
    } else if opts.shmems {
        "sharedmemory"
    } else {
        "semaphores"
    };
    let rows = match describe_rows(&opts, None, &columns, &clock, &mut names) {
        Ok(v) => v,
        Err(e) => {
            ui.err(&e);
            return 1;
        }
    };
    render_output(output_mode, table_name, &columns, opts.noheadings, &rows, opts.shell);
    0
}

fn render_output(
    mode: OutputMode,
    table_name: &str,
    columns: &[&'static str],
    noheadings: bool,
    rows: &[Row],
    shell: bool,
) {
    if mode == OutputMode::Json {
        render::render_json_named(table_name, columns, rows);
    } else {
        render_rows(mode, columns, noheadings, rows, shell);
    }
}

/// Bundles the "what time is it, and in what format should times render"
/// context threaded through every cell-value/row-building call, purely to
/// keep those functions' argument counts down.
pub(crate) struct Clock {
    format: TimeFormat,
    now: i64,
}

fn describe_rows(
    opts: &Options,
    id_filter: Option<i32>,
    columns: &[&'static str],
    clock: &Clock,
    names: &mut NameCache,
) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    if opts.queues {
        for e in model::read_msg_table(id_filter)? {
            rows.push(build_row(&Entry::Msg(&e), columns, opts, clock, names));
        }
    } else if opts.shmems {
        for e in model::read_shm_table(id_filter)? {
            rows.push(build_row(&Entry::Shm(&e), columns, opts, clock, names));
        }
    } else if opts.semaphores {
        for e in model::read_sem_table(id_filter)? {
            rows.push(build_row(&Entry::Sem(&e), columns, opts, clock, names));
        }
    }
    Ok(rows)
}

fn build_row(entry: &Entry, columns: &[&'static str], opts: &Options, clock: &Clock, names: &mut NameCache) -> Row {
    columns
        .iter()
        .map(|&col| {
            let val = cell_value(entry, col, opts.bytes, opts.numeric_perms, clock.format, clock.now, names);
            (col, val)
        })
        .collect()
}

fn describe_single(
    ui: &Ui,
    columns: &[&'static str],
    opts: &Options,
    id: i32,
    output_mode: OutputMode,
    clock: &Clock,
    names: &mut NameCache,
) -> i32 {
    if opts.queues {
        match model::read_msg_table(Some(id)) {
            Ok(entries) if entries.len() == 1 => {
                let row = build_row(&Entry::Msg(&entries[0]), columns, opts, clock, names);
                finish_single(output_mode, "messages", columns, &row, None);
            }
            Ok(_) => eprintln!("lsipc: id {id} not found"),
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        }
    } else if opts.shmems {
        match model::read_shm_table(Some(id)) {
            Ok(entries) if entries.len() == 1 => {
                let row = build_row(&Entry::Shm(&entries[0]), columns, opts, clock, names);
                finish_single(output_mode, "sharedmemory", columns, &row, None);
            }
            Ok(_) => eprintln!("lsipc: id {id} not found"),
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        }
    } else {
        match model::read_sem_table(Some(id)) {
            Ok(entries) if entries.len() == 1 => {
                let row = build_row(&Entry::Sem(&entries[0]), columns, opts, clock, names);
                let elements = model::fetch_sem_elements(entries[0].id, entries[0].nsems);
                finish_single(output_mode, "semaphores", columns, &row, Some(elements));
            }
            Ok(_) => eprintln!("lsipc: id {id} not found"),
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        }
    }
    0
}

fn finish_single(
    mode: OutputMode,
    table_name: &str,
    columns: &[&'static str],
    row: &Row,
    elements: Option<Vec<model::SemElement>>,
) {
    if mode == OutputMode::Pretty {
        render_pretty(row, elements.as_deref());
    } else {
        render_output(mode, table_name, columns, false, std::slice::from_ref(row), false);
    }
}

fn global_rows(opts: &Options, no_kind_given: bool, columns: &[&'static str]) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let in_bytes = opts.bytes;

    if opts.queues || no_kind_given {
        let limits = model::read_msg_limits()?;
        let count = model::read_msg_table(None)?.len() as u64;
        rows.push(global_row(columns, "MSGMNI", "Number of System V message queues", Some(count), limits.mni, true));
        rows.push(global_row(columns, "MSGMAX", "Max size of System V message (bytes)", None, limits.max, in_bytes));
        rows.push(global_row(columns, "MSGMNB", "Default max size of System V queue (bytes)", None, limits.mnb, in_bytes));
    }
    if opts.shmems || no_kind_given {
        let limits = model::read_shm_limits()?;
        let entries = model::read_shm_table(None)?;
        let page_size = page_size()?;
        let pages = entries.iter().map(|e| e.size).sum::<u64>() / page_size;
        rows.push(global_row(columns, "SHMMNI", "Shared memory segments", Some(entries.len() as u64), limits.mni, true));
        rows.push(global_row(columns, "SHMALL", "Shared memory pages", Some(pages), limits.all, true));
        rows.push(global_row(columns, "SHMMAX", "Max size of shared memory segment (bytes)", None, limits.max, in_bytes));
        rows.push(global_row(columns, "SHMMIN", "Min size of shared memory segment (bytes)", None, limits.min, in_bytes));
    }
    if opts.semaphores || no_kind_given {
        let limits = model::read_sem_limits()?;
        let entries = model::read_sem_table(None)?;
        let total_nsems: u64 = entries.iter().map(|e| e.nsems).sum();
        rows.push(global_row(columns, "SEMMNI", "Number of semaphore identifiers", Some(entries.len() as u64), limits.mni, true));
        rows.push(global_row(columns, "SEMMNS", "Total number of semaphores", Some(total_nsems), limits.mns, true));
        rows.push(global_row(columns, "SEMMSL", "Max semaphores per semaphore set", None, limits.msl, true));
        rows.push(global_row(columns, "SEMOPM", "Max number of operations per semop(2)", None, limits.opm, true));
        rows.push(global_row(columns, "SEMVMX", "Semaphore max value", None, limits.vmx, true));
    }

    Ok(rows)
}

fn global_row(
    columns: &[&'static str],
    resource: &str,
    description: &str,
    used: Option<u64>,
    limit: u64,
    in_bytes: bool,
) -> Row {
    columns
        .iter()
        .map(|&col| {
            let val = match col {
                "RESOURCE" => Some(resource.to_string()),
                "DESCRIPTION" => Some(description.to_string()),
                "LIMIT" => Some(size_desc(limit, in_bytes)),
                "USED" => Some(used.map(|u| size_desc(u, in_bytes)).unwrap_or_else(|| "-".to_string())),
                "USE%" => Some(
                    used.map(|u| format!("{:.2}%", (u as f64) / (limit as f64) * 100.0))
                        .unwrap_or_else(|| "-".to_string()),
                ),
                _ => None,
            };
            (col, val)
        })
        .collect()
}

fn size_desc(n: u64, in_bytes: bool) -> String {
    if in_bytes {
        n.to_string()
    } else {
        human_size(n)
    }
}

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

fn page_size() -> Result<u64, String> {
    // SAFETY: `sysconf` with a valid, well-known name constant simply
    // returns a `long`, or -1 on error (checked below); no pointers involved.
    let value = unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) };
    if value <= 0 {
        Err("sysconf(_SC_PAGE_SIZE) failed".to_string())
    } else {
        Ok(value as u64)
    }
}
