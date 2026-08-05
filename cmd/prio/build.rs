fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PRIO_FORCE_COLOR");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" {
        eprintln!(
            "cargo:warning=prio is designed exclusively for Zainium OS. \
 Behaviour on other targets is undefined."
        );
    }

    // Embed build date as env var accessible at runtime via env!()
    let build_date = {
        let output = std::process::Command::new("date").arg("+%Y-%m-%d").output();
        match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => "unknown".to_string(),
        }
    };
    println!("cargo:rustc-env=PRIO_BUILD_DATE={}", build_date);

    // Expose target triple for version output
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=PRIO_TARGET={}", target);
}
