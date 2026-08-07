//! Evaluation logic for the POSIX `test` / `[` builtin.
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::Path;

/// Evaluate a `test`/`[` expression (already stripped of the trailing `]`
/// for bracket invocations) and return the process exit code: `0` (true),
/// `1` (false), or `2` (usage/evaluation error, e.g. an unknown operator or
/// a non-numeric argument to `-eq`/`-lt`/etc).
///
/// `bracket` selects the diagnostic prefix (`[:` vs `test:`) used for error
/// messages; it does not otherwise change evaluation semantics.
pub fn eval(args: &[String], bracket: bool) -> i32 {
    if args.is_empty() {
        return 1; // false
    }
    if args.len() == 1 {
        return if args[0].is_empty() { 1 } else { 0 };
    }
    // unary
    if args.len() == 2 {
        return unary(&args[0], &args[1], bracket);
    }
    // binary
    if args.len() == 3 {
        return binary(&args[0], &args[1], &args[2], bracket);
    }
    // ! EXPR
    if args[0] == "!" {
        return if eval(&args[1..], bracket) == 0 { 1 } else { 0 };
    }
    // simple -a / -o
    if let Some(pos) = args.iter().position(|a| a == "-a") {
        let left = eval(&args[..pos], bracket);
        let right = eval(&args[pos + 1..], bracket);
        return if left == 0 && right == 0 { 0 } else { 1 };
    }
    if let Some(pos) = args.iter().position(|a| a == "-o") {
        let left = eval(&args[..pos], bracket);
        let right = eval(&args[pos + 1..], bracket);
        return if left == 0 || right == 0 { 0 } else { 1 };
    }
    // STRING1 = STRING2 with more? treat as binary first three
    if args.len() >= 3 {
        return binary(&args[0], &args[1], &args[2], bracket);
    }
    2
}

fn prog(bracket: bool) -> &'static str {
    if bracket {
        "["
    } else {
        "test"
    }
}

fn unary(op: &str, arg: &str, bracket: bool) -> i32 {
    let p = Path::new(arg);
    let ok = match op {
        "-b" => meta(p)
            .map(|m| m.file_type().is_block_device())
            .unwrap_or(false),
        "-c" => meta(p)
            .map(|m| m.file_type().is_char_device())
            .unwrap_or(false),
        "-d" => p.is_dir(),
        "-e" => p.exists(),
        "-f" => p.is_file(),
        "-g" => meta(p).map(|m| m.mode() & 0o2000 != 0).unwrap_or(false),
        "-h" | "-L" => p
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        "-n" => !arg.is_empty(),
        "-p" => meta(p).map(|m| m.file_type().is_fifo()).unwrap_or(false),
        "-r" => p
            .metadata()
            .map(|m| m.permissions().mode() & 0o444 != 0)
            .unwrap_or(false),
        "-S" => meta(p).map(|m| m.file_type().is_socket()).unwrap_or(false),
        "-s" => meta(p).map(|m| m.len() > 0).unwrap_or(false),
        "-t" => match arg.parse::<i32>() {
            Ok(fd) => {
                // SAFETY: `libc::isatty` takes a plain `c_int` file
                // descriptor and performs no pointer dereferences. It is
                // defined behavior for any `i32` value, valid or not — an
                // invalid/unopened fd simply makes it return 0 and set
                // errno to EBADF, it cannot cause UB.
                unsafe { libc::isatty(fd) != 0 }
            }
            Err(_) => {
                eprintln!("{}: {arg}: not a valid file descriptor", prog(bracket));
                return 2;
            }
        },
        "-u" => meta(p).map(|m| m.mode() & 0o4000 != 0).unwrap_or(false),
        "-w" => p
            .metadata()
            .map(|m| m.permissions().mode() & 0o222 != 0)
            .unwrap_or(false),
        "-x" => p
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false),
        "-z" => arg.is_empty(),
        "!" => return if arg.is_empty() { 0 } else { 1 },
        _ => {
            eprintln!("{}: {op}: unknown unary operator", prog(bracket));
            return 2;
        }
    };
    bool_to_code(ok)
}

