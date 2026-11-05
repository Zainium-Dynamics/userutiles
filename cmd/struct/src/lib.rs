#[cfg(not(target_os = "linux"))]
compile_error!("struct is supported only on Zainium OS targets.");

mod r#struct;

/// Run the `struct` CLI. `args` must include `argv[0]` (invocation name) first.
pub fn run<I>(args: I) -> i32
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    r#struct::run(args)
}
