// run.rs — cp CLI entry for the unified `zutils` binary.
//
// Decision tree per source:
// source is a symlink and not dereferencing => recreate the symlink itself
// source is a directory (after following, if dereferencing) =>
//     -R/-r/-a required, else CpError::IsDirectory
//     => ops::tree::copy_tree (parallel, atomic per-file)
// otherwise (regular file) => copy_single_file (atomic temp-file + rename)
//
// Dereference default follows GNU cp: symlinks are followed for a plain
// (non-recursive) copy unless -P/-d/-a says otherwise; symlinks are NOT
// followed during a recursive copy unless -L overrides it.

use std::{fs, path::Path};

use clap::Parser;
use crossbeam_channel::unbounded;

use crate::{
    cli::Opts,
    error::{io_err, CpError, Result},
    metadata,
    ops::{
        file::{self, FileCopyOpts},
        tree::{self, TreeCopyOpts},
    },
    progress, ui, verify,
};
use usercore::protect;

pub fn run(args: Vec<String>) -> i32 {
    #[cfg(not(any(target_os = "linux", target_os = "redox")))]
    ui::fatal("cp only supports Linux and Redox OS.");

    let opts = Opts::parse_from(args);

    let (sources, dest_s) = opts.sources_and_dest();
    if sources.is_empty() {
        ui::fatal("At least one source path is required.");
    }
    if dest_s.is_empty() {
        ui::fatal("Destination path is required.");
    }
    let dest = Path::new(&dest_s);

    if opts.no_target_directory && sources.len() > 1 {
        ui::fatal("extra operand — -T disallows more than one source.");
    }

    // -a implies -R --preserve=all --no-dereference; -d implies
    // --no-dereference (copy symlinks as symlinks, no full metadata copy).
    let recursive = opts.recursive || opts.archive;
    let preserve = opts.preserve || opts.archive;
    let dereference = resolve_dereference(&opts, recursive);

    if sources.len() > 1 && !dest.is_dir() {
        ui::fatal(&format!("target '{}' is not a directory", dest.display()));
    }

    let file_opts = FileCopyOpts {
        reflink: opts.reflink,
        sparse: opts.sparse,
    };

    let mut status = 0;
    for source_str in &sources {
        let src = Path::new(source_str);

        let src_meta = match fs::symlink_metadata(src) {
            Ok(m) => m,
            Err(e) => {
                ui::err(&format!("cannot stat '{}': {e}", src.display()));
                status = 1;
                continue;
            }
        };

        let effective_dest: std::path::PathBuf = if dest.is_dir() && !opts.no_target_directory {
            dest.join(src.file_name().unwrap_or_default())
        } else {
            dest.to_owned()
        };

        if let Some(reason) = protect::modification_denied(&effective_dest) {
            ui::err(&format!(
                "cannot overwrite '{}': {}",
                effective_dest.display(),
                reason.message()
            ));
            status = 1;
            continue;
        }

        if opts.verbose || sources.len() > 1 {
            ui::kv("Source", &src.display().to_string());
            ui::kv("Dest", &effective_dest.display().to_string());
        }

        let result = dispatch_one(
            src,
            &src_meta,
            &effective_dest,
            &opts,
            recursive,
            preserve,
            dereference,
            file_opts,
        );

        match result {
            Ok(()) => {
                if opts.verbose {
                    ui::ok(&format!(
                        "'{}' -> '{}'",
                        src.display(),
                        effective_dest.display()
                    ));
                }
            }
            Err(CpError::NoClobber(_)) => {
                if opts.verbose {
                    ui::warn(&format!(
                        "not overwriting '{}' (--no-clobber)",
                        effective_dest.display()
                    ));
                }
            }
            Err(e) => {
                ui::err(&e.to_string());
                status = 1;
            }
        }
    }

    status
}

#[allow(clippy::too_many_arguments)]
fn dispatch_one(
    src: &Path,
    src_meta: &fs::Metadata,
    dest: &Path,
    opts: &Opts,
    recursive: bool,
    preserve: bool,
    dereference: bool,
    file_opts: FileCopyOpts,
) -> Result<()> {
    let is_symlink = src_meta.file_type().is_symlink();

    if is_symlink && !dereference {
        return copy_symlink_single(src, dest, preserve);
    }

    let is_dir = if is_symlink {
        // Already decided to dereference above — follow it to see what it
        // points at. A dangling symlink here surfaces as a normal I/O error.
        fs::metadata(src).map_err(|e| io_err(src, e))?.is_dir()
    } else {
        src_meta.is_dir()
    };

    if is_dir {
        if !recursive {
            return Err(CpError::IsDirectory(src.to_owned()));
        }
        return copy_dir(src, dest, opts, preserve, dereference, file_opts);
    }

    copy_single_file(src, dest, opts, preserve, file_opts)
}

fn resolve_dereference(opts: &Opts, recursive: bool) -> bool {
    if opts.dereference {
        true
    } else if opts.no_dereference || opts.links || opts.archive {
        false
    } else {
        // GNU default: follow symlinks for a plain copy, don't follow them
        // when copying recursively.
        !recursive
    }
}

fn copy_symlink_single(src: &Path, dest: &Path, preserve: bool) -> Result<()> {
    if fs::symlink_metadata(dest).is_ok() {
        fs::remove_file(dest).map_err(|e| io_err(dest, e))?;
    }
    let target = fs::read_link(src).map_err(|e| io_err(src, e))?;
    std::os::unix::fs::symlink(&target, dest).map_err(|e| io_err(dest, e))?;
    if preserve {
        metadata::preserve(src, dest)?;
    }
    Ok(())
}

fn copy_single_file(
    src: &Path,
    dest: &Path,
    opts: &Opts,
    preserve: bool,
    file_opts: FileCopyOpts,
) -> Result<()> {
    if opts.no_clobber && dest.exists() {
        return Err(CpError::NoClobber(dest.to_owned()));
    }
    if opts.interactive && dest.exists() && !prompt_overwrite(dest) {
        return Ok(());
    }
    if opts.update && dest.exists() && tree::dest_is_newer_or_equal(src, dest) {
        return Ok(());
    }

    let (tx, rx) = unbounded();
    let drain = std::thread::spawn(move || for _ in rx {});
    let result = file::copy_file_atomic(src, dest, file_opts, &tx);
    drop(tx);
    let _ = drain.join();
    result?;

    if preserve {
        metadata::preserve(src, dest)?;
    }
    if opts.verify {
        verify::verify(src, dest)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_dir(
    src: &Path,
    dest: &Path,
    opts: &Opts,
    preserve: bool,
    dereference: bool,
    file_opts: FileCopyOpts,
) -> Result<()> {
    let tree_opts = TreeCopyOpts {
        preserve,
        dereference,
        no_clobber: opts.no_clobber,
        update: opts.update,
        one_file_system: opts.one_file_system,
        verify: opts.verify,
        jobs: opts.jobs,
        file: file_opts,
    };

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

    let result = tree::copy_tree(src, dest, tree_opts, tx);
    let _ = progress_handle.join();
    result
}

fn prompt_overwrite(path: &Path) -> bool {
    use std::io::{self, Write};
    eprint!("cp: overwrite '{}'? ", path.display());
    let _ = io::stderr().flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.chars().next(), Some('y') | Some('Y'))
}
