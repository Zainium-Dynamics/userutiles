// run.rs — xmv CLI entry for the unified `zutils` binary.
//
// Decision tree mirrors the former binary `main.rs`:
//
// --undo → journal.undo_last()
// --exchange A B → ops::atomic::atomic_exchange()
// --no-replace src dest → ops::atomic::atomic_no_replace()
// same device, standard → ops::rename::rename_*()
// cross-device → ops::crossdev::move_cross_device()
use std::path::Path;
use usercore::protect;

use bytesize::ByteSize;
use clap::Parser;
use crossbeam_channel::unbounded;

use crate::{
    cli::Opts,
    error::XmvError,
    ops::{atomic, crossdev, rename},
    progress, trash, ui,
    undo::{Journal, Operation},
};

pub fn run(args: Vec<String>) {
    // ── Platform guard ────────────────────────────────────────────────────────
    #[cfg(not(any(target_os = "linux", target_os = "redox")))]
    ui::fatal("mv only supports Linux and Redox OS.");

    // ── Ctrl-C handler ────────────────────────────────────────────────────────
    ctrlc::set_handler(|| {
 eprintln!();
 ui::warn("Operation interrupted. The journal records any partial state — run mv --undo to reverse.");
 std::process::exit(130);
 })
 .expect("Failed to install Ctrl-C handler");

    let opts = Opts::parse_from(args);

    // ── Open / resolve journal ────────────────────────────────────────────────
    let journal_path = opts
        .journal
        .as_deref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(Journal::default_path);

    let mut journal = Journal::open(journal_path.clone()).unwrap_or_else(|e| {
        ui::fatal(&format!("Cannot open journal: {e}"));
    });

    // ── --undo: reverse last committed operation ──────────────────────────────
    if opts.undo {
        ui::heading("mv — Undo");
        ui::kv("Journal", &journal_path.display().to_string());
        println!();
        match journal.undo_last() {
            Ok(()) => ui::ok("Last operation reversed."),
            Err(e) => ui::fatal(&e.to_string()),
        }
        return;
    }

    // ── Validate argument count ───────────────────────────────────────────────
    let (sources, dest_s) = opts.sources_and_dest();
    if sources.is_empty() {
        ui::fatal("At least one source path is required.");
    }
    if dest_s.is_empty() {
        ui::fatal("Destination path is required.");
    }
    let dest = Path::new(&dest_s);

    // ── --exchange: atomic swap of two paths ──────────────────────────────────
    if opts.exchange {
        if sources.len() != 1 {
            ui::fatal("--exchange requires exactly two arguments: mv --exchange pathA pathB");
        }
        let path_a = Path::new(&sources[0]);
        let path_b = dest;

        ui::heading("mv — Atomic Exchange");
        ui::kv("Path A", &path_a.display().to_string());
        ui::kv("Path B", &path_b.display().to_string());
        println!();

        match atomic::atomic_exchange(path_a, path_b, &mut journal) {
            Ok(()) => {
                commit_last(&mut journal);
                ui::ok("Paths exchanged atomically.");
            }
            Err(XmvError::Renameat2Unsupported) => {
                ui::warn("renameat2 unavailable — using non-atomic fallback (kernel < 3.15).");
                ui::ok("Paths exchanged (non-atomic fallback).");
            }
            Err(e) => ui::fatal(&e.to_string()),
        }
        return;
    }

    // ── Multi-source moves: all sources must go into a dest directory ─────────
    if sources.len() > 1 && !dest.is_dir() {
        ui::fatal("When moving multiple sources, the destination must be an existing directory.");
    }

    // ── Process each source ───────────────────────────────────────────────────
    ui::heading("mv — Move");

    let total_sources = sources.len();
    for source_str in sources.iter() {
        let src = Path::new(source_str);

        if !src.exists() {
            ui::fatal(&format!("Source not found: '{}'", src.display()));
        }

        if src.is_dir() && !opts.recursive {
            ui::fatal(&format!(
                "'{}' is a directory — use -R / --recursive.",
                src.display()
            ));
        }

        // Determine effective destination path.
        let effective_dest: std::path::PathBuf = if dest.is_dir() {
            dest.join(src.file_name().expect("source has no filename"))
        } else {
            dest.to_owned()
        };

        if let Some(reason) = protect::removal_denied(src) {
            ui::fatal(&format!("Cannot move '{}': {}", src.display(), reason.message()));
        }
        if let Some(reason) = protect::modification_denied(&effective_dest) {
            ui::fatal(&format!("Cannot overwrite '{}': {}", effective_dest.display(), reason.message()));
        }

        if opts.verbose || total_sources > 1 {
            ui::kv("Source", &src.display().to_string());
            ui::kv("Dest", &effective_dest.display().to_string());
        }

        // ── --trash-safe: send existing dest to Trash before overwriting ──────
        if opts.trash_safe && effective_dest.exists() {
            match trash::move_to_trash(&effective_dest) {
                Ok(trash_path) => {
                    ui::info(&format!(
                        "Existing '{}' moved to Trash: {}",
                        effective_dest.display(),
                        trash_path.display()
                    ));
                }
                Err(e) => ui::fatal(&format!("--trash-safe failed: {e}")),
            }
        } else if opts.no_clobber && effective_dest.exists() {
            ui::warn(&format!(
                "Skipping '{}' — destination exists (--no-clobber).",
                src.display()
            ));
            continue;
        }

        // ── Route: same-device or cross-device ───────────────────────────────
        let same_dev = rename::same_device(src, effective_dest.parent().unwrap_or(dest));

        if same_dev {
            move_same_device(src, &effective_dest, &opts, &mut journal);
        } else {
            move_cross_device(src, &effective_dest, &opts, &mut journal);
        }
    }
}

