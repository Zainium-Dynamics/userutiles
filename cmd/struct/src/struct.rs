//! Core implementation for the `struct` binary.
//!
//! Design priorities:
//! - zero external dependencies;
//! - manual argument parsing;
//! - one target per process invocation;
//! - no overwrite semantics under normal operation or race conditions;
//! - Linux path fidelity through `OsStrExt::as_bytes`.

use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const EXIT_OK: i32 = 0;
const EXIT_ERR: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    force_file: bool,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    kind: TargetKind,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    Create(Request),
}

#[derive(Debug)]
struct StructError {
    kind: ErrorKind,
    message: String,
    path: Option<PathBuf>,
    source: Option<io::Error>,
    overwrite_blocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    Usage,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// Run the `struct` CLI over `args` (which must include `argv[0]` first,
/// matching `std::env::args_os()`), printing results/errors to
/// stdout/stderr and returning the process exit code.
pub fn run<I>(args: I) -> i32
where
    I: IntoIterator<Item = OsString>,
{
    match parse_args(args) {
        Ok(Command::Help) => {
            print_help();
            EXIT_OK
        }
        Ok(Command::Create(request)) => match execute(request) {
            Ok(report) => {
                print_success(&report);
                EXIT_OK
            }
            Err(error) => {
                print_error(&error);
                EXIT_ERR
            }
        },
        Err(error) => {
            print_error(&error);
            eprintln!();
            eprintln!("Run `struct --help` for usage.");
            EXIT_ERR
        }
    }
}

fn parse_args<I>(args: I) -> Result<Command, StructError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter().skip(1);
    let mut force_file = false;
    let mut path = None;

    while let Some(arg) = args.next() {
        let bytes = arg.as_bytes();

        match bytes {
            b"-h" | b"--help" => return Ok(Command::Help),
            b"-t" => {
                force_file = true;
            }
            b"--" => {
                // Everything after `--` is positional, allowing targets such as
                // `-name` without interpreting them as options.
                let next = args
                    .next()
                    .ok_or_else(|| usage_error("missing path after `--`"))?;
                set_single_path(&mut path, next)?;
                reject_extra_operands(args)?;
                break;
            }
            _ if bytes.starts_with(b"-") => {
                return Err(usage_error(format!(
                    "unknown option `{}`",
                    display_os(&arg)
                )));
            }
            _ => {
                set_single_path(&mut path, arg)?;
            }
        }
    }

    match path {
        Some(path) => Ok(Command::Create(Request { force_file, path })),
        None if force_file => Err(usage_error("missing path after `-t`")),
        None => Ok(Command::Help),
    }
}

fn set_single_path(slot: &mut Option<PathBuf>, value: OsString) -> Result<(), StructError> {
    if slot.is_some() {
        return Err(usage_error(
            "multiple targets are not supported; invoke `struct` once per target",
        ));
    }

    *slot = Some(PathBuf::from(value));
    Ok(())
}

fn reject_extra_operands<I>(args: I) -> Result<(), StructError>
where
    I: IntoIterator<Item = OsString>,
{
    if args.into_iter().next().is_some() {
        return Err(usage_error(
            "multiple targets are not supported; invoke `struct` once per target",
        ));
    }

    Ok(())
}

/// Validate and carry out a single create [`Request`]: reject empty paths,
/// refuse to overwrite an already-existing terminal target, classify the
/// target as a file or directory, and create it (plus any missing
/// ancestors).
fn execute(request: Request) -> Result<CreateReport, StructError> {
    validate_non_empty_path(&request.path)?;

    // Safety preflight: inspect the terminal target before creating parents or
    // files. `symlink_metadata` is used deliberately because a symlink itself is
    // an existing filesystem object and must be protected from replacement.
    ensure_target_absent(&request.path)?;

    let target = Target {
        kind: classify_target(&request.path, request.force_file),
        path: request.path,
    };

    // Classification happens after the existence preflight so existing root
    // paths such as `/` abort as "already exists" instead of producing a lower
    // quality validation error.
    validate_target_shape(&target)?;

    match target.kind {
        TargetKind::File => create_file(&target.path),
        TargetKind::Directory => create_directory(&target.path),
    }
}

