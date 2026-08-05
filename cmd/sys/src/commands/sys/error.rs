use crate::theme::{self, Tone};

#[allow(dead_code)]
pub fn print_error(msg: &str) {
    eprintln!("  {} {}", theme::paint(Tone::Red, "error:"), msg);
}
