// Integration tests: these test the prio binary end-to-end using assert_cmd.
// They require a Linux host; tests that touch real scheduling syscalls are
// guarded by #[cfg(target_os = "linux")].

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn prio() -> Command {
    Command::cargo_bin("prio").expect("prio binary not found — run `cargo build` first")
}

// ── Help / Version ────────────────────────────────────────────────────────────

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

// ── --list ────────────────────────────────────────────────────────────────────

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

// ── Nice Validation ───────────────────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn invalid_nice_too_low_rejected() {
    prio()
        .args(["-n", "-21", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty()); // error is on stdout via our own renderer
}

#[test]
#[cfg(target_os = "linux")]
fn invalid_nice_too_high_rejected() {
    prio().args(["-n", "20", "true"]).assert().failure();
}

// ── CPU Level ─────────────────────────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn cpu_level_invalid_rejected() {
    prio().args(["--cpu", "101", "true"]).assert().failure();
}

// ── Memory Parse ──────────────────────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn invalid_max_ram_rejected() {
    prio()
        .args(["--max-ram", "notanumber", "true"])
        .assert()
        .failure();
}

// ── Duration Parse ────────────────────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn invalid_time_rejected() {
    prio().args(["--time", "5d", "true"]).assert().failure();
}

// ── --reset ───────────────────────────────────────────────────────────────────

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

// ── Spawn: normal niceness (no root needed) ───────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn spawn_with_positive_nice_succeeds() {
    // +10 can be set without root.
    prio().args(["-n", "10", "--", "true"]).assert().success();
}

// ── Spawn: inherit exit code ──────────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn spawn_exit_code_inherited() {
    prio()
        .args(["-n", "10", "--", "sh", "-c", "exit 42"])
        .assert()
        .code(42);
}

// ── I/O mode parsing ─────────────────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn unknown_io_mode_rejected() {
    prio()
        .args(["--io", "turbo", "--", "true"])
        .assert()
        .failure();
}

// ── --boost with non-existent PID ─────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn boost_nonexistent_pid_fails() {
    // PID 9999999 is almost certainly not running.
    prio().args(["--boost", "9999999"]).assert().failure();
}

// ── --pid with non-existent PID ───────────────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn pid_nonexistent_fails() {
    prio()
        .args(["--pid", "9999999", "-n", "5"])
        .assert()
        .failure();
}

// ── Verbose flag doesn't break basic flow ────────────────────────────────────

#[test]
#[cfg(target_os = "linux")]
fn verbose_spawn_succeeds() {
    prio()
        .args(["-v", "-n", "10", "--", "true"])
        .assert()
        .success();
}
