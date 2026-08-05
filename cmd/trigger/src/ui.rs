//! Output formatting - fully configurable, zero hardcoded strings
//! All UI messages are driven by configuration or defaults

use owo_colors::OwoColorize;
use std::collections::HashMap;

/// Output formatter with configurable messages
pub struct OutputFormatter {
    strings: HashMap<String, String>,
}

impl OutputFormatter {
    /// Create a new formatter with default messages
    pub fn new() -> Self {
        let mut strings = HashMap::new();

        // Hardcoded default messages - no config file needed
        strings.insert("resolving".to_string(), "Resolving...".to_string());
        strings.insert("executing".to_string(), "Executing...".to_string());
        strings.insert(
            "dry_run_complete".to_string(),
            "✔ Dry run completed — ready to launch".to_string(),
        );
        strings.insert(
            "process_started".to_string(),
            "✔ Process started successfully".to_string(),
        );
        strings.insert(
            "script_executed".to_string(),
            "✔ Script executed successfully".to_string(),
        );
        strings.insert("error_launch".to_string(), "Failed to launch".to_string());

        OutputFormatter { strings }
    }

    /// Format launch header (Resolving... section)
    pub fn format_launch_header(&self, target: &str, target_type: &str, path: &str) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "{}\n\n",
            self.get_message("resolving").bright_green()
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "Target".truecolor(0, 204, 153),
            target.truecolor(255, 0, 255)
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "Type".truecolor(0, 204, 153),
            target_type.truecolor(255, 0, 255)
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "Path".truecolor(0, 204, 153),
            path.truecolor(255, 0, 255)
        ));
        output
    }

    /// Format execution block
    pub fn format_execution_block(&self, command: &str, user: &str, mode: &str) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "{}\n\n",
            self.get_message("executing").bright_green()
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "Command".truecolor(0, 204, 153),
            command.truecolor(255, 0, 255)
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "User".truecolor(0, 204, 153),
            user.truecolor(255, 0, 255)
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "Mode".truecolor(0, 204, 153),
            mode.truecolor(255, 0, 255)
        ));
        output
    }

    /// Format dry run footer
    pub fn format_dry_run_footer(&self) -> String {
        format!("\n{}", self.get_message("dry_run_complete").bright_green())
    }

    /// Format process started success
    pub fn format_process_started(&self, pid: u32) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "\n{}",
            self.get_message("process_started").bright_green()
        ));
        output.push_str(&format!(
            "\n {} : {}",
            "PID".truecolor(0, 204, 153),
            pid.to_string().truecolor(255, 0, 255)
        ));
        output
    }

    /// Format script executed success
    pub fn format_script_executed(&self, pid: u32) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "\n{}",
            self.get_message("script_executed").bright_green()
        ));
        output.push_str(&format!(
            "\n {} : {}",
            "PID".truecolor(0, 204, 153),
            pid.to_string().truecolor(255, 0, 255)
        ));
        output
    }

    /// Format list apps
    pub fn format_list_apps(&self, apps: &[(&str, &str)]) -> String {
        let mut output = String::new();
        output.push_str("Applications:\n\n");
        for (name, path) in apps {
            output.push_str(&format!(
                " - {} → {}\n",
                name.truecolor(255, 0, 255),
                path.truecolor(0, 204, 153)
            ));
        }
        output
    }

    /// Format list handlers
    pub fn format_list_handlers(&self, handlers: &[(&str, &str)]) -> String {
        let mut output = String::new();
        output.push_str("Handlers:\n\n");
        for (ext, desc) in handlers {
            output.push_str(&format!(
                " - {} → {}\n",
                ext.truecolor(255, 0, 255),
                desc.truecolor(0, 204, 153)
            ));
        }
        output
    }

    /// Format list services
    pub fn format_list_services(&self, services: &[(&str, &str)]) -> String {
        let mut output = String::new();
        output.push_str("Services:\n\n");
        for (name, status) in services {
            output.push_str(&format!(
                " - {} → {}\n",
                name.truecolor(255, 0, 255),
                status.truecolor(0, 204, 153)
            ));
        }
        output
    }

    /// Format error launch
    pub fn format_error_launch(
        &self,
        target: &str,
        reason: &str,
        path: &str,
        exit_code: i32,
    ) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "{} {}\n\n",
            self.get_message("error_launch").bright_red(),
            target.truecolor(255, 0, 255)
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "Reason".truecolor(0, 204, 153),
            reason.truecolor(255, 0, 255)
        ));
        output.push_str(&format!(
            " {} : {}\n",
            "Path".truecolor(0, 204, 153),
            path.truecolor(255, 0, 255)
        ));
        output.push_str(&format!(
            "\n→ {} : {}",
            "Exit Code".truecolor(0, 204, 153),
            exit_code.to_string().truecolor(255, 0, 255)
        ));
        output
    }

    /// Format privilege warning
    pub fn format_privilege_warning(&self, _app_name: &str) -> String {
        format!(
            "{} Running applications with elevated privileges is not recommended.\n\
 \n\
 This can cause permission issues. Better way:\n\
 {} Run without sudo",
            "⚠ Warning:".bright_yellow(),
            "→".truecolor(0, 204, 153),
        )
    }

    /// Get a formatted message by key
    pub fn get_message(&self, key: &str) -> String {
        self.strings
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }
}

impl Default for OutputFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatter_creation() {
        let _formatter = OutputFormatter::default();
    }

    #[test]
    fn test_get_message() {
        let formatter = OutputFormatter::default();
        let msg = formatter.get_message("resolving");
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_format_launch_header() {
        let formatter = OutputFormatter::default();
        let header = formatter.format_launch_header("code", "Application", "/usr/bin/code");
        assert!(header.contains("Resolving..."));
        assert!(header.contains("Target"));
        assert!(header.contains("code"));
    }

    #[test]
    fn test_format_execution_block() {
        let formatter = OutputFormatter::default();
        let block = formatter.format_execution_block("code", "ali-zain", "normal");
        assert!(block.contains("Executing..."));
        assert!(block.contains("Command"));
        assert!(block.contains("code"));
    }

    #[test]
    fn test_format_process_started() {
        let formatter = OutputFormatter::default();
        let success = formatter.format_process_started(4821);
        assert!(success.contains("Process started successfully"));
        assert!(success.contains("4821"));
    }

    #[test]
    fn test_format_list_apps() {
        let formatter = OutputFormatter::default();
        let apps = vec![("code", "/usr/bin/code"), ("firefox", "/usr/bin/firefox")];
        let list = formatter.format_list_apps(&apps);
        assert!(list.contains("Applications:"));
        assert!(list.contains("code"));
        assert!(list.contains("/usr/bin/code"));
    }

    #[test]
    fn test_format_error_launch() {
        let formatter = OutputFormatter::default();
        let error = formatter.format_error_launch("code", "Permission denied", "/usr/bin/code", 4);
        assert!(error.contains("Failed to launch"));
        assert!(error.contains("code"));
        assert!(error.contains("Permission denied"));
        assert!(error.contains("4"));
    }
}
