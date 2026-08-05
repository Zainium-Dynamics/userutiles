//! End-to-end CLI tests for `ptx`, exercised by spawning the built binary.

use std::io::Write;
use std::process::{Command, Stdio};

fn run_with_stdin(args: &[&str], input: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ptx"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ptx");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait ptx");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn golden_path_sorts_by_keyword_case_insensitively() {
    let (stdout, stderr, code) = run_with_stdin(&[], "Banana apple\n");
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    // Each keyword is right-aligned to start at column 31 (30-wide left
    // context field + a space). "apple" should sort before "Banana"
    // (case-insensitive), so it's the keyword on the first output line.
    assert!(lines[0][31..].starts_with("apple"));
    assert!(lines[1][31..].starts_with("Banana"));
}

#[test]
fn empty_input_produces_no_output() {
    let (stdout, _, code) = run_with_stdin(&[], "");
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
}

#[test]
fn missing_file_errors() {
    let (_, stderr, code) = run_with_stdin(&["/no/such/file/ptx_test"], "");
    assert_eq!(code, 1);
    assert!(stderr.contains("ptx"));
}

#[test]
fn invalid_option_errors() {
    let (_, stderr, code) = run_with_stdin(&["--bogus"], "");
    assert_eq!(code, 1);
    assert!(stderr.contains("invalid option"));
}
