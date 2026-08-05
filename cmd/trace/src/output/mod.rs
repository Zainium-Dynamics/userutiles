pub mod table;
pub mod toml_out;

use crate::tracer::TraceData;
use crate::utils::TraceResult;

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Table,
    Toml,
}

impl OutputFormat {
    pub fn format(&self, data: &TraceData) -> TraceResult<String> {
        match self {
            OutputFormat::Table => table::format_table(data),
            OutputFormat::Toml => toml_out::format_toml(data),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Table => "txt",
            OutputFormat::Toml => "toml",
        }
    }
}
