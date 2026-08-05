// cli.rs — Command-line interface for mv (formerly xmv).

use clap::Parser;

/// mv — Next-generation move: atomic swaps, undo journal, trash-safe
#[derive(Parser, Debug, Clone)]
#[command(
 name = "mv",
 version,
 about = "Next-generation mv: atomic, undoable, trash-aware",
 long_about = None,
)]
pub struct Opts {
    /// SOURCE... DEST (or A B for --exchange)
    #[arg(required = false, num_args = 0..)]
    pub paths: Vec<String>,

    /// Move directories recursively (cross-device)
    #[arg(short = 'R', long = "recursive")]
    pub recursive: bool,

    /// Do not overwrite existing destination
    #[arg(short = 'n', long = "no-clobber")]
    pub no_clobber: bool,

    /// Atomically exchange two paths using renameat2(RENAME_EXCHANGE).
    /// Both paths must exist and be on the same filesystem.
    /// Usage: mv --exchange pathA pathB
    #[arg(long = "exchange")]
    pub exchange: bool,

    /// Make the rename non-destructive: fail if dest already exists
    /// Uses renameat2(RENAME_NOREPLACE) — atomic, no TOCTOU race.
    #[arg(long = "no-replace")]
    pub no_replace: bool,

    /// Verify XXH3-128 checksum after cross-device copy before deleting source
    #[arg(long = "verify", default_value_t = true)]
    pub verify: bool,

    /// Write a transaction journal to this path for --undo support.
    /// Default: $XDG_STATE_HOME/mv/journal.toml (TOML only — never JSON)
    #[arg(long = "journal")]
    pub journal: Option<String>,

    /// Undo the last recorded transaction from the journal
    #[arg(long = "undo")]
    pub undo: bool,

    /// Move destination to XDG trash instead of overwriting it (--trash-safe)
    #[arg(long = "trash-safe")]
    pub trash_safe: bool,

    /// Number of parallel threads for cross-device copy (default: CPU count)
    #[arg(short = 'j', long = "jobs", default_value_t = num_cpus())]
    pub jobs: usize,

    /// Show progress bar during cross-device moves
    #[arg(long = "progress", default_value_t = true)]
    pub progress: bool,

    /// Verbose output — print each operation as it happens
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Preserve all metadata during cross-device moves (permissions, timestamps, xattrs)
    #[arg(short = 'a', long = "archive")]
    pub archive: bool,
}

impl Opts {
    /// Split trailing positionals into sources + dest.
    pub fn sources_and_dest(&self) -> (Vec<String>, String) {
        if self.paths.is_empty() {
            return (Vec::new(), String::new());
        }
        if self.paths.len() == 1 {
            return (self.paths.clone(), String::new());
        }
        let dest = self.paths.last().cloned().unwrap_or_default();
        let sources = self.paths[..self.paths.len() - 1].to_vec();
        (sources, dest)
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}
