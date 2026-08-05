//! user pathchk — diagnose invalid pathnames.
use std::path::Path;

use usercore::Ui;

/// Entry point for the `pathchk` utility. Parses `std::env::args()` as
/// `[-p|-P] NAME...` and diagnoses each NAME for validity (empty, embedded
/// NUL, overlong components/path) and, with `-p`/`-P`, portability (non-
/// portable characters, POSIX length limits).
///
/// Returns 0 if all NAMEs are valid, 1 if any diagnostic was printed.
pub fn run() -> i32 {
    let ui = Ui::new("pathchk");
    let mut portability = false;
    let mut paths: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: pathchk [OPTION]... NAME...\nDiagnose invalid or unportable file names.\n -p check for most POSIX systems\n -P check for empty names and leading -\n");
                return 0;
            }
            "--version" => {
                println!("pathchk (user_utils) 0.1.0");
                return 0;
            }
            "-p" | "-P" | "--portability" => portability = true,
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => paths.push(other.to_string()),
        }
    }
    if paths.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    let mut status = 0;
    for p in &paths {
        for diag in check_path(p, portability) {
            ui.err(&diag);
            status = 1;
        }
    }
    status
}

/// Diagnose a single pathname `p`, returning one message per problem found
/// (empty, in which case no other checks run; embedded NUL; and, unless
/// portability checks apply, per-component/overall length limits; with
/// `portability`, additionally non-portable characters and the stricter
/// POSIX length limits).
fn check_path(p: &str, portability: bool) -> Vec<String> {
    let mut diags = Vec::new();
    if p.is_empty() {
        diags.push("empty pathname".to_string());
        return diags;
    }
    if p.contains('\0') {
        diags.push(format!("'{p}' has NUL"));
        return diags;
    }
    if portability {
        for comp in Path::new(p).components() {
            let s = comp.as_os_str().to_string_lossy();
            if s.len() > 14 {
                diags.push(format!("limit '{s}' longer than 14"));
            }
            if s.chars()
                .any(|c| !(c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'))
            {
                diags.push(format!("non-portable character in '{s}'"));
            }
        }
        if p.len() > 255 {
            diags.push(format!("limit '{p}' longer than 255"));
        }
    }
    for comp in Path::new(p).components() {
        let s = comp.as_os_str().to_string_lossy();
        if s.len() > 255 {
            diags.push(format!("limit '{s}' longer than 255"));
        }
    }
    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_path_has_no_diagnostics() {
        assert!(check_path("foo/bar", false).is_empty());
    }

    #[test]
    fn empty_path_is_flagged() {
        assert_eq!(check_path("", false), vec!["empty pathname".to_string()]);
    }

    #[test]
    fn embedded_nul_is_flagged() {
        let diags = check_path("foo\0bar", false);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].contains("NUL"));
    }

    #[test]
    fn overlong_component_flagged_without_portability() {
        let long = "a".repeat(300);
        let diags = check_path(&long, false);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].contains("longer than 255"));
    }

    #[test]
    fn portability_flags_component_over_14_chars() {
        let diags = check_path("this_is_definitely_too_long", true);
        assert!(diags.iter().any(|d| d.contains("longer than 14")));
    }

    #[test]
    fn portability_flags_non_portable_characters() {
        let diags = check_path("bad name!", true);
        assert!(diags.iter().any(|d| d.contains("non-portable character")));
    }

    #[test]
    fn portability_allows_dot_underscore_dash() {
        let diags = check_path("a.b_c-d", true);
        assert!(!diags.iter().any(|d| d.contains("non-portable character")));
    }

    #[test]
    fn portability_flags_overall_path_over_255() {
        let long = format!("{}/{}", "a".repeat(10), "b".repeat(250));
        let diags = check_path(&long, true);
        assert!(diags.iter().any(|d| d.contains("longer than 255")));
    }
}
