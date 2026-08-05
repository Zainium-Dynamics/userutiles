//! user nologin — displayed as the login shell for accounts that must
//! not be able to log in interactively; always refuses and exits 1.
use std::fs;

const DEFAULT_MESSAGE: &str = "This account is currently not available.";
const NOLOGIN_TXT: &str = "/etc/nologin.txt";

const HELP: &str = "Usage: nologin [options]\n\
Politely refuse a login.\n\n\
  -c, --command <command>  does nothing (compatibility)\n\
      --init-file <file>   does nothing (compatibility)\n\
  -i, --interactive        does nothing (compatibility)\n\
  -l, --login              does nothing (compatibility)\n\
      --noprofile          does nothing (compatibility)\n\
      --norc               does nothing (compatibility)\n\
      --posix               does nothing (compatibility)\n\
      --rcfile <file>      does nothing (compatibility)\n\
  -r, --restricted         does nothing (compatibility)\n\
  -h, --help               display this help and exit\n\
      --version            output version information and exit\n";

/// Entry point for the `nologin` utility. All shell-compatibility flags
/// (`-c`, `--noprofile`, …) are accepted and ignored, since `nologin` is
/// meant to be installed as a login shell but never actually run one.
/// Prints `/etc/nologin.txt` (or a default message) and always exits 1.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            "--version" => {
                println!("nologin (user_utils) 0.1.0");
                return 0;
            }
            // Options that take a value but are otherwise ignored.
            "-c" | "--command" | "--init-file" | "--rcfile" => {
                i += 1;
            }
            // Boolean compatibility flags: silently accepted.
            "-i" | "--interactive" | "-l" | "--login" | "--noprofile" | "--norc" | "--posix"
            | "-r" | "--restricted" => {}
            _ => {}
        }
        i += 1;
    }

    println!("{}", nologin_message());
    1
}

fn nologin_message() -> String {
    fs::read_to_string(NOLOGIN_TXT)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_MESSAGE.to_string())
}