fn move_same_device(src: &Path, dest: &Path, opts: &Opts, journal: &mut Journal) {
    if let Err(e) = journal.record(Operation::Move {
        src: src.to_owned(),
        dest: dest.to_owned(),
    }) {
        ui::warn(&format!("Journal write failed (continuing): {e}"));
    }

    let result = if opts.no_replace {
        rename::rename_no_replace(src, dest)
    } else {
        rename::rename_overwrite(src, dest)
    };

    match result {
        Ok(()) => {
            commit_last(journal);
            if opts.verbose {
                ui::ok(&format!(
                    "'{}' → '{}' (atomic rename)",
                    src.display(),
                    dest.display()
                ));
            }
        }
        Err(XmvError::Renameat2Unsupported) => {
            if let Err(e) = rename::rename_overwrite(src, dest) {
                ui::fatal(&e.to_string());
            }
            commit_last(journal);
            if opts.verbose {
                ui::ok(&format!(
                    "'{}' → '{}' (rename fallback)",
                    src.display(),
                    dest.display()
                ));
            }
        }
        Err(XmvError::NoClobber(_)) => {
            ui::warn(&format!(
                "Destination '{}' already exists (--no-replace).",
                dest.display()
            ));
        }
        Err(e) => ui::fatal(&e.to_string()),
    }
}

fn move_cross_device(src: &Path, dest: &Path, opts: &Opts, journal: &mut Journal) {
    let src_size: u64 = dir_size(src);

    if opts.verbose {
        ui::info(&format!(
            "Cross-device move detected — copying {} ...",
            ByteSize(src_size)
        ));
        ui::kv("Verify", if opts.verify { "XXH3-128" } else { "off" });
        ui::kv("Archive", if opts.archive { "on" } else { "off" });
        ui::kv("Jobs", &opts.jobs.to_string());
        println!();
    }

    if let Err(e) = journal.record(Operation::Move {
        src: src.to_owned(),
        dest: dest.to_owned(),
    }) {
        ui::warn(&format!("Journal write failed (continuing): {e}"));
    }

    let (tx, rx) = unbounded();

    let show = opts.progress;
    let progress_handle = std::thread::spawn(move || {
        if show {
            progress::render_progress(rx)
        } else {
            for _ in rx {}
            (0, 0)
        }
    });

    match crossdev::move_cross_device(src, dest, opts.jobs, opts.verify, opts.archive, tx) {
        Ok(()) => {
            let (files, bytes) = progress_handle.join().unwrap_or((0, 0));
            commit_last(journal);
            ui::ok(&format!(
                "'{}' → '{}' ({} file(s), {})",
                src.display(),
                dest.display(),
                files,
                ByteSize(bytes),
            ));
        }
        Err(e) => {
            let _ = progress_handle.join();
            ui::fatal(&format!(
                "Cross-device move failed: {e}\nSource has NOT been deleted."
            ));
        }
    }
}

fn commit_last(journal: &mut Journal) {
    let _ = journal.commit_last();
}

fn dir_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}
