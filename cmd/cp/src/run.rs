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
    let preserve = opts.preserve || opts.preserve_list.is_some() || opts.archive;
    let dereference = resolve_dereference(&opts, recursive);

    if sources.len() > 1 && !dest.is_dir() {
        ui::fatal(&format!("target '{}' is not a directory", dest.display()));
    }

    let backup = opts.backup.clone().or(opts.backup_short.then(|| "existing".to_string()));
    let backup = backup.filter(|c| c != "none" && c != "off");
    let suffix = opts.suffix.clone().or_else(|| std::env::var("SIMPLE_BACKUP_SUFFIX").ok()).unwrap_or_else(|| "~".into());

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

        let mut backed_up = None;
        if let Some(control) = backup.as_deref() {
            if fs::symlink_metadata(&effective_dest).is_ok_and(|m| !m.is_dir()) && !same_file(src, &effective_dest) {
                let b = usercore::pathx::backup_name(&effective_dest, control, &suffix);
                if let Err(e) = fs::rename(&effective_dest, &b) {
                    ui::err(&format!("cannot backup '{}': {e}", effective_dest.display()));
                    status = 1;
                    continue;
                }
                backed_up = Some(b);
            }
        }

        let result = if opts.link || opts.symbolic_link {
            make_link(src, &effective_dest, &opts)
        } else {
            dispatch_one(
            src,
            &src_meta,
            &effective_dest,
            &opts,
            recursive,
            preserve,
            dereference,
            file_opts,
        )
        };

        match result {
            Ok(()) => {
                if opts.verbose {
                    match &backed_up {
                        Some(b) => println!("'{}' -> '{}' (backup: '{}')", src.display(), effective_dest.display(), b.display()),
                        None => println!("'{}' -> '{}'", src.display(), effective_dest.display()),
                    }
                    let copied_dir = recursive && !opts.link && !opts.symbolic_link;
                    if copied_dir && (src_meta.is_dir() || (dereference && src.is_dir())) {
                        list_tree(src, &effective_dest, dereference);
                    }
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

/// -l / -s: a hard or symbolic link to SOURCE instead of a copy.
fn make_link(src: &Path, dest: &Path, opts: &Opts) -> Result<()> {
    if fs::symlink_metadata(dest).is_ok() {
        if opts.no_clobber {
            return Err(CpError::NoClobber(dest.to_owned()));
        }
        if !opts.force {
            return Err(io_err(dest, std::io::Error::from(std::io::ErrorKind::AlreadyExists)));
        }
        fs::remove_file(dest).map_err(|e| io_err(dest, e))?;
    }
    if opts.symbolic_link {
        std::os::unix::fs::symlink(src, dest).map_err(|e| io_err(dest, e))
    } else {
        fs::hard_link(src, dest).map_err(|e| io_err(dest, e))
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    matches!((fs::metadata(a), fs::metadata(b)), (Ok(x), Ok(y)) if x.dev() == y.dev() && x.ino() == y.ino())
}

/// `-v` on a directory copy: one line per copied entry, each directory before its contents.
fn list_tree(src: &Path, dest: &Path, dereference: bool) {
    let Ok(rd) = fs::read_dir(src) else { return };
    let mut names: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.file_name()).collect();
    names.sort();
    for n in names {
        let (s, d) = (src.join(&n), dest.join(&n));
        println!("'{}' -> '{}'", s.display(), d.display());
        let is_dir = if dereference { s.is_dir() } else { fs::symlink_metadata(&s).is_ok_and(|m| m.is_dir()) };
        if is_dir {
            list_tree(&s, &d, dereference);
        }
    }
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

    if same_file(src, dest) {
        return Err(CpError::SameFile(src.to_owned(), dest.to_owned()));
    }
    let existing = {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(dest).ok().map(|m| m.permissions().mode())
    };

    let (tx, rx) = unbounded();
    let drain = std::thread::spawn(move || for _ in rx {});
    let result = file::copy_file_atomic(src, dest, file_opts, &tx);
    drop(tx);
    let _ = drain.join();
    result?;

    if preserve {
        metadata::preserve(src, dest)?;
    } else {
        metadata::plain_mode(src, dest, existing)?;
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