fn validate_non_empty_path(path: &Path) -> Result<(), StructError> {
    if path.as_os_str().is_empty() {
        return Err(usage_error("empty path operands are not valid"));
    }

    Ok(())
}

fn ensure_target_absent(path: &Path) -> Result<(), StructError> {
    match existing_kind(path) {
        Ok(Some(kind)) => Err(StructError {
            kind: ErrorKind::Runtime,
            message: format!("{} already exists", kind.label()),
            path: Some(path.to_path_buf()),
            source: None,
            overwrite_blocked: true,
        }),
        Ok(None) => Ok(()),
        Err(source) => Err(StructError {
            kind: ErrorKind::Runtime,
            message: "cannot inspect target path".to_string(),
            path: Some(path.to_path_buf()),
            source: Some(source),
            overwrite_blocked: false,
        }),
    }
}

fn existing_kind(path: &Path) -> io::Result<Option<ExistingKind>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            let file_type = metadata.file_type();

            let kind = if file_type.is_file() {
                ExistingKind::File
            } else if file_type.is_dir() {
                ExistingKind::Directory
            } else if file_type.is_symlink() {
                ExistingKind::Symlink
            } else {
                ExistingKind::Other
            };

            Ok(Some(kind))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn classify_target(path: &Path, force_file: bool) -> TargetKind {
    if force_file || path_contains_dot(path) {
        TargetKind::File
    } else {
        TargetKind::Directory
    }
}

fn path_contains_dot(path: &Path) -> bool {
    path.as_os_str().as_bytes().contains(&b'.')
}

fn validate_target_shape(target: &Target) -> Result<(), StructError> {
    if target.kind == TargetKind::File {
        validate_file_target(&target.path)?;
    }

    Ok(())
}

fn validate_file_target(path: &Path) -> Result<(), StructError> {
    // Unix cannot create a regular file through a path ending in `/`.
    // Rejecting it before parent creation prevents confusing partial work.
    if path.as_os_str().as_bytes().ends_with(b"/") {
        return Err(StructError {
            kind: ErrorKind::Runtime,
            message: "file targets must not end with `/`".to_string(),
            path: Some(path.to_path_buf()),
            source: None,
            overwrite_blocked: false,
        });
    }

    let Some(file_name) = path.file_name() else {
        return Err(StructError {
            kind: ErrorKind::Runtime,
            message: "file target has no terminal filename".to_string(),
            path: Some(path.to_path_buf()),
            source: None,
            overwrite_blocked: false,
        });
    };

    // `.` and `..` are path navigation components, not creatable leaf files.
    if matches!(file_name.as_bytes(), b"." | b"..") {
        return Err(StructError {
            kind: ErrorKind::Runtime,
            message: "file target cannot use `.` or `..` as the filename".to_string(),
            path: Some(path.to_path_buf()),
            source: None,
            overwrite_blocked: false,
        });
    }

    Ok(())
}

fn create_file(path: &Path) -> Result<CreateReport, StructError> {
    let parent = meaningful_parent(path).map(Path::to_path_buf);

    if let Some(parent) = parent.as_deref() {
        // Parent creation is allowed; the protected terminal target was already
        // checked above, and `create_new(true)` below closes the remaining race.
        fs::create_dir_all(parent).map_err(|source| StructError {
            kind: ErrorKind::Runtime,
            message: "failed to create parent directory tree".to_string(),
            path: Some(parent.to_path_buf()),
            source: Some(source),
            overwrite_blocked: false,
        })?;
    }

    // `create_new(true)` maps to atomic "create only if absent" behavior. This
    // is the final no-overwrite guard in case another process creates the same
    // path between our preflight check and the open call.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| {
            let already_exists = source.kind() == io::ErrorKind::AlreadyExists;

            StructError {
                kind: ErrorKind::Runtime,
                message: if already_exists {
                    "file already exists".to_string()
                } else {
                    "failed to create file".to_string()
                },
                path: Some(path.to_path_buf()),
                source: Some(source),
                overwrite_blocked: already_exists,
            }
        })?;

    Ok(CreateReport {
        kind: TargetKind::File,
        path: path.to_path_buf(),
        parent,
    })
}