fn binary(a: &str, op: &str, b: &str, bracket: bool) -> i32 {
    match op {
        "=" | "==" => bool_to_code(a == b),
        "!=" => bool_to_code(a != b),
        "-eq" | "-ne" | "-gt" | "-ge" | "-lt" | "-le" => match nums(a, b) {
            Some((x, y)) => bool_to_code(match op {
                "-eq" => x == y,
                "-ne" => x != y,
                "-gt" => x > y,
                "-ge" => x >= y,
                "-lt" => x < y,
                _ => x <= y,
            }),
            None => {
                // Regression guard: a non-numeric operand here must be a
                // hard evaluation error (exit 2), not silently "false"
                // (exit 1) — the two are observably different to callers
                // (e.g. `if`/`&&` chains, scripts checking `$?`).
                let bad = if a.parse::<i64>().is_err() { a } else { b };
                eprintln!("{}: {bad}: integer expression expected", prog(bracket));
                2
            }
        },
        "-ef" => bool_to_code(same_file(a, b)),
        "-nt" => bool_to_code(newer(a, b)),
        "-ot" => bool_to_code(newer(b, a)),
        _ => {
            eprintln!("{}: {op}: unknown binary operator", prog(bracket));
            2
        }
    }
}

fn bool_to_code(ok: bool) -> i32 {
    if ok {
        0
    } else {
        1
    }
}

fn meta(p: &Path) -> Option<fs::Metadata> {
    fs::symlink_metadata(p).ok()
}

fn nums(a: &str, b: &str) -> Option<(i64, i64)> {
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn same_file(a: &str, b: &str) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

fn newer(a: &str, b: &str) -> bool {
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.mtime() > y.mtime(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_is_false() {
        assert_eq!(eval(&[], false), 1);
    }

    #[test]
    fn single_nonempty_string_is_true() {
        assert_eq!(eval(&args(&["hello"]), false), 0);
    }

    #[test]
    fn single_empty_string_is_false() {
        assert_eq!(eval(&args(&[""]), false), 1);
    }

    #[test]
    fn string_equality() {
        assert_eq!(eval(&args(&["a", "=", "a"]), false), 0);
        assert_eq!(eval(&args(&["a", "=", "b"]), false), 1);
        assert_eq!(eval(&args(&["a", "!=", "b"]), false), 0);
    }

    #[test]
    fn numeric_comparisons() {
        assert_eq!(eval(&args(&["3", "-eq", "3"]), false), 0);
        assert_eq!(eval(&args(&["3", "-lt", "4"]), false), 0);
        assert_eq!(eval(&args(&["4", "-gt", "3"]), false), 0);
        assert_eq!(eval(&args(&["4", "-le", "4"]), false), 0);
        assert_eq!(eval(&args(&["4", "-ge", "4"]), false), 0);
        assert_eq!(eval(&args(&["3", "-ne", "4"]), false), 0);
    }

    #[test]
    fn non_numeric_comparison_is_a_hard_error_not_false() {
        // Regression: previously `nums(...).unwrap_or(false)` made this
        // indistinguishable from a legitimate "false" (exit 1) result.
        assert_eq!(eval(&args(&["abc", "-eq", "3"]), false), 2);
        assert_eq!(eval(&args(&["3", "-eq", "abc"]), false), 2);
    }

    #[test]
    fn negation() {
        assert_eq!(eval(&args(&["!", ""]), false), 0);
        assert_eq!(eval(&args(&["!", "x"]), false), 1);
    }

    #[test]
    fn logical_and_or() {
        assert_eq!(eval(&args(&["a", "-a", "b"]), false), 0);
        assert_eq!(eval(&args(&["", "-a", "b"]), false), 1);
        assert_eq!(eval(&args(&["", "-o", "b"]), false), 0);
        assert_eq!(eval(&args(&["", "-o", ""]), false), 1);
    }

    #[test]
    fn file_existence() {
        let p = std::env::temp_dir().join(format!("user_test_cmd_{}_exists", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(eval(&args(&["-e", p.to_str().unwrap()]), false), 0);
        assert_eq!(eval(&args(&["-f", p.to_str().unwrap()]), false), 0);
        assert_eq!(eval(&args(&["-d", p.to_str().unwrap()]), false), 1);
        let _ = std::fs::remove_file(&p);
        assert_eq!(eval(&args(&["-e", p.to_str().unwrap()]), false), 1);
    }

    #[test]
    fn unknown_unary_operator_is_error() {
        assert_eq!(eval(&args(&["-Q", "x"]), false), 2);
    }

    #[test]
    fn unknown_binary_operator_is_error() {
        assert_eq!(eval(&args(&["a", "-Q", "b"]), false), 2);
    }

    #[test]
    fn newer_and_ef_comparisons() {
        let dir = std::env::temp_dir().join(format!("user_test_cmd_{}_newer", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a");
        let b = dir.join("b");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        assert_eq!(
            eval(
                &args(&[a.to_str().unwrap(), "-ef", a.to_str().unwrap()]),
                false
            ),
            0
        );
        assert_eq!(
            eval(
                &args(&[a.to_str().unwrap(), "-ef", b.to_str().unwrap()]),
                false
            ),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
