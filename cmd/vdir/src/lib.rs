//! user vdir — long listing (`ls -l`).
//!
//! `vdir` is a thin historical alias: it behaves exactly like `ls -l`, just
//! under a different invocation name. All listing logic lives in `user_ls`.

/// Entry point for the `vdir` utility. Forwards `std::env::args()` (minus
/// argv0) to [`user_ls::run_args`] with `-l` prepended, so `vdir FOO` behaves
/// exactly like `ls -l FOO`.
pub fn run() -> i32 {
    user_ls::run_args(&build_args(std::env::args().skip(1)))
}

/// Build the argument vector passed to `user_ls::run_args`: always `-l`
/// first, followed by the caller's own arguments in order.
fn build_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = vec!["-l".to_string()];
    out.extend(args);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_args_prepends_long_flag() {
        assert_eq!(build_args(std::iter::empty()), vec!["-l".to_string()]);
    }

    #[test]
    fn build_args_keeps_caller_args_in_order() {
        let args = vec!["-a".to_string(), "/tmp".to_string()];
        assert_eq!(
            build_args(args),
            vec!["-l".to_string(), "-a".to_string(), "/tmp".to_string()]
        );
    }
}
