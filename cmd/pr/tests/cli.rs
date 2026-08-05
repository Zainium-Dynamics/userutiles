//! End-to-end CLI tests for `pr`, exercised by spawning the built binary.

use std::io::Write;
use std::process::{Command, Stdio};

fn run_with_stdin(args: &[&str], input: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pr"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pr");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait pr");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn golden_path_numbers_lines_with_t_flag() {
    let (stdout, stderr, code) = run_with_stdin(&["-t", "-n"], "one\ntwo\n");
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("1\tone"));
    assert!(stdout.contains("2\ttwo"));
}

#[test]
fn missing_file_reports_error_and_nonzero_status() {
    let (_, stderr, code) = run_with_stdin(&["/no/such/file/pr_test"], "");
    assert_eq!(code, 1);
    assert!(stderr.contains("pr"));
}

#[test]
fn invalid_page_length_errors_instead_of_silently_defaulting() {
    let (_, stderr, code) = run_with_stdin(&["-l", "notanumber"], "x\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("invalid page length"));
}

#[test]
fn empty_input_produces_no_output_lines() {
    let (stdout, _, code) = run_with_stdin(&["-t"], "");
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
}
