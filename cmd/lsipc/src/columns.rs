//! Column name tables and default/explicit column-list resolution,
//! mirroring the real `lsipc(1)` / uutils reference column model.

pub(crate) const GENERIC: [&str; 13] = [
    "KEY", "ID", "OWNER", "PERMS", "CUID", "CUSER", "CGID", "CGROUP", "UID", "USER", "GID",
    "GROUP", "CTIME",
];
pub(crate) const SHM_ALL: [&str; 8] = [
    "SIZE", "NATTCH", "STATUS", "ATTACH", "DETACH", "COMMAND", "CPID", "LPID",
];
pub(crate) const MSG_ALL: [&str; 6] = ["USEDBYTES", "MSGS", "SEND", "RECV", "LSPID", "LRPID"];
pub(crate) const SEM_ALL: [&str; 2] = ["NSEMS", "OTIME"];
pub(crate) const SUMMARY_ALL: [&str; 5] = ["RESOURCE", "DESCRIPTION", "USED", "USE%", "LIMIT"];

const ALL_COLUMN_NAMES: [&str; 34] = [
    "KEY",
    "ID",
    "OWNER",
    "PERMS",
    "CUID",
    "CUSER",
    "CGID",
    "CGROUP",
    "UID",
    "USER",
    "GID",
    "GROUP",
    "CTIME",
    "USEDBYTES",
    "MSGS",
    "SEND",
    "RECV",
    "LSPID",
    "LRPID",
    "SIZE",
    "NATTCH",
    "STATUS",
    "ATTACH",
    "DETACH",
    "COMMAND",
    "CPID",
    "LPID",
    "NSEMS",
    "OTIME",
    "RESOURCE",
    "DESCRIPTION",
    "USED",
    "USE%",
    "LIMIT",
];

/// `(name, title)` pairs, for `-H`-equivalent help text and the `-i` pretty
/// view's left-hand labels.
pub(crate) const COLUMN_TITLES: [(&str, &str); 34] = [
    ("KEY", "Key"),
    ("ID", "ID"),
    ("OWNER", "Owner"),
    ("PERMS", "Permissions"),
    ("CUID", "Creator UID"),
    ("CUSER", "Creator user"),
    ("CGID", "Creator GID"),
    ("CGROUP", "Creator group"),
    ("UID", "UID"),
    ("USER", "User name"),
    ("GID", "GID"),
    ("GROUP", "Group name"),
    ("CTIME", "Last change"),
    ("USEDBYTES", "Bytes used"),
    ("MSGS", "Messages"),
    ("SEND", "Msg sent"),
    ("RECV", "Msg received"),
    ("LSPID", "Msg sender"),
    ("LRPID", "Msg receiver"),
    ("SIZE", "Segment size"),
    ("NATTCH", "Attached processes"),
    ("STATUS", "Status"),
    ("ATTACH", "Attach time"),
    ("DETACH", "Detach time"),
    ("COMMAND", "Creator command"),
    ("CPID", "Creator PID"),
    ("LPID", "Last user PID"),
    ("NSEMS", "Semaphores"),
    ("OTIME", "Last operation"),
    ("RESOURCE", "Resource"),
    ("DESCRIPTION", "Description"),
    ("USED", "Used"),
    ("USE%", "Use"),
    ("LIMIT", "Limit"),
];

mod default {
    pub(crate) const QUEUES: [&str; 8] = [
        "KEY",
        "ID",
        "PERMS",
        "OWNER",
        "USEDBYTES",
        "MSGS",
        "LSPID",
        "LRPID",
    ];
    pub(crate) const SHM: [&str; 11] = [
        "KEY", "ID", "PERMS", "OWNER", "SIZE", "NATTCH", "STATUS", "CTIME", "CPID", "LPID",
        "COMMAND",
    ];
    pub(crate) const SEM: [&str; 5] = ["KEY", "ID", "PERMS", "OWNER", "NSEMS"];
    pub(crate) const GLOBAL: [&str; 5] = ["RESOURCE", "DESCRIPTION", "LIMIT", "USED", "USE%"];
    pub(crate) const CREATOR: [&str; 4] = ["CUID", "CGID", "UID", "GID"];
}

