fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(e) = blueprint::run(args) {
        eprintln!("blueprint: {e}");
        std::process::exit(1);
    }
}
