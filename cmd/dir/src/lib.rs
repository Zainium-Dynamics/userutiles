//! user dir: list directory contents (ls -C style).

/// Entry point for the `dir` utility. Forwards the process argv (minus
/// `argv[0]`) to `user_ls::run_args`, which implements all listing,
/// formatting, and coloring logic.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    user_ls::run_args(&args)
}

#[cfg(test)]
mod tests {
    #[test]
    fn help_flag_returns_success() {
        assert_eq!(user_ls::run_args(&["--help".to_string()]), 0);
    }
}
