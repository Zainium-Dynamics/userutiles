// progress.rs — Progress rendering for cross-device moves.
//
// Receives ProgressEvents over a crossbeam channel and drives an indicatif
// MultiProgress display following the Zainuim cyber-tech aesthetic:
//
// Bright Cyan — heading / tool name
// Bright Magenta — values (filenames, sizes)
// Yellow — throughput, ETA
// Bright Green — success tick (✓)
// Bright Red — error indicator (only in ui.rs)

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use bytesize::ByteSize;
use crossbeam_channel::Receiver;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Events sent from copier threads to the progress renderer.
pub enum ProgressEvent {
    /// A file has started being copied (opens a per-file bar).
    FileStart { path: PathBuf, total: u64 },
    /// Some bytes were transferred for an in-flight file.
    Progress { path: PathBuf, bytes: u64 },
    /// A file finished (closes its per-file bar).
    FileDone { path: PathBuf, bytes: u64 },
}

/// Blocking render loop — intended to run on a dedicated thread.
/// Returns `(files_moved, total_bytes)` when the channel is closed.
pub fn render_progress(rx: Receiver<ProgressEvent>) -> (u64, u64) {
    let mp = MultiProgress::new();

    // ── Aggregate bar (bottom of the stack) ───────────────────────────────────
    let overall = mp.add(ProgressBar::new(0));
    overall.set_style(
        ProgressStyle::with_template(
            "\x1b[96m mv\x1b[0m \x1b[35m{bytes}\x1b[0m/\x1b[35m{total_bytes}\x1b[0m \
 \x1b[33m{bytes_per_sec}\x1b[0m \
 [{bar:42.cyan/blue}] \x1b[33m{eta}\x1b[0m",
        )
        .unwrap()
        .progress_chars("━╸─"),
    );
    overall.enable_steady_tick(Duration::from_millis(100));

    let mut file_bars: HashMap<PathBuf, ProgressBar> = HashMap::new();
    let mut files_done: u64 = 0;
    let mut bytes_done: u64 = 0;

    for event in rx {
        match event {
            ProgressEvent::FileStart { path, total } => {
                overall.inc_length(total);
                let pb = mp.insert_before(&overall, ProgressBar::new(total));
                pb.set_style(per_file_style());
                pb.enable_steady_tick(Duration::from_millis(80));
                let name = display_name(&path);
                pb.set_message(format!("\x1b[92m→\x1b[0m \x1b[35m{name}\x1b[0m"));
                file_bars.insert(path, pb);
            }

            ProgressEvent::Progress { path, bytes } => {
                if let Some(pb) = file_bars.get(&path) {
                    pb.inc(bytes);
                }
                overall.inc(bytes);
            }

            ProgressEvent::FileDone { path, bytes } => {
                if let Some(pb) = file_bars.remove(&path) {
                    let name = display_name(&path);
                    pb.finish_with_message(format!(
                        "\x1b[92m✓\x1b[0m \x1b[35m{name}\x1b[0m \x1b[33m{}\x1b[0m",
                        ByteSize(bytes)
                    ));
                } else {
                    // Skipped file (already complete) — keep overall bar in sync.
                    overall.inc_length(bytes);
                    overall.inc(bytes);
                }
                files_done += 1;
                bytes_done += bytes;
            }
        }
    }

    overall.finish_with_message(format!(
        "\x1b[92m✓ Done\x1b[0m \x1b[35m{files_done} file(s) moved\x1b[0m \
 \x1b[33m{}\x1b[0m",
        ByteSize(bytes_done)
    ));

    (files_done, bytes_done)
}

fn per_file_style() -> ProgressStyle {
    ProgressStyle::with_template(" {msg:50} [{bar:32.cyan/blue}] \x1b[33m{bytes_per_sec}\x1b[0m")
        .unwrap()
        .progress_chars("━╸─")
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
