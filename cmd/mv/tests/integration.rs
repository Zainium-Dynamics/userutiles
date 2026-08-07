// tests/integration.rs — Integration tests for xmv.
//
// Each test creates an isolated temp directory, performs operations through
// the public API, and asserts filesystem state. All tests clean up after
// themselves.
//
// Run with: cargo test

use std::{fs, path::PathBuf};

use xmv::{
    ops::rename,
    undo::{Journal, Operation},
    verify,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xmv_test_{tag}_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &std::path::Path, content: &[u8]) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn journal(dir: &std::path::Path) -> Journal {
    Journal::open(dir.join("journal.toml")).unwrap()
}

// ─── rename: same-device overwrite ───────────────────────────────────────────

#[test]
fn test_rename_overwrite() {
    let dir = tmp_dir("rename_overwrite");
    let src = dir.join("src.txt");
    let dest = dir.join("dest.txt");

    write(&src, b"hello");
    rename::rename_overwrite(&src, &dest).unwrap();

    assert!(!src.exists(), "src should be gone");
    assert_eq!(fs::read(&dest).unwrap(), b"hello");
    fs::remove_dir_all(&dir).ok();
}

// ─── rename: same-device no-replace succeeds when dest absent ────────────────

#[test]
fn test_rename_no_replace_succeeds() {
    let dir = tmp_dir("no_replace_ok");
    let src = dir.join("a.txt");
    let dest = dir.join("b.txt");

    write(&src, b"data");
    let result = rename::rename_no_replace(&src, &dest);

    // On kernels < 3.15 this returns Renameat2Unsupported (not an error per se).
    match result {
        Ok(()) => {
            assert!(!src.exists());
            assert_eq!(fs::read(&dest).unwrap(), b"data");
        }
        Err(xmv::error::XmvError::Renameat2Unsupported) => {
            // Acceptable on old kernels — caller falls back to rename_overwrite.
        }
        Err(e) => panic!("Unexpected error: {e}"),
    }
    fs::remove_dir_all(&dir).ok();
}

// ─── rename: no-replace rejects existing dest ────────────────────────────────

#[test]
fn test_rename_no_replace_rejects_existing() {
    let dir = tmp_dir("no_replace_reject");
    let src = dir.join("a.txt");
    let dest = dir.join("b.txt");

    write(&src, b"new");
    write(&dest, b"existing");

    let result = rename::rename_no_replace(&src, &dest);

    match result {
        Err(xmv::error::XmvError::Renameat2Unsupported) => {
            // Old kernel — acceptable.
        }
        Err(_) => {
            // Any other error means the operation was correctly rejected.
            // dest must be untouched.
            assert_eq!(fs::read(&dest).unwrap(), b"existing");
        }
        Ok(()) => panic!("rename_no_replace should have failed with existing dest"),
    }
    fs::remove_dir_all(&dir).ok();
}

// ─── atomic exchange ─────────────────────────────────────────────────────────

#[test]
fn test_atomic_exchange() {
    let dir = tmp_dir("exchange");
    let path_a = dir.join("a.txt");
    let path_b = dir.join("b.txt");

    write(&path_a, b"content_a");
    write(&path_b, b"content_b");

    let mut j = journal(&dir);
    let result = xmv::ops::atomic::atomic_exchange(&path_a, &path_b, &mut j);

    match result {
        Ok(()) => {
            // After exchange: a has b's old content, b has a's old content.
            assert_eq!(fs::read(&path_a).unwrap(), b"content_b");
            assert_eq!(fs::read(&path_b).unwrap(), b"content_a");
        }
        Err(xmv::error::XmvError::Renameat2Unsupported) => {
            // Old kernel — fallback path attempted inside atomic_exchange.
        }
        Err(e) => panic!("Exchange failed: {e}"),
    }
    fs::remove_dir_all(&dir).ok();
}

// ─── atomic exchange: missing path returns error ─────────────────────────────

