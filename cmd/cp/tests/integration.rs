/// Integration tests for the `cp` binary.
///
/// These drive the built binary through assert_cmd (full CLI parsing +
/// dispatch + real filesystem effects), using tempfile-backed scratch
/// directories so nothing touches the real filesystem outside the test.
use std::{fs, os::unix::fs::symlink, time::Duration};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn cp() -> Command {
    Command::cargo_bin("cp").expect("cp binary not found — run `cargo build` first")
}

// ─── Basic invocation ─────────────────────────────────────────────────────────

#[test]
fn help_flag_works() {
    cp().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"))
        .stdout(predicate::str::contains("--recursive"));
}

#[test]
fn missing_operands_fail_with_usage_error() {
    cp().assert().failure();
}

// ─── Plain file copy ──────────────────────────────────────────────────────────

#[test]
fn plain_file_copy() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let dst = dir.path().join("b.txt");
    fs::write(&src, b"hello world").unwrap();

    cp().args([src.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read(&dst).unwrap(), b"hello world");
    assert!(src.exists(), "cp must not remove the source");
}

#[test]
fn copy_into_existing_directory_keeps_basename() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let target_dir = dir.path().join("out");
    fs::write(&src, b"data").unwrap();
    fs::create_dir(&target_dir).unwrap();

    cp().args([src.to_str().unwrap(), target_dir.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read(target_dir.join("a.txt")).unwrap(), b"data");
}

// ─── Recursive directory copy ─────────────────────────────────────────────────

#[test]
fn recursive_directory_copy() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("top.txt"), b"top").unwrap();
    fs::write(src.join("nested/deep.txt"), b"deep").unwrap();

    cp().args(["-R", src.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read(dst.join("top.txt")).unwrap(), b"top");
    assert_eq!(fs::read(dst.join("nested/deep.txt")).unwrap(), b"deep");
}

#[test]
fn directory_without_recursive_flag_is_rejected() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("f.txt"), b"x").unwrap();

    cp().args([src.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .failure();

    assert!(!dst.exists(), "no partial copy should be created");
}

// ─── Symlink handling: -P (preserve link) vs -L (dereference) ────────────────

#[test]
fn no_dereference_recreates_the_symlink_itself() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.txt");
    let link = dir.path().join("link.txt");
    let dst = dir.path().join("copy.txt");
    fs::write(&target, b"real content").unwrap();
    symlink(&target, &link).unwrap();

    cp().args(["-P", link.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    let meta = fs::symlink_metadata(&dst).unwrap();
    assert!(meta.file_type().is_symlink(), "-P must copy the link itself");
}

#[test]
fn dereference_copies_symlink_target_content() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.txt");
    let link = dir.path().join("link.txt");
    let dst = dir.path().join("copy.txt");
    fs::write(&target, b"real content").unwrap();
    symlink(&target, &link).unwrap();

    cp().args(["-L", link.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    let meta = fs::symlink_metadata(&dst).unwrap();
    assert!(!meta.file_type().is_symlink(), "-L must copy real content, not a link");
    assert_eq!(fs::read(&dst).unwrap(), b"real content");
}

// ─── -p/--preserve: permissions and timestamps ────────────────────────────────

#[test]
fn preserve_flag_carries_over_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let dst = dir.path().join("b.txt");
    fs::write(&src, b"data").unwrap();
    fs::set_permissions(&src, fs::Permissions::from_mode(0o600)).unwrap();

    cp().args(["-p", src.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    let mode = fs::metadata(&dst).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

// ─── -n/--no-clobber ───────────────────────────────────────────────────────────

#[test]
fn no_clobber_refuses_to_overwrite_existing_destination() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let dst = dir.path().join("b.txt");
    fs::write(&src, b"new content").unwrap();
    fs::write(&dst, b"original content").unwrap();

    // -n prints a warning and continues rather than failing hard.
    cp().args(["-n", src.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(
        fs::read(&dst).unwrap(),
        b"original content",
        "destination must be untouched with --no-clobber"
    );
}

// ─── Atomic overwrite: no stray temp files, correct final content ────────────

#[test]
fn overwrite_leaves_no_stray_temp_files() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let dst = dir.path().join("b.txt");
    fs::write(&src, b"new content").unwrap();
    fs::write(&dst, b"old content").unwrap();

    cp().args([src.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read(&dst).unwrap(), b"new content");

    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("usercp") && n.contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");
}

// ─── --reflink=auto silently falls back when unsupported ─────────────────────

#[test]
fn reflink_auto_falls_back_silently_when_unsupported() {
    // tmpfs (where tempdir() usually lands) doesn't support FICLONE, so
    // --reflink=auto (the default) must still succeed via a normal copy
    // rather than erroring out.
    let dir = tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let dst = dir.path().join("b.txt");
    fs::write(&src, b"reflink me").unwrap();

    cp().args(["--reflink=auto", src.to_str().unwrap(), dst.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read(&dst).unwrap(), b"reflink me");
}

// ─── Self-referential symlink during recursive copy errors, doesn't hang ─────

#[test]
fn recursive_copy_rejects_symlink_cycle_instead_of_hanging() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("f.txt"), b"x").unwrap();
    // A symlink inside src pointing back at src's parent — if dereferenced,
    // this would recurse forever without cycle detection.
    symlink(&src, src.join("loop")).unwrap();
    let dst = dir.path().join("dst");

    // -L forces dereferencing, which is what would walk into the cycle;
    // the (dev, ino) visited-set in ops::tree must catch it and error out
    // instead of hanging.
    cp().args(["-R", "-L", src.to_str().unwrap(), dst.to_str().unwrap()])
        .timeout(Duration::from_secs(10))
        .assert()
        .failure();
}
