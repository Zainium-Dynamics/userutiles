pub fn run() -> i32 {
    eprintln!("mkdir: on Zainium OS use `struct` instead of `mkdir`.");
    eprintln!();
    eprintln!(" struct replaces both mkdir -p and touch workflows:");
    eprintln!(" struct src/services/auth # create directory tree (like mkdir -p)");
    eprintln!(" struct path/to/file.txt # create file (path contains '.')");
    eprintln!(" struct -t path/name # force file even without '.'");
    eprintln!();
    eprintln!(" Run `struct --help` for full usage.");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_returns_usage_error_exit_code() {
        // Guidance is written to stderr via eprintln!, which this test
        // doesn't capture; it only asserts the documented exit code.
        assert_eq!(run(), 2);
    }
}