#[test]
fn test_atomic_exchange_missing_path() {
    let dir = tmp_dir("exchange_missing");
    let path_a = dir.join("exists.txt");
    let path_b = dir.join("does_not_exist.txt");

    write(&path_a, b"data");

    let mut j = journal(&dir);
    let err = xmv::ops::atomic::atomic_exchange(&path_a, &path_b, &mut j)
        .expect_err("Should fail — path_b missing");

    assert!(
        matches!(err, xmv::error::XmvError::SourceNotFound(_)),
        "Expected SourceNotFound, got: {err}"
    );
    fs::remove_dir_all(&dir).ok();
}

// ─── cross-device move: copy + verify + delete ───────────────────────────────
// Note: this test runs on the same device (tmp), which exercises the copy
// engine. True cross-device scenarios require two separate mounts.

#[test]
fn test_crossdev_move_file() {
    let dir = tmp_dir("crossdev_file");
    let src = dir.join("src.bin");
    let dest = dir.join("dest_dir");

    let data: Vec<u8> = (0u32..65536).flat_map(|i| i.to_le_bytes()).collect();
    write(&src, &data);
    fs::create_dir_all(&dest).unwrap();

    let (tx, rx) = crossbeam_channel::unbounded();
    let drain = std::thread::spawn(move || for _ in rx {});

    xmv::ops::crossdev::move_cross_device(
        &src, &dest, 2,     // jobs
        true,  // verify
        false, // preserve_meta
        tx,
    )
    .unwrap();

    drain.join().ok();

    let dest_file = dest.join("src.bin");
    assert!(!src.exists(), "source should be deleted");
    assert_eq!(
        fs::read(&dest_file).unwrap(),
        data,
        "content must be intact"
    );
    fs::remove_dir_all(&dir).ok();
}

// ─── cross-device move: empty directory ──────────────────────────────────────

#[test]
fn test_crossdev_move_empty_dir() {
    let dir = tmp_dir("crossdev_empty");
    let src = dir.join("empty_src");
    let dest = dir.join("empty_dest");

    fs::create_dir_all(&src).unwrap();

    let (tx, rx) = crossbeam_channel::unbounded();
    let drain = std::thread::spawn(move || for _ in rx {});

    xmv::ops::crossdev::move_cross_device(&src, &dest, 1, false, false, tx).unwrap();
    drain.join().ok();

    assert!(!src.exists(), "empty source dir should be removed");
    assert!(dest.exists(), "dest dir should be created");
    fs::remove_dir_all(&dir).ok();
}

// ─── verify: passes on identical files ───────────────────────────────────────

#[test]
fn test_verify_ok() {
    let dir = tmp_dir("verify_ok");
    let a = dir.join("a.bin");
    let b = dir.join("b.bin");

    let data: Vec<u8> = (0u8..=255).cycle().take(1024 * 1024).collect();
    write(&a, &data);
    write(&b, &data);

    verify::verify(&a, &b).expect("identical files must verify");
    fs::remove_dir_all(&dir).ok();
}

// ─── verify: catches corruption ──────────────────────────────────────────────

#[test]
fn test_verify_detects_corruption() {
    let dir = tmp_dir("verify_corrupt");
    let a = dir.join("good.bin");
    let b = dir.join("bad.bin");

    write(&a, b"original");
    write(&b, b"corrupted");

    let err = verify::verify(&a, &b).expect_err("should fail on mismatch");
    assert!(matches!(err, xmv::error::XmvError::ChecksumMismatch { .. }));
    fs::remove_dir_all(&dir).ok();
}

// ─── undo journal: record + commit + undo a move ─────────────────────────────

#[test]
fn test_undo_move() {
    let dir = tmp_dir("undo_move");
    let src = dir.join("original.txt");
    let dest = dir.join("moved.txt");

    write(&src, b"undo me");

    // Perform the move manually so we control the journal.
    let mut j = journal(&dir);
    let offset = j
        .record(Operation::Move {
            src: src.clone(),
            dest: dest.clone(),
        })
        .unwrap();

    rename::rename_overwrite(&src, &dest).unwrap();
    j.commit(offset).unwrap();

    assert!(!src.exists());
    assert!(dest.exists());

    // Undo — should move dest back to src.
    j.undo_last().unwrap();

    assert!(src.exists(), "src should be restored after undo");
    assert!(!dest.exists(), "dest should be gone after undo");
    assert_eq!(fs::read(&src).unwrap(), b"undo me");
    fs::remove_dir_all(&dir).ok();
}

