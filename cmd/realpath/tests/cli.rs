//! End-to-end CLI tests for `realpath`, exercised by spawning the built binary.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "user_realpath_cli_test_{}_{}_{}",
        std::process::id(),
        tag,
        n
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn run(args: &[&str]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_realpath"))
        .args(args)
        .output()
        .expect("spawn realpath");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn golden_path_resolves_symlink() {
    let dir = scratch_dir("golden");
    let target = dir.join("target.txt");
    fs::write(&target, b"hi").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let (stdout, stderr, code) = run(&[link.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(stdout.trim_end(), target.canonicalize().unwrap().to_str().unwrap());
}

#[test]
fn nonexistent_path_falls_back_to_absolute_form() {
    let dir = scratch_dir("missing");
    let missing = dir.join("nope.txt");

    let (stdout, _, code) = run(&[missing.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), missing.to_str().unwrap());
}

#[test]
fn missing_operand_errors() {
    let (_, stderr, code) = run(&[]);
    assert_eq!(code, 1);
    assert!(stderr.contains("missing operand"));
}

#[test]
fn invalid_option_errors() {
    let (_, stderr, code) = run(&["--bogus"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("invalid option"));
}

#[test]
fn zero_flag_uses_nul_separator() {
    let dir = scratch_dir("zero");
    let (stdout, _, code) = run(&["-z", dir.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(stdout.ends_with('\0'));
    assert!(!stdout.contains('\n'));
}

#[test]
fn relative_to_missing_argument_errors() {
    let (_, stderr, code) = run(&["--relative-to"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("option requires an argument"));
}