pub(crate) fn column_applies_to(
    name: &str,
    queues: bool,
    shmems: bool,
    semaphores: bool,
    global: bool,
) -> bool {
    if GENERIC.contains(&name) {
        return true;
    }
    (queues && MSG_ALL.contains(&name))
        || (shmems && SHM_ALL.contains(&name))
        || (semaphores && SEM_ALL.contains(&name))
        || (global && SUMMARY_ALL.contains(&name))
}

/// Flags relevant to column selection, mirroring the CLI options that
/// influence the default column set.
pub(crate) struct ColumnFlags {
    pub(crate) queues: bool,
    pub(crate) shmems: bool,
    pub(crate) semaphores: bool,
    pub(crate) global: bool,
    pub(crate) creator: bool,
    pub(crate) time: bool,
}

/// `all_defaults`: used for the pretty (`-i`) single-resource view, which
/// always has exactly one kind flag set (`-i` requires one) and ignores
/// `-c`/`-t` (every generic + kind-specific column is shown regardless).
pub(crate) fn all_defaults(flags: &ColumnFlags) -> Vec<&'static str> {
    let mut columns: Vec<&'static str> = GENERIC.to_vec();
    if flags.queues {
        columns.extend(MSG_ALL);
    }
    if flags.shmems {
        columns.extend(SHM_ALL);
    }
    if flags.semaphores {
        columns.extend(SEM_ALL);
    }
    columns
}

/// `filter_defaults`: the normal (non-`-i`) default column set, built up
/// additively from whichever of `-q`/`-m`/`-s`/`-g`/`-c`/`-t` were passed —
/// note this only adds a kind's default columns when that kind's flag was
/// *explicitly* given (not when it's just one of the kinds being
/// iterated as part of a "no kind flag given" fallback).
pub(crate) fn filter_defaults(flags: &ColumnFlags) -> Vec<&'static str> {
    let mut columns: Vec<&'static str> = Vec::new();

    if flags.queues {
        columns.extend(default::QUEUES);
    }
    if flags.shmems {
        columns.extend(default::SHM);
    }
    if flags.semaphores {
        columns.extend(default::SEM);
    }
    if flags.global {
        columns.extend(default::GLOBAL);
    }
    if flags.creator {
        columns.extend(default::CREATOR);
    }
    if flags.time {
        if flags.queues || (!flags.shmems && !flags.semaphores) {
            columns.extend(["SEND", "RECV", "CTIME"]);
        }
        if flags.shmems || (!flags.queues && !flags.semaphores) {
            // If COMMAND was the last column, keep it last.
            let reappend_command = matches!(columns.last(), Some(&"COMMAND"));
            if reappend_command {
                columns.pop();
            }
            columns.extend(["ATTACH", "DETACH"]);
            if reappend_command {
                columns.push("COMMAND");
            }
        }
        if flags.semaphores || (!flags.queues && !flags.shmems) {
            columns.extend(["OTIME", "CTIME"]);
        }
    }

    columns
}

/// Resolves the effective output column list from `-o`/`--output`, matching
/// real `lsipc(1)` semantics: `-o LIST` (no leading `+`) replaces the
/// default set entirely; `-o +LIST` appends `LIST` to the default set
/// (`all_defaults` for the pretty view, `filter_defaults` otherwise).
/// Column names not applicable to the currently selected IPC kind(s)
/// produce a non-fatal warning (returned as the second tuple element),
/// matching the reference's behavior of warning rather than erroring.
pub(crate) fn resolve_columns(
    output: Option<&str>,
    flags: &ColumnFlags,
    pretty: bool,
) -> Result<(Vec<&'static str>, Option<String>), String> {
    let base = || -> Vec<&'static str> {
        if pretty && !flags.creator && !flags.time {
            all_defaults(flags)
        } else {
            filter_defaults(flags)
        }
    };

    let Some(spec) = output else {
        return Ok((base(), None));
    };

    let (append, list_str) = match spec.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, spec),
    };

    let mut list: Vec<&'static str> = Vec::new();
    for name in list_str.split(',') {
        let Some(&canonical) = ALL_COLUMN_NAMES.iter().find(|&&c| c == name) else {
            return Err(format!("unknown column: {name}"));
        };
        list.push(canonical);
    }
    if list.is_empty() {
        return Err(format!("unknown column: {spec}"));
    }

    let not_applicable: Vec<&str> = list
        .iter()
        .copied()
        .filter(|&name| {
            !column_applies_to(
                name,
                flags.queues,
                flags.shmems,
                flags.semaphores,
                flags.global,
            )
        })
        .collect();
    let warning = (!not_applicable.is_empty()).then(|| {
        format!(
            "The following columns do not apply to the specified IPC: {}.",
            not_applicable.join(",")
        )
    });

    if append {
        let mut columns = base();
        columns.extend(list);
        Ok((columns, warning))
    } else {
        Ok((list, warning))
    }
}

