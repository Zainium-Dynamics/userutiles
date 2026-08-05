
use log::LevelFilter;
use std::io::Write;

pub fn init_logger(verbose: bool) {
    let level = if verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    env_logger::Builder::new()
        .filter_level(level)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] {}",
                record.level(),
                record.args()
            )
        })
        .init();
}
