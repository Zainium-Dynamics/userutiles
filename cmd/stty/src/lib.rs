//! user stty — print/change terminal line settings (common subset).
use std::io;

use usercore::Ui;

/// Entry point for the `stty` utility. Parses `std::env::args()`, reads the
/// current terminal attributes for stdin via `tcgetattr(3)`, optionally
/// applies requested setting changes, and prints a report unless `-a`/`-g`
/// output was requested instead.
///
/// Returns 0 on success, 1 if stdin is not a terminal or an argument is
/// invalid.
pub fn run() -> i32 {
    let ui = Ui::new("stty");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("stty (user_utils) 0.1.0");
        return 0;
    }
    let fd = libc::STDIN_FILENO;
    // SAFETY: `libc::termios` is a plain C struct of integer/array
    // fields with no pointers and no bit pattern that is invalid to
    // hold, so the all-zero value produced by `mem::zeroed` is a valid
    // (if not yet meaningful) `termios` value. It is immediately
    // overwritten by `tcgetattr` below before any field is read.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: `term` is a valid, initialized (zeroed) local declared
    // just above, so `&mut term` is a valid, non-null, uniquely-owned
    // pointer of the expected type for `tcgetattr(3)` to write into.
    if unsafe { libc::tcgetattr(fd, &mut term) } != 0 {
        ui.err(&io::Error::last_os_error().to_string());
        return 1;
    }
    if args.is_empty() {
        print_brief(&term);
        return 0;
    }
    let mut all = false;
    let mut readable = false;
    let mut changed = false;
    for a in &args {
        match a.as_str() {
            "-a" | "--all" => all = true,
            "-g" => readable = true,
            "sane" => {
                apply_sane(&mut term);
                changed = true;
            }
            "raw" => {
                apply_raw(&mut term);
                changed = true;
            }
            "cooked" | "-raw" => {
                apply_cooked(&mut term);
                changed = true;
            }
            "echo" => {
                term.c_lflag |= libc::ECHO;
                changed = true;
            }
            "-echo" => {
                term.c_lflag &= !libc::ECHO;
                changed = true;
            }
            "icanon" => {
                term.c_lflag |= libc::ICANON;
                changed = true;
            }
            "-icanon" => {
                term.c_lflag &= !libc::ICANON;
                changed = true;
            }
            "isig" => {
                term.c_lflag |= libc::ISIG;
                changed = true;
            }
            "-isig" => {
                term.c_lflag &= !libc::ISIG;
                changed = true;
            }
            "opost" => {
                term.c_oflag |= libc::OPOST;
                changed = true;
            }
            "-opost" => {
                term.c_oflag &= !libc::OPOST;
                changed = true;
            }
            other if !other.is_empty() && other.bytes().all(|b| b.is_ascii_digit()) => {
                // Baud rate argument accepted for compatibility; this
                // build reports the terminal's existing speed rather
                // than reprogramming the line discipline's baud rate.
            }
            other => {
                ui.err(&format!("invalid argument '{other}'"));
                return 1;
            }
        }
    }
    if all {
        print_all(&term);
        return 0;
    }
    if readable {
        print_g(&term);
        return 0;
    }
    if changed {
        // SAFETY: `term` is a valid, fully-initialized local (populated
        // by `tcgetattr` above and only mutated through the typed
        // `apply_*`/flag-flip helpers, never via raw pointers), so
        // `&term` is a valid pointer of the expected type for
        // `tcsetattr(3)` to read from.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &term) } != 0 {
            ui.err(&io::Error::last_os_error().to_string());
            return 1;
        }
    }
    0
}

fn print_help() {
    print!(
        "Usage: stty [-a|-g] [SETTING]...\n\
Print or change terminal characteristics.\n\n\
  -a                     print all settings\n\
  -g                     print in stty-readable form\n\
  sane, raw, cooked, echo/-echo, icanon/-icanon, isig/-isig, opost/-opost\n\
      --help             display this help and exit\n\
      --version          output version information and exit\n"
    );
}

fn print_brief(t: &libc::termios) {
    println!("speed {} baud; line = 0;", speed(t));
    println!("{}", flag_summary(t).join(" "));
}