fn create_directory(path: &Path) -> Result<CreateReport, StructError> {
    let parent = meaningful_parent(path).map(Path::to_path_buf);

    if let Some(parent) = parent.as_deref() {
        // Build only the ancestors with `create_dir_all`, then create the final
        // target using `create_dir` so an already-existing terminal directory is
        // reported as an error instead of being accepted like `mkdir -p`.
        fs::create_dir_all(parent).map_err(|source| StructError {
            kind: ErrorKind::Runtime,
            message: "failed to create parent directory tree".to_string(),
            path: Some(parent.to_path_buf()),
            source: Some(source),
            overwrite_blocked: false,
        })?;
    }

    fs::create_dir(path).map_err(|source| {
        let already_exists = source.kind() == io::ErrorKind::AlreadyExists;

        StructError {
            kind: ErrorKind::Runtime,
            message: if already_exists {
                "directory already exists".to_string()
            } else {
                "failed to create directory".to_string()
            },
            path: Some(path.to_path_buf()),
            source: Some(source),
            overwrite_blocked: already_exists,
        }
    })?;

    Ok(CreateReport {
        kind: TargetKind::Directory,
        path: path.to_path_buf(),
        parent,
    })
}

fn meaningful_parent(path: &Path) -> Option<&Path> {
    // `Path::parent("file")` is `Some("")`; that empty parent is not a real
    // directory to create. Root (`/`) is intentionally retained for absolute
    // paths such as `/var/lib/zainium/state.db`.
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CreateReport {
    kind: TargetKind,
    path: PathBuf,
    parent: Option<PathBuf>,
}

impl ExistingKind {
    fn label(self) -> &'static str {
        match self {
            ExistingKind::File => "file",
            ExistingKind::Directory => "directory",
            ExistingKind::Symlink => "symlink",
            ExistingKind::Other => "path",
        }
    }
}

fn usage_error(message: impl Into<String>) -> StructError {
    StructError {
        kind: ErrorKind::Usage,
        message: message.into(),
        path: None,
        source: None,
        overwrite_blocked: false,
    }
}

fn print_help() {
    println!("struct - Zainium OS smart filesystem creator");
    println!();
    println!("Replaces common `mkdir -p` and `touch` create workflows with one safe tool.");
    println!(
        "(Traditional timestamp updates: use `touch`. Directory trees/listings: `blueprint`.)"
    );
    println!();
    println!("USAGE:");
    println!(" struct <PATH>");
    println!(" struct -t <PATH>");
    println!();
    println!("RULES:");
    println!(" 1. If <PATH> already exists, abort immediately (no overwrite).");
    println!(" 2. If <PATH> contains '.', create it as a file (touch-style create).");
    println!(" 3. If <PATH> does not contain '.', create it as a directory tree (mkdir -p).");
    println!(" 4. Use -t to force an extensionless file target.");
    println!();
    println!("EXAMPLES:");
    println!(" struct src/core/engine.rs # parents + file (like touch after mkdir -p)");
    println!(" struct src/services/auth # directory tree (like mkdir -p)");
    println!(" struct -t bin/trigger # extensionless file");
    println!();
    println!("RELATED:");
    println!(" touch update timestamps / empty create (GNU-style)");
    println!(" blueprint project/tree layouts (not the `tree` command)");
    println!();
    println!("EXIT STATUS:");
    println!(" 0 success");
    println!(" 1 invalid input or filesystem error");
}

fn print_success(report: &CreateReport) {
    match report.kind {
        TargetKind::File => {
            if let Some(parent) = &report.parent {
                println!("struct: parent tree ready: {}", display_path(parent));
            }
            println!("struct: file created: {}", display_path(&report.path));
        }
        TargetKind::Directory => {
            println!("struct: directory created: {}", display_path(&report.path));
        }
    }
}

