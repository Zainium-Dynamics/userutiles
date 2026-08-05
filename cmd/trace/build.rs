fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not set");

    // Rustc uses target_os=linux for Zainium OS kernel ABI builds.
    if target_os != "linux" {
        panic!("trace targets Zainium OS only. Target OS: {target_os}");
    }

    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");

    match target_arch.as_str() {
        "x86_64" | "aarch64" | "arm" => {
            println!("cargo:rustc-env=TRACE_ARCH={target_arch}");
        }
        _ => {
            eprintln!("Warning: trace is primarily tested on x86_64, aarch64, and arm");
            eprintln!("Current target: {target_arch}");
        }
    }

    println!("cargo:rustc-env=TRACE_BUILD_TIME={}", chrono_build_time());
}

fn chrono_build_time() -> String {
    use std::time::SystemTime;
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "unknown".to_string(),
    }
}