// ─── undo: reverses a move performed via the cross-device engine ─────────────
// Regression test for the bug where undo_last() used a plain fs::rename()
// for every Move entry, which fails with EXDEV for moves that actually went
// through the cross-device copy+verify+delete engine (the very moves that
// most need --undo, since the original source is already gone). Like
// test_crossdev_move_file, this runs on a single filesystem in CI so it
// exercises the reused crossdev::move_cross_device reversal path rather than
// a real EXDEV — genuine cross-device coverage needs two separate mounts.

#[test]
fn test_undo_move_performed_via_crossdev_engine() {
    let dir = tmp_dir("undo_crossdev");
    let src = dir.join("payload.bin");
    let dest_dir = dir.join("dest");
    let dest = dest_dir.join("payload.bin");

    let data: Vec<u8> = (0u16..20000).flat_map(|i| i.to_le_bytes()).collect();
    write(&src, &data);
    fs::create_dir_all(&dest_dir).unwrap();

    let mut j = journal(&dir);
    let offset = j
        .record(Operation::Move {
            src: src.clone(),
            dest: dest.clone(),
        })
        .unwrap();

    let (tx, rx) = crossbeam_channel::unbounded();
    let drain = std::thread::spawn(move || for _ in rx {});
    xmv::ops::crossdev::move_cross_device(&src, &dest_dir, 2, true, false, tx).unwrap();
    drain.join().ok();
    j.commit(offset).unwrap();

    assert!(!src.exists(), "source should be gone after the move");
    assert!(dest.exists(), "payload should be at dest after the move");

    j.undo_last().unwrap();

    assert!(src.exists(), "src should be restored after undo");
    assert!(!dest.exists(), "dest copy should be gone after undo");
    assert_eq!(
        fs::read(&src).unwrap(),
        data,
        "content must survive the round trip"
    );
    fs::remove_dir_all(&dir).ok();
}

// ─── undo: exchange is its own inverse ───────────────────────────────────────

#[test]
fn test_undo_exchange() {
    let dir = tmp_dir("undo_exchange");
    let path_a = dir.join("a.txt");
    let path_b = dir.join("b.txt");

    write(&path_a, b"alpha");
    write(&path_b, b"beta");

    let mut j = journal(&dir);
    let result = xmv::ops::atomic::atomic_exchange(&path_a, &path_b, &mut j);

    // Only assert undo on kernels that support renameat2.
    if result.is_ok() {
        // atomic_exchange only records *intent*; the caller (run.rs) commits
        // it after success so a crash mid-operation leaves a safely-ignorable
        // uncommitted entry. Mirror that here or undo_last() will correctly
        // refuse to undo an uncommitted op.
        j.commit_last().unwrap();

        // After exchange: a=beta, b=alpha
        assert_eq!(fs::read(&path_a).unwrap(), b"beta");

        j.undo_last().unwrap();

        // After undo (exchange again): a=alpha, b=beta
        assert_eq!(fs::read(&path_a).unwrap(), b"alpha");
        assert_eq!(fs::read(&path_b).unwrap(), b"beta");
    }

    fs::remove_dir_all(&dir).ok();
}

// ─── trash: file is relocated to XDG Trash ───────────────────────────────────

#[test]
fn test_trash_move() {
    let dir = tmp_dir("trash");
    let file = dir.join("disposable.txt");

    write(&file, b"trash me");

    let trash_path = xmv::trash::move_to_trash(&file).unwrap();

    assert!(!file.exists(), "original should be gone after trash");
    assert!(trash_path.exists(), "file should appear in Trash/files/");

    // .trashinfo must exist alongside the trashed file.
    let info = trash_path
        .with_extension("txt.trashinfo")
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("info")
        .join(format!(
            "{}.trashinfo",
            trash_path.file_name().unwrap().to_string_lossy()
        ));

    assert!(info.exists(), ".trashinfo file must exist");

    // Clean up: remove the trashed file and its .trashinfo.
    fs::remove_file(&trash_path).ok();
    fs::remove_file(&info).ok();
    fs::remove_dir_all(&dir).ok();
}
