//! user link — create a hard link.
use std::fs;

use usercore::Ui;

/// Entry point for the `link` utility. Parses `std::env::args()` (exactly
/// two operands, FILE1 and FILE2) and calls `link(2)` (via
/// [`fs::hard_link`]) to create FILE2 as a hard link to the existing
/// FILE1. Unlike a check-then-create sequence, `link(2)` is a single
/// atomic syscall, so there is no TOCTOU window between checking FILE2's
/// existence and creating it.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("link");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        if args.is_empty() {
            ui.err("missing operand");
            return 1;
        }
        print!("Usage: link FILE1 FILE2\nCall the link function to create a link named FILE2 to an existing FILE1.\n");
        return 0;
    }
    if args[0] == "--version" {
        println!("link (user_utils) 0.1.0");
        return 0;
    }
    if args.len() < 2 {
        ui.err(&format!("missing operand after '{}'", args[0]));
        return 1;
    }
    if args.len() > 2 {
        ui.err(&format!("extra operand '{}'", args[2]));
        return 1;
    }
    match fs::hard_link(&args[0], &args[1]) {
        Ok(()) => 0,
        Err(e) => {
            ui.err(&format!(
                "cannot create link '{}' to '{}': {e}",
                args[1], args[0]
            ));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("user_link_test_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hard_link_creates_second_name_for_same_inode() {
        let dir = scratch_dir("basic");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        fs::write(&a, b"payload").unwrap();
        fs::hard_link(&a, &b).unwrap();
        assert_eq!(fs::read(&b).unwrap(), b"payload");
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&a).unwrap().ino(),
            fs::metadata(&b).unwrap().ino()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hard_link_missing_source_errors() {
        let dir = scratch_dir("missing");
        let a = dir.join("nope.txt");
        let b = dir.join("b.txt");
        assert!(fs::hard_link(&a, &b).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hard_link_existing_target_errors() {
        let dir = scratch_dir("exists");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        fs::write(&a, b"x").unwrap();
        fs::write(&b, b"already here").unwrap();
        // link(2) must not silently overwrite an existing FILE2.
        assert!(fs::hard_link(&a, &b).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
