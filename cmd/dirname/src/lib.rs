//! user dirname — strip last component from file name.
use std::path::Path;

use usercore::Ui;

/// Entry point for the `dirname` utility. Parses `std::env::args()` and
/// prints the directory portion of each `NAME` operand (everything before
/// the last `/`), one per line (or NUL-terminated with `-z`).
///
/// Returns 0 on success, 1 on a usage error (missing operand or unknown
/// flag).
pub fn run() -> i32 {
    let ui = Ui::new("dirname");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        ui.err("missing operand");
        return 1;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("Usage: dirname [OPTION] NAME...\n -z, --zero end each output line with NUL\n");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("dirname (user_utils) 0.1.0");
        return 0;
    }
    let mut zero = false;
    let mut names = Vec::new();
    for a in args {
        match a.as_str() {
            "-z" | "--zero" => zero = true,
            s if s.starts_with('-') && s != "-" => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => names.push(other.to_string()),
        }
    }
    let end = if zero { "\0" } else { "\n" };
    for n in &names {
        print!("{}{end}", dirname_of(n));
    }
    0
}

/// Compute the `dirname` of a single path string: everything before the
/// last `/`, or `.` if there is no directory component. `/` itself maps
/// to `/` (its own parent, per POSIX `dirname` semantics), which
/// `Path::parent()` alone would report as `None`/empty.
fn dirname_of(name: &str) -> String {
    if name == "/" {
        return "/".to_string();
    }
    Path::new(name)
        .parent()
        .map(|p| {
            let s = p.to_string_lossy();
            if s.is_empty() {
                ".".to_string()
            } else {
                s.into_owned()
            }
        })
        .unwrap_or_else(|| ".".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_last_component() {
        assert_eq!(dirname_of("/usr/bin/sort"), "/usr/bin");
        assert_eq!(dirname_of("dir1/str"), "dir1");
    }

    #[test]
    fn no_directory_component_yields_dot() {
        assert_eq!(dirname_of("stdio.h"), ".");
    }

    #[test]
    fn root_maps_to_itself() {
        assert_eq!(dirname_of("/"), "/");
    }

    #[test]
    fn trailing_slash_is_stripped_before_taking_parent() {
        assert_eq!(dirname_of("/usr/bin/"), "/usr");
    }

    #[test]
    fn single_component_relative_path() {
        assert_eq!(dirname_of("usr"), ".");
    }
}
