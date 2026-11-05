use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn prio() -> Command {
    Command::cargo_bin("prio").expect("prio binary not found — run `cargo build` first")
}

#[test]
fn help_flag_succeeds() {
    prio()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ZainiumOS"));
}

#[test]
fn version_flag_succeeds() {
    prio()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
#[cfg(target_os = "linux")]
fn list_shows_processes() {
    prio()
        .arg("--list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Top Processes by Priority"))
        .stdout(predicate::str::contains("PID"));
}

#[test]
#[cfg(target_os = "linux")]
fn invalid_nice_too_low_rejected() {
    prio()
        .args(["-n", "-21", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty());
}

#[test]
#[cfg(target_os = "linux")]
fn invalid_nice_too_high_rejected() {
    prio().args(["-n", "20", "true"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cpu_level_invalid_rejected() {
    prio().args(["--cpu", "101", "true"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn invalid_max_ram_rejected() {
    prio()
        .args(["--max-ram", "notanumber", "true"])
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "linux")]
fn invalid_time_rejected() {
    prio().args(["--time", "5d", "true"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn reset_self_succeeds() {
    let pid = std::process::id();
    prio()
        .args(["--reset", &pid.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reset"));
}

#[test]
#[cfg(target_os = "linux")]
fn spawn_with_positive_nice_succeeds() {
    prio().args(["-n", "10", "--", "true"]).assert().success();
}

#[test]
#[cfg(target_os = "linux")]
fn spawn_exit_code_inherited() {
    prio()
        .args(["-n", "10", "--", "sh", "-c", "exit 42"])
        .assert()
        .code(42);
}

#[test]
#[cfg(target_os = "linux")]
fn unknown_io_mode_rejected() {
    prio()
        .args(["--io", "turbo", "--", "true"])
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "linux")]
fn boost_nonexistent_pid_fails() {
    prio().args(["--boost", "9999999"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn pid_nonexistent_fails() {
    prio()
        .args(["--pid", "9999999", "-n", "5"])
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "linux")]
fn verbose_spawn_succeeds() {
    prio()
        .args(["-v", "-n", "10", "--", "true"])
        .assert()
        .success();
}