fn print_all(t: &libc::termios) {
    print_brief(t);
    println!(
        "iflag: {:#x} oflag: {:#x} cflag: {:#x} lflag: {:#x}",
        t.c_iflag, t.c_oflag, t.c_cflag, t.c_lflag
    );
}

fn print_g(t: &libc::termios) {
    // simplified g format
    println!(
        "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
        t.c_iflag,
        t.c_oflag,
        t.c_cflag,
        t.c_lflag,
        t.c_cc[libc::VINTR] as u32,
        t.c_cc[libc::VQUIT] as u32,
        t.c_cc[libc::VERASE] as u32,
        t.c_cc[libc::VKILL] as u32
    );
}

/// Read the terminal's output baud rate via `cfgetospeed(3)`.
fn speed(t: &libc::termios) -> libc::speed_t {
    // SAFETY: `t` is a valid `&libc::termios` reference (coerced to
    // `*const termios`), guaranteed non-null and properly initialized by
    // the caller; `cfgetospeed(3)` only reads from it and cannot fail.
    unsafe { libc::cfgetospeed(t) }
}

/// Summarize the local-mode flags `stty` reports in its brief (no-argument)
/// output: `echo`/`-echo`, `icanon`/`-icanon`, `isig`/`-isig`.
fn flag_summary(t: &libc::termios) -> Vec<&'static str> {
    let mut flags = Vec::with_capacity(3);
    flags.push(if t.c_lflag & libc::ECHO != 0 {
        "echo"
    } else {
        "-echo"
    });
    flags.push(if t.c_lflag & libc::ICANON != 0 {
        "icanon"
    } else {
        "-icanon"
    });
    flags.push(if t.c_lflag & libc::ISIG != 0 {
        "isig"
    } else {
        "-isig"
    });
    flags
}

fn apply_sane(t: &mut libc::termios) {
    t.c_iflag = libc::BRKINT | libc::ICRNL | libc::IMAXBEL | libc::IUTF8;
    t.c_oflag = libc::OPOST | libc::ONLCR;
    t.c_lflag = libc::ISIG | libc::ICANON | libc::ECHO | libc::ECHOE | libc::ECHOK | libc::IEXTEN;
    t.c_cflag |= libc::CREAD;
}

fn apply_raw(t: &mut libc::termios) {
    // SAFETY: `t` is a valid `&mut libc::termios` reference (coerced to
    // `*mut termios`), guaranteed non-null, properly initialized, and
    // uniquely borrowed by the caller; `cfmakeraw(3)` only mutates the
    // struct's flag fields in place and cannot fail.
    unsafe {
        libc::cfmakeraw(t);
    }
}

fn apply_cooked(t: &mut libc::termios) {
    apply_sane(t);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zeroed_termios() -> libc::termios {
        // SAFETY: `libc::termios` is a plain-old-data struct of integer
        // fields; the all-zero bit pattern is a valid (if not
        // "sane") value, and this test never passes it to a real
        // ioctl.
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn apply_sane_sets_echo_icanon_isig() {
        let mut t = zeroed_termios();
        apply_sane(&mut t);
        assert_ne!(t.c_lflag & libc::ECHO, 0);
        assert_ne!(t.c_lflag & libc::ICANON, 0);
        assert_ne!(t.c_lflag & libc::ISIG, 0);
    }

    #[test]
    fn apply_cooked_matches_sane() {
        let mut a = zeroed_termios();
        let mut b = zeroed_termios();
        apply_sane(&mut a);
        apply_cooked(&mut b);
        assert_eq!(a.c_lflag, b.c_lflag);
        assert_eq!(a.c_iflag, b.c_iflag);
        assert_eq!(a.c_oflag, b.c_oflag);
    }

    #[test]
    fn flag_summary_reflects_lflag_bits() {
        let mut t = zeroed_termios();
        assert_eq!(flag_summary(&t), vec!["-echo", "-icanon", "-isig"]);
        t.c_lflag = libc::ECHO | libc::ICANON | libc::ISIG;
        assert_eq!(flag_summary(&t), vec!["echo", "icanon", "isig"]);
    }

    #[test]
    fn apply_raw_clears_icanon_and_echo() {
        let mut t = zeroed_termios();
        apply_sane(&mut t);
        apply_raw(&mut t);
        assert_eq!(t.c_lflag & libc::ICANON, 0);
        assert_eq!(t.c_lflag & libc::ECHO, 0);
    }
}
