//! End-to-end CLI tests for `printenv`, exercised by spawning the built binary.

use std::process::Command;

fn run(args: &[&str], envs: &[(&str, &str)]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_printenv"))
        .args(args)
        .env_clear()
        .envs(envs.iter().copied())
        .output()
        .expect("spawn printenv");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn prints_requested_variable() {
    let (stdout, stderr, code) = run(&["FOO"], &[("FOO", "bar")]);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(stdout, "bar\n");
}

#[test]
fn prints_all_variables_when_none_requested() {
    let (stdout, _, code) = run(&[], &[("FOO", "bar")]);
    assert_eq!(code, 0);
    assert!(stdout.contains("FOO=bar\n"));
}

#[test]
fn missing_variable_is_silent_but_nonzero_status() {
    let (stdout, stderr, code) = run(&["NO_SUCH_VAR_HERE"], &[]);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.is_empty());
}

#[test]
fn null_flag_uses_nul_separator() {
    let (stdout, _, code) = run(&["-0", "FOO"], &[("FOO", "bar")]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "bar\0");
}

#[test]
fn invalid_option_errors() {
    let (_, stderr, code) = run(&["--bogus"], &[]);
    assert_eq!(code, 1);
    assert!(stderr.contains("invalid option"));
}
