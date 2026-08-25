//! user unlink — remove a single file via unlink(2).
use std::fs;
use std::path::Path;

use usercore::{protect, Ui};

/// Entry point for the `unlink` utility. Parses `std::env::args()` and
/// removes exactly one named file by calling `unlink(2)` (via
/// [`fs::remove_file`]) directly — no directory recursion, no globbing.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("unlink");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    if args[0] == "--help" || args[0] == "-h" {
        print!("Usage: unlink FILE\nCall the unlink function to remove the specified FILE.\n");
        return 0;
    }
    if args[0] == "--version" {
        println!("unlink (user_utils) 0.1.0");
        return 0;
    }
    if args.len() > 1 {
        ui.err(&format!("extra operand '{}'", args[1]));
        return 1;
    }
    match unlink_file(Path::new(&args[0])) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!("cannot unlink '{}': {e}", args[0]));
            1
        }
    }
}

/// Remove a single file at `path` via `unlink(2)`. Thin wrapper around
/// [`fs::remove_file`] kept separate from [`run`] so it's testable without
/// going through `std::env::args()`.
fn unlink_file(path: &Path) -> std::io::Result<()> {
    if let Some(reason) = protect::removal_denied(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            reason.message(),
        ));
    }
    fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("user_unlink_test_{}_{name}", std::process::id()))
    }

    #[test]
    fn unlink_file_removes_existing_file() {
        let p = tmp_path("exists");
        File::create(&p).unwrap();
        assert!(p.exists());
        assert!(unlink_file(&p).is_ok());
        assert!(!p.exists());
    }

    #[test]
    fn unlink_file_errors_on_missing_file() {
        let p = tmp_path("missing");
        let _ = fs::remove_file(&p);
        let err = unlink_file(&p).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn unlink_file_errors_on_directory() {
        let p = tmp_path("dir");
        let _ = fs::remove_dir(&p);
        fs::create_dir(&p).unwrap();
        let result = unlink_file(&p);
        assert!(result.is_err());
        let _ = fs::remove_dir(&p);
    }
}
