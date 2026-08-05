//! End-to-end CLI tests for `pwd`, exercised by spawning the built binary.

use std::process::Command;

fn run_in(dir: &std::path::Path, args: &[&str], pwd_env: Option<&str>) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pwd"));
    cmd.args(args).current_dir(dir);
    match pwd_env {
        Some(v) => {
            cmd.env("PWD", v);
        }
        None => {
            cmd.env_remove("PWD");
        }
    }
    let out = cmd.output().expect("spawn pwd");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn golden_path_prints_real_cwd_without_pwd_env() {
    let dir = std::env::temp_dir();
    let canon = dir.canonicalize().unwrap();
    let (stdout, stderr, code) = run_in(&dir, &[], None);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(stdout.trim_end(), canon.to_str().unwrap());
}

#[test]
fn physical_flag_resolves_symlinks() {
    let base = std::env::temp_dir().join(format!("user_pwd_test_{}", std::process::id()));
    let real = base.join("real");
    let link = base.join("link");
    std::fs::create_dir_all(&real).unwrap();
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&real, &link).expect("create symlink");

    let canon_real = real.canonicalize().unwrap();
    let (stdout, _, code) = run_in(&link, &["-P"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), canon_real.to_str().unwrap());

    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn logical_flag_trusts_matching_pwd_env() {
    let dir = std::env::temp_dir().canonicalize().unwrap();
    let (stdout, stderr, code) = run_in(&dir, &["-L"], Some(dir.to_str().unwrap()));
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(stdout.trim_end(), dir.to_str().unwrap());
}

#[test]
fn invalid_option_errors_with_status_2() {
    let dir = std::env::temp_dir();
    let (_, stderr, code) = run_in(&dir, &["--bogus"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("invalid option"));
}
