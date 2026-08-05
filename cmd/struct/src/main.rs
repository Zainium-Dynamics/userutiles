fn main() {
    let code = user_struct::run(std::env::args_os());
    std::process::exit(code);
}
