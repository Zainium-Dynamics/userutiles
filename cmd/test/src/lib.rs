//! user_test_cmd — the POSIX `test` / `[` builtin.
mod logic;

/// Entry point for the `test`/`[` utility. Detects which name it was
/// invoked as (`test` or `[`) from `argv[0]`, and for `[` requires and
/// strips a trailing `]` operand before evaluating the expression.
///
/// Returns 0 (true), 1 (false), or 2 (usage/evaluation error).
pub fn run() -> i32 {
    let mut args: Vec<String> = std::env::args().collect();
    let prog = args.first().map(|s| s.as_str()).unwrap_or("test");
    let is_bracket = prog.ends_with('[') || prog == "[";
    if !args.is_empty() {
        args.remove(0);
    }
    if is_bracket {
        if args.last().map(|s| s.as_str()) != Some("]") {
            eprintln!("[: missing `]'");
            return 2;
        }
        args.pop();
    }
    logic::eval(&args, is_bracket)
}

#[cfg(test)]
mod tests {
    // `run()` reads `std::env::args()` directly and cannot be driven
    // hermetically per-case; `logic::eval` (exercised in `logic.rs`'s own
    // tests) covers the evaluation semantics. This just checks the crate
    // wires up without panicking under the process's real argv.
    #[test]
    fn run_does_not_panic() {
        let _ = super::run();
    }
}
