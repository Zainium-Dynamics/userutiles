//! user nproc — print number of processing units.
use usercore::Ui;

/// Entry point for the `nproc` utility. Parses `std::env::args()` as
/// `[--all] [--ignore=N]` and prints the number of available processing
/// units (from `sched_getaffinity`, falling back to
/// `std::thread::available_parallelism`), minus N, floored at 1.
///
/// Returns 0 on success, 1 on a usage error.
pub fn run() -> i32 {
    let ui = Ui::new("nproc");
    let mut ignore = 0usize;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: nproc [OPTION]...\nPrint the number of processing units available.\n --all print the number of installed processors\n --ignore=N exclude N processing units\n");
                return 0;
            }
            "--version" => {
                println!("nproc (user_utils) 0.1.0");
                return 0;
            }
            "--all" => {}
            s if s.starts_with("--ignore=") => {
                let n = &s["--ignore=".len()..];
                match n.parse::<usize>() {
                    Ok(v) => ignore = v,
                    Err(_) => {
                        ui.err(&format!("invalid number '{n}'"));
                        return 1;
                    }
                }
            }
            other => {
                ui.err(&format!("invalid option -- '{other}'"));
                return 1;
            }
        }
    }
    println!("{}", available(num_cpus(), ignore));
    0
}

/// Number of usable CPUs floored at 1: `total` minus `ignore`, saturating
/// (never wrapping below 0) and never returning fewer than 1 (matching GNU
/// `nproc`, which always reports at least one processing unit).
fn available(total: usize, ignore: usize) -> usize {
    total.saturating_sub(ignore).max(1)
}

/// Number of CPUs in this process's current affinity mask (via
/// `sched_getaffinity`), falling back to
/// `std::thread::available_parallelism` if that call fails or reports zero.
fn num_cpus() -> usize {
    // SAFETY: `libc::cpu_set_t` is a fixed-size bitmask backed by an array of
    // unsigned integers with no invalid bit patterns, so the all-zero value from
    // `mem::zeroed` is a valid (empty) `cpu_set_t`. `sched_getaffinity` is called
    // with pid `0` (meaning the calling thread), a size matching
    // `size_of::<cpu_set_t>()` exactly as required by the ABI, and `&mut set`, a
    // valid pointer to that live local for the kernel to write into; the return
    // value is checked (`== 0`) before `set` is read. `CPU_ISSET` is then called
    // for each `i` in `0..CPU_SETSIZE`, which is exactly the bit range the
    // `cpu_set_t` type is sized for, so no out-of-bounds bit access occurs, and
    // `&set` is a valid pointer to the same live local.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set) == 0 {
            let count = (0..libc::CPU_SETSIZE as usize)
                .filter(|&i| libc::CPU_ISSET(i, &set))
                .count();
            if count > 0 {
                return count;
            }
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_subtracts_ignore_count() {
        assert_eq!(available(8, 2), 6);
    }

    #[test]
    fn available_floors_at_one() {
        assert_eq!(available(4, 4), 1);
        assert_eq!(available(4, 100), 1);
    }

    #[test]
    fn available_does_not_underflow_on_huge_ignore() {
        assert_eq!(available(1, usize::MAX), 1);
    }

    #[test]
    fn num_cpus_reports_at_least_one() {
        assert!(num_cpus() >= 1);
    }
}
