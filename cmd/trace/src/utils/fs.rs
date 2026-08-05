use std::fs;
use std::path::Path;
use crate::utils::TraceResult;

pub fn ensure_output_dir(path: &str) -> TraceResult<()> {
    if !Path::new(path).exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn generate_filename(process_name: &str, extension: &str) -> String {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    format!("{}_{}.{}", process_name, timestamp, extension)
}

pub fn write_output_file(directory: &str, filename: &str, content: &str) -> TraceResult<String> {
    ensure_output_dir(directory)?;
    let path = format!("{}/{}", directory, filename);
    fs::write(&path, content)?;
    Ok(path)
}
