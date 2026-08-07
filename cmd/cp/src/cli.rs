// cli.rs — Command-line interface for cp.

use clap::{Parser, ValueEnum};

/// Controls whether `cp` attempts a copy-on-write reflink (Linux `FICLONE`)
/// before falling back to a data copy.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[value(rename_all = "lower")]
pub enum ReflinkMode {
    /// Try FICLONE first; silently fall back to a normal copy if unsupported.
    #[default]
    Auto,
    /// Require FICLONE to succeed; error out if the filesystem can't reflink.
    Always,
    /// Never attempt FICLONE.
    Never,
}

/// Controls whether `cp` preserves holes in sparse source files.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[value(rename_all = "lower")]
pub enum SparseMode {
    /// Use a hole-aware copy only when the source file is detected as sparse.
    #[default]
    Auto,
    /// Always copy via `SEEK_HOLE`/`SEEK_DATA`, even for dense files.
    Always,
    /// Never skip holes — every byte (including zero-filled holes) is written.
    Never,
}

/// cp — copy files and directories: reflink/sparse-aware, atomic overwrite.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "cp",
    version,
    about = "Copy files and directories",
    long_about = None,
)]
pub struct Opts {
    /// SOURCE... DEST (or SOURCE... with -t DIRECTORY)
    #[arg(required = false, num_args = 0..)]
    pub paths: Vec<String>,

    /// Copy directories recursively
    #[arg(short = 'R', long = "recursive", visible_short_alias = 'r')]
    pub recursive: bool,

    /// If an existing destination file cannot be replaced, remove it and try again
    #[arg(short = 'f', long = "force")]
    pub force: bool,

    /// Prompt before overwriting an existing destination file
    #[arg(short = 'i', long = "interactive")]
    pub interactive: bool,

    /// Do not overwrite an existing file
    #[arg(short = 'n', long = "no-clobber")]
    pub no_clobber: bool,

    /// Explain what is being done
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Preserve mode, timestamps, extended attributes, and ownership
    #[arg(short = 'p', long = "preserve")]
    pub preserve: bool,

    /// Same as -dR --preserve=all: archive a whole tree
    #[arg(short = 'a', long = "archive")]
    pub archive: bool,

    /// Never follow symbolic links in SOURCE
    #[arg(short = 'P', long = "no-dereference")]
    pub no_dereference: bool,

    /// Always follow symbolic links in SOURCE
    #[arg(short = 'L', long = "dereference")]
    pub dereference: bool,

    /// Same as --no-dereference --preserve=links (copy symlinks as symlinks)
    #[arg(short = 'd')]
    pub links: bool,

    /// Copy only when SOURCE is newer than the destination, or destination is missing
    #[arg(short = 'u', long = "update")]
    pub update: bool,

    /// Copy all SOURCE arguments into DIRECTORY
    #[arg(short = 't', long = "target-directory", value_name = "DIRECTORY")]
    pub target_directory: Option<String>,

    /// Treat DEST as a normal file, not a directory (rejects >1 source)
    #[arg(
        short = 'T',
        long = "no-target-directory",
        conflicts_with = "target_directory"
    )]
    pub no_target_directory: bool,

    /// Stay on the source filesystem — do not descend into other mount points
    #[arg(long = "one-file-system")]
    pub one_file_system: bool,

    /// Control copy-on-write reflinking (Linux FICLONE)
    #[arg(
        long = "reflink",
        value_enum,
        num_args = 0..=1,
        default_missing_value = "always",
        default_value = "auto"
    )]
    pub reflink: ReflinkMode,

    /// Control hole-preserving sparse-file copying
    #[arg(
        long = "sparse",
        value_enum,
        num_args = 0..=1,
        default_missing_value = "always",
        default_value = "auto"
    )]
    pub sparse: SparseMode,

    /// Verify destination contents against the source with XXH3-128 after copying
    #[arg(long = "verify")]
    pub verify: bool,

    /// Show a progress bar (recommended for large or recursive copies)
    #[arg(long = "progress")]
    pub progress: bool,

    /// Number of parallel threads for recursive directory copies (default: CPU count)
    #[arg(short = 'j', long = "jobs", default_value_t = num_cpus())]
    pub jobs: usize,
}

impl Opts {
    /// Split trailing positionals into sources + dest, honoring `-t/--target-directory`.
    pub fn sources_and_dest(&self) -> (Vec<String>, String) {
        if let Some(dir) = &self.target_directory {
            return (self.paths.clone(), dir.clone());
        }
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