pub(crate) fn column_title(name: &'static str) -> &'static str {
    COLUMN_TITLES
        .iter()
        .find(|&&(id, _)| id == name)
        .map(|&(_, title)| title)
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(
        queues: bool,
        shmems: bool,
        semaphores: bool,
        global: bool,
        creator: bool,
        time: bool,
    ) -> ColumnFlags {
        ColumnFlags {
            queues,
            shmems,
            semaphores,
            global,
            creator,
            time,
        }
    }

    #[test]
    fn filter_defaults_shm_only() {
        let f = flags(false, true, false, false, false, false);
        assert_eq!(filter_defaults(&f), default::SHM.to_vec());
    }

    #[test]
    fn filter_defaults_global_only() {
        let f = flags(false, false, false, true, false, false);
        assert_eq!(filter_defaults(&f), default::GLOBAL.to_vec());
    }

    #[test]
    fn filter_defaults_shm_time_reappends_command_last() {
        let f = flags(false, true, false, false, false, true);
        let cols = filter_defaults(&f);
        // default::SHM already ends with COMMAND; -t should insert
        // ATTACH/DETACH before it and put COMMAND back at the end.
        assert_eq!(cols.last(), Some(&"COMMAND"));
        let attach_pos = cols.iter().position(|&c| c == "ATTACH").unwrap();
        let command_pos = cols.iter().position(|&c| c == "COMMAND").unwrap();
        assert!(attach_pos < command_pos);
    }

    #[test]
    fn filter_defaults_sem_time_adds_otime_ctime() {
        let f = flags(false, false, true, false, false, true);
        let cols = filter_defaults(&f);
        assert!(cols.ends_with(&["OTIME", "CTIME"]));
    }

    #[test]
    fn all_defaults_shm_includes_generic_and_shm_specific() {
        let f = flags(false, true, false, false, false, false);
        let cols = all_defaults(&f);
        assert_eq!(cols.len(), GENERIC.len() + SHM_ALL.len());
        assert!(cols.contains(&"KEY"));
        assert!(cols.contains(&"COMMAND"));
    }

    #[test]
    fn resolve_columns_explicit_replaces_default() {
        let f = flags(false, true, false, false, false, false);
        let (cols, warn) = resolve_columns(Some("KEY,ID"), &f, false).unwrap();
        assert_eq!(cols, vec!["KEY", "ID"]);
        assert!(warn.is_none());
    }

    #[test]
    fn resolve_columns_plus_prefix_appends() {
        let f = flags(false, true, false, false, false, false);
        let (cols, warn) = resolve_columns(Some("+CTIME"), &f, false).unwrap();
        let mut expected = default::SHM.to_vec();
        expected.push("CTIME");
        assert_eq!(cols, expected);
        assert!(warn.is_none());
    }

    #[test]
    fn resolve_columns_rejects_unknown_name() {
        let f = flags(false, true, false, false, false, false);
        assert!(resolve_columns(Some("BOGUS"), &f, false).is_err());
    }

    #[test]
    fn resolve_columns_warns_on_inapplicable_column() {
        // NSEMS is a semaphore-only column; selecting it while listing
        // shared memory should warn, not error.
        let f = flags(false, true, false, false, false, false);
        let (cols, warn) = resolve_columns(Some("KEY,NSEMS"), &f, false).unwrap();
        assert_eq!(cols, vec!["KEY", "NSEMS"]);
        assert!(warn.unwrap().contains("NSEMS"));
    }
}
