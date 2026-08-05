//! End-to-end CLI tests for `readlink`, exercised by spawning the built binary.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "user_readlink_cli_test_{}_{}_{}",
        std::process::id(),
        tag,
        n
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn run(args: &[&str]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_readlink"))
        .args(args)
        .output()
        .expect("spawn readlink");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn golden_path_prints_symlink_target() {
    let dir = scratch_dir("golden");
    let target = dir.join("target.txt");
    fs::write(&target, b"hi").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let (stdout, stderr, code) = run(&[link.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(stdout.trim_end(), target.to_str().unwrap());
}

#[test]
fn canonicalize_flag_resolves_to_absolute_real_path() {
    let dir = scratch_dir("canon");
    let target = dir.join("target.txt");
    fs::write(&target, b"hi").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let (stdout, _, code) = run(&["-f", link.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), target.canonicalize().unwrap().to_str().unwrap());
}

#[test]
fn non_symlink_target_errors() {
    let dir = scratch_dir("plain");
    let file = dir.join("plain.txt");
    fs::write(&file, b"hi").unwrap();

    let (stdout, stderr, code) = run(&[file.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("readlink"));
}

#[test]
fn quiet_flag_suppresses_error_output() {
    let (stdout, stderr, code) = run(&["-q", "/no/such/path/readlink_test"]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.is_empty());
}

#[test]
fn missing_operand_errors() {
    let (_, stderr, code) = run(&[]);
    assert_eq!(code, 1);
    assert!(stderr.contains("missing operand"));
}

#[test]
fn no_newline_flag_omits_trailing_delimiter_for_single_file() {
    let dir = scratch_dir("nonewline");
    let target = dir.join("target.txt");
    fs::write(&target, b"hi").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let (stdout, _, code) = run(&["-n", link.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(stdout, target.to_str().unwrap());
}