fn print_error(error: &StructError) {
    eprintln!("struct: error: {}", error.message);

    if let Some(path) = &error.path {
        eprintln!("path: {}", display_path(path));
    }

    if let Some(source) = &error.source {
        eprintln!("cause: {}", source);
    }

    if error.kind == ErrorKind::Runtime && error.overwrite_blocked {
        eprintln!("aborted: existing filesystem objects are never overwritten");
    }
}

fn display_os(value: &OsStr) -> String {
    value.to_string_lossy().into_owned()
}

fn display_path(path: &Path) -> String {
    path.as_os_str().to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(value: &str) -> OsString {
        OsString::from(value)
    }

    #[test]
    fn no_arguments_prints_help() {
        assert_eq!(parse_args([arg("struct")]).unwrap(), Command::Help);
    }

    #[test]
    fn help_flags_print_help() {
        assert_eq!(
            parse_args([arg("struct"), arg("-h")]).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args([arg("struct"), arg("--help")]).unwrap(),
            Command::Help
        );
    }

    #[test]
    fn parses_force_file_request() {
        assert_eq!(
            parse_args([arg("struct"), arg("-t"), arg("bin/trigger")]).unwrap(),
            Command::Create(Request {
                force_file: true,
                path: PathBuf::from("bin/trigger"),
            })
        );
    }

    #[test]
    fn rejects_multiple_targets() {
        assert!(parse_args([arg("struct"), arg("one"), arg("two")]).is_err());
    }

    #[test]
    fn detects_dots_anywhere_in_the_path_as_file_targets() {
        assert_eq!(
            classify_target(Path::new("release.v1/config"), false),
            TargetKind::File
        );
    }

    #[test]
    fn treats_extensionless_paths_as_directories_by_default() {
        assert_eq!(
            classify_target(Path::new("var/lib/zainium/cache"), false),
            TargetKind::Directory
        );
    }

    #[test]
    fn force_file_overrides_extensionless_directory_detection() {
        assert_eq!(
            classify_target(Path::new("run/zainium-lock"), true),
            TargetKind::File
        );
    }

    #[test]
    fn rejects_trailing_slash_file_targets() {
        let target = Target {
            kind: TargetKind::File,
            path: PathBuf::from("tmp/file.txt/"),
        };

        assert!(validate_target_shape(&target).is_err());
    }

    #[test]
    fn force_file_without_path_is_a_usage_error() {
        assert!(parse_args([arg("struct"), arg("-t")]).is_err());
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("user_struct_test_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn end_to_end_creates_nested_file_and_parents() {
        let root = tmp_dir("nested_file");
        let target = root.join("a/b/c/file.txt");
        let code = run([arg("struct"), OsString::from(target.as_os_str())]);
        assert_eq!(code, EXIT_OK);
        assert!(target.is_file());
        assert!(root.join("a/b/c").is_dir());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn end_to_end_creates_directory_tree() {
        let root = tmp_dir("nested_dir");
        let target = root.join("x/y/z");
        let code = run([arg("struct"), OsString::from(target.as_os_str())]);
        assert_eq!(code, EXIT_OK);
        assert!(target.is_dir());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn end_to_end_refuses_to_overwrite_existing_target() {
        let root = tmp_dir("existing");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("already-here");
        fs::create_dir(&target).unwrap();
        let code = run([arg("struct"), OsString::from(target.as_os_str())]);
        assert_eq!(code, EXIT_ERR);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn end_to_end_force_file_creates_extensionless_file() {
        let root = tmp_dir("force_file");
        let target = root.join("trigger");
        let code = run([arg("struct"), arg("-t"), OsString::from(target.as_os_str())]);
        assert_eq!(code, EXIT_OK);
        assert!(target.is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_empty_path_operand() {
        assert!(validate_non_empty_path(Path::new("")).is_err());
    }

    #[test]
    fn meaningful_parent_skips_empty_relative_parent() {
        assert_eq!(meaningful_parent(Path::new("file.txt")), None);
        assert_eq!(
            meaningful_parent(Path::new("dir/file.txt")),
            Some(Path::new("dir"))
        );
    }
}
