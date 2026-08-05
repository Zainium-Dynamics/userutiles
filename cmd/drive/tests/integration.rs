/// Integration tests for the `drive` binary.
///
/// These tests exercise the CLI through assert_cmd so they validate the full
/// argument parsing, dispatch, and output formatting pipeline without needing
/// root or physical block devices. All tests that touch the filesystem use
/// temporary directories provided by the `tempfile` crate.
use assert_cmd::Command;
use predicates::prelude::*;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn drive() -> Command {
    Command::cargo_bin("drive").expect("drive binary not found — run `cargo build` first")
}

// ─── Basic invocation ─────────────────────────────────────────────────────────

#[test]
fn no_args_prints_usage() {
    drive()
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ZainiumOS Advanced Storage Manager",
        ))
        .stdout(predicate::str::contains("drive list"))
        .stdout(predicate::str::contains("drive health"));
}

#[test]
fn version_flag_works() {
    drive()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("drive"));
}

#[test]
fn help_flag_works() {
    drive()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"))
        .stdout(predicate::str::contains("Commands"))
        .stdout(predicate::str::contains("Options"));
}

// ─── Subcommand help ──────────────────────────────────────────────────────────

#[test]
fn format_help_shows_fs_flag() {
    drive()
        .args(["format", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--fs"))
        .stdout(predicate::str::contains("ext4"));
}

#[test]
fn mount_help_shows_mountpoint_flag() {
    drive()
        .args(["mount", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mountpoint").or(predicate::str::contains("DEVICE")));
}

#[test]
fn snapshot_help_shows_subcommands() {
    drive()
        .args(["snapshot", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("delete"));
}

#[test]
fn benchmark_help_shows_flags() {
    drive()
        .args(["benchmark", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("block-size-kib").or(predicate::str::contains("duration")),
        );
}

#[test]
fn clone_help_shows_verify_flag() {
    drive()
        .args(["clone", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("verify"));
}

#[test]
fn repair_help_shows_dry_run_flag() {
    drive()
        .args(["repair", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"));
}

#[test]
fn health_help_shows_device_arg() {
    drive()
        .args(["health", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("device").or(predicate::str::contains("DEVICE")));
}

// ─── Error paths (no real device needed) ─────────────────────────────────────

#[test]
fn info_nonexistent_device_exits_ok_with_error_message() {
    // drive prints a colored error and returns Ok — it does not panic
    drive()
        .args(["info", "drive_no_such_device_xyzzy"])
        .assert()
        .success()
        // Should either say "not found" or list devices and not find ours
        .stdout(
            predicate::str::is_match("(?i)(not found|unknown)")
                .unwrap()
                .or(predicate::str::contains("drive_no_such_device")),
        );
}

#[test]
fn mount_nonexistent_device_exits_ok_with_error_message() {
    drive()
        .args(["mount", "/dev/drive_no_such_xyzzy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✖").or(predicate::str::contains("not found")));
}

#[test]
fn umount_nonexistent_device_exits_ok() {
    drive()
        .args(["umount", "/dev/drive_no_such_xyzzy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✖").or(predicate::str::contains("not found")));
}

#[test]
fn repair_nonexistent_device_exits_ok() {
    drive()
        .args(["repair", "/dev/drive_no_such_xyzzy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✖").or(predicate::str::contains("not found")));
}

#[test]
fn format_nonexistent_device_exits_ok() {
    drive()
        .args(["format", "/dev/drive_no_such_xyzzy", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✖").or(predicate::str::contains("not found")));
}

#[test]
fn clone_nonexistent_source_exits_ok() {
    drive()
        .args(["clone", "/dev/drive_no_such_src", "/dev/drive_no_such_dst"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✖").or(predicate::str::contains("not found")));
}

// ─── TOML output (user_utils: TOML only, never JSON) ──────────────────────────

#[test]
fn list_toml_produces_valid_table() {
    let out = drive()
        .args(["--toml", "list"])
        .output()
        .expect("failed to run drive");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Result<toml::Value, _> = toml::from_str(&stdout);
    assert!(parsed.is_ok(), "Expected valid TOML, got:\n{stdout}");

    let val = parsed.unwrap();
    assert!(
        val.get("devices").is_some(),
        "Expected devices table, got: {val}"
    );
}

#[test]
fn health_toml_produces_valid_table() {
    let out = drive()
        .args(["--toml", "health"])
        .output()
        .expect("failed to run drive");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Result<toml::Value, _> = toml::from_str(&stdout);
    assert!(parsed.is_ok(), "Expected valid TOML, got:\n{stdout}");
}

// ─── Benchmark against a real temp directory ─────────────────────────────────

#[test]
fn benchmark_against_tmp_directory() {
    use tempfile::tempdir;
    let dir = tempdir().expect("cannot create tempdir");
    let path = dir.path().to_str().unwrap();

    // Use very short duration so the test is fast
    drive()
        .args([
            "benchmark",
            path,
            "--block-size-kib",
            "64",
            "--duration-secs",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sequential"))
        .stdout(predicate::str::contains("MB/s"));
}

// ─── Snapshot against a non-btrfs path (graceful fail) ───────────────────────

#[test]
fn snapshot_create_on_non_btrfs_reports_gracefully() {
    use tempfile::tempdir;
    let dir = tempdir().expect("cannot create tempdir");
    let path = dir.path().to_str().unwrap();

    drive()
        .args(["snapshot", "create", "--volume", path])
        .assert()
        .success()
        // Should report that btrfs is required, not panic
        .stdout(predicate::str::contains("btrfs").or(predicate::str::contains("✖")));
}

// ─── Invalid subcommand rejected ─────────────────────────────────────────────

#[test]
fn unknown_subcommand_is_rejected() {
    drive().arg("frobinate").assert().failure(); // clap returns non-zero for unknown subcommands
}
