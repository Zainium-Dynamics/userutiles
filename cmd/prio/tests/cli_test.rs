// cli_test.rs — Binary-level CLI tests for prio.
//
// Unit-level tests for pure functions live as inline `#[cfg(test)]` blocks
// inside `src/utils/priority.rs` and `src/utils/timebound.rs`, run via
// `cargo test --bin prio`. Tests here exercise the compiled binary through
// assert_cmd.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn prio() -> Command {
    Command::cargo_bin("prio").expect("prio binary not found")
}

#[test]
fn cli_help_exits_zero() {
    prio().arg("--help").assert().success();
}

#[test]
fn cli_help_contains_product_name() {
    prio()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ZainiumOS Process Priority & Resource Scheduler",
        ));
}

#[test]
fn cli_version_contains_semver() {
    prio()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
#[cfg(target_os = "linux")]
fn cli_nice_out_of_range_low() {
    prio().args(["-n", "-21", "true"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_nice_out_of_range_high() {
    prio().args(["-n", "20", "true"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_cpu_over_100_rejected() {
    prio().args(["--cpu", "101", "true"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_unknown_io_mode_rejected() {
    prio()
        .args(["--io", "warp", "--", "true"])
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_bad_max_ram_rejected() {
    prio()
        .args(["--max-ram", "???", "--", "true"])
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_bad_duration_rejected() {
    prio()
        .args(["--time", "99d", "--", "true"])
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_list_exits_zero() {
    prio().arg("--list").assert().success();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_list_shows_heading() {
    prio()
        .arg("--list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Top Processes"));
}

#[test]
#[cfg(target_os = "linux")]
fn cli_spawn_positive_nice_exits_zero() {
    prio().args(["-n", "10", "--", "true"]).assert().success();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_spawn_exit_code_forwarded() {
    prio()
        .args(["-n", "5", "--", "sh", "-c", "exit 7"])
        .assert()
        .code(7);
}

#[test]
#[cfg(target_os = "linux")]
fn cli_spawn_cpu_shorthand() {
    prio()
        .args(["--cpu", "30", "--", "true"])
        .assert()
        .success();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_reset_self_pid_exits_zero() {
    let pid = std::process::id().to_string();
    prio().args(["--reset", &pid]).assert().success();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_reset_nonexistent_pid_fails() {
    prio().args(["--reset", "9999997"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_boost_nonexistent_pid_fails() {
    prio().args(["--boost", "9999998"]).assert().failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_pid_nonexistent_fails() {
    prio()
        .args(["--pid", "9999996", "-n", "5"])
        .assert()
        .failure();
}

#[test]
#[cfg(target_os = "linux")]
fn cli_verbose_flag_does_not_break_spawn() {
    prio()
        .args(["-v", "-n", "10", "--", "true"])
        .assert()
        .success();
}
