//! user true
pub fn run() -> i32 {
    let mut args = std::env::args_os().skip(1);
    if let Some(a) = args.next() {
        match a.to_string_lossy().as_ref() {
            "--help" => {
                print!("Usage: true [ignored command line arguments]\nor: true OPTION\nExit with a status code indicating success.\n\n --help display this help and exit\n --version output version information and exit\n");
            }
            "--version" => println!("true (user_utils) 0.1.0"),
            _ => {}
        }
    }
    0
}
