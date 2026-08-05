//! user factor — print prime factors.
use std::io::{self, BufRead};

use usercore::Ui;

/// Entry point for the `factor` utility. Parses `std::env::args()` as a
/// list of `NUMBER`s (falling back to reading whitespace-separated numbers
/// from stdin if none are given) and prints each number's prime
/// factorization as `NUMBER: f1 f2 ...`.
///
/// Returns 0 on success, 1 on a usage or I/O error.
pub fn run() -> i32 {
    let ui = Ui::new("factor");
    let mut nums: Vec<u64> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("Usage: factor [NUMBER]...\nPrint the prime factors of each NUMBER.\n");
                return 0;
            }
            "--version" => {
                println!("factor (user_utils) 0.1.0");
                return 0;
            }
            s if s.starts_with('-') => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => match other.parse::<u64>() {
                Ok(n) => nums.push(n),
                Err(_) => {
                    ui.err(&format!("'{other}' is not a valid positive integer"));
                    return 1;
                }
            },
        }
    }
    if nums.is_empty() {
        for line in io::stdin().lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    ui.err(&format!("{e}"));
                    return 1;
                }
            };
            for tok in line.split_whitespace() {
                match tok.parse::<u64>() {
                    Ok(n) => nums.push(n),
                    Err(_) => {
                        ui.err(&format!("'{tok}' is not a valid positive integer"));
                        return 1;
                    }
                }
            }
        }
    }
    for n in nums {
        let f = factors(n);
        print!("{n}:");
        for x in f {
            print!(" {x}");
        }
        println!();
    }
    0
}

/// Return the prime factorization of `n` in ascending order (repeated
/// primes appear once per multiplicity), e.g. `factors(12) == [2, 2, 3]`.
/// `factors(0)` and `factors(1)` both return an empty list.
///
/// Trial division uses `checked_mul` to detect when the candidate factor
/// `f` would overflow `f * f`: at that point `f` already exceeds
/// `sqrt(u64::MAX) >= sqrt(n)`, so any remaining `n > 1` must itself be
/// prime and is pushed directly, without ever computing an overflowing
/// product.
fn factors(mut n: u64) -> Vec<u64> {
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    while n % 2 == 0 {
        out.push(2);
        n /= 2;
    }
    let mut f = 3u64;
    while let Some(sq) = f.checked_mul(f) {
        if sq > n {
            break;
        }
        while n % f == 0 {
            out.push(f);
            n /= f;
        }
        f += 2;
    }
    if n > 1 {
        out.push(n);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factors_of_zero_and_one_are_empty() {
        assert_eq!(factors(0), Vec::<u64>::new());
        assert_eq!(factors(1), Vec::<u64>::new());
    }

    #[test]
    fn factors_of_prime_is_itself() {
        assert_eq!(factors(13), vec![13]);
    }

    #[test]
    fn factors_of_composite() {
        assert_eq!(factors(12), vec![2, 2, 3]);
        assert_eq!(factors(360), vec![2, 2, 2, 3, 3, 5]);
    }

    #[test]
    fn factors_of_power_of_two() {
        assert_eq!(factors(1024), vec![2; 10]);
    }

    #[test]
    #[ignore = "exercises the full trial-division range up to ~2^32 (~2B \
                loop iterations); slow (tens of seconds) even in release \
                builds, so it's excluded from the default `cargo test` run \
                — run explicitly with `cargo test -- --ignored` after \
                touching the trial-division loop in `factors`"]
    fn factors_near_u64_max_does_not_panic() {
        // A large prime close to u64::MAX exercises the f*f overflow path
        // in the trial-division loop; this must not panic or hang.
        let big_prime = u64::MAX - 58; // known prime
        assert_eq!(factors(big_prime), vec![big_prime]);
    }

    #[test]
    fn factors_of_u64_max_does_not_panic() {
        // u64::MAX = 3 * 5 * 17 * 257 * 641 * 65537 * 6700417
        let result = factors(u64::MAX);
        assert_eq!(result.iter().product::<u64>(), u64::MAX);
        assert!(result.windows(2).all(|w| w[0] <= w[1]));
    }
}
