use log::{debug, info, warn};
use std::fs;
use std::process::{Command, ExitCode, Stdio};

use crate::config::Config;
use crate::environment::Environment;
use crate::error::{Result, TriggerError};
use crate::platform::Platform;
use crate::ui::OutputFormatter;
use strsim::levenshtein;

/// Targets for the `--list` flag
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ListTarget {
    /// Discovered applications in PATH
    Apps,
    /// Detected script file handlers
    Handlers,
    /// Registered ZainiumOS services
    Services,
}

/// Type of target: either an application or a script file
#[derive(Debug, Clone)]
enum TargetType {
    /// Application/binary in PATH
    Application,
    /// Script file with associated handler
    Script { handler: String },
}

/// Run application or script based on provided arguments
pub fn run(trigger_args: &[String], dry_run: bool) -> Result<()> {
    if trigger_args.is_empty() {
        return Err(TriggerError::ExecutionFailed {
            target: "unknown".to_string(),
            reason: "No application or file specified.".to_string(),
        });
    }

    let platform = Platform::get();
    let config = Config::load()?;
    let formatter = OutputFormatter::new();

    let target = &trigger_args[0];

    let target_type = detect_target_type(target, &config, platform)?;

    match target_type {
        TargetType::Application => {
            run_application(target, trigger_args, &config, platform, &formatter, dry_run)
        }
        TargetType::Script { handler } => {
            run_script(target, &handler, trigger_args, &formatter, dry_run)
        }
    }
}

/// Detect whether target is a file or application
fn detect_target_type(target: &str, config: &Config, platform: &Platform) -> Result<TargetType> {
    debug!("Detecting target type for: {}", target);

    // Check if it's a file first (canonicalize to prevent symlink attacks)
    if let Ok(canonical) = fs::canonicalize(target) {
        if canonical.exists() {
            debug!("{} is a file", target);

            let extension = canonical
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|s| s.to_string());

            if let Some(ext) = extension {
                if let Some(handler_config) = config.get_handler(&ext) {
                    debug!("Found handler for .{}: {}", ext, handler_config.handler);
                    return Ok(TargetType::Script {
                        handler: handler_config.handler.clone(),
                    });
                }
            }

            return Err(TriggerError::FileNotFound {
                path: target.to_string(),
            });
        }
    }

    debug!("{} is not a file, checking if it's an application", target);

    // Check in discovered apps
    if config.get_app(target).is_some() {
        return Ok(TargetType::Application);
    }

    // Check in PATH
    if find_in_path(target, platform).is_some() {
        return Ok(TargetType::Application);
    }

    // Check if it's a registered ZainiumOS service
    if crate::zainium::find_service(target).is_some() {
        debug!(
            "'{}' is a Zainium service — will launch via service resolver",
            target
        );
        return Ok(TargetType::Application);
    }

    Err(TriggerError::AppNotFound {
        app: target.to_string(),
        suggestions: find_suggestions(target, config),
    })
}

/// Find binary in system PATH
fn find_in_path(target: &str, _platform: &Platform) -> Option<String> {
    for path_dir in Environment::path_dirs() {
        let candidate = path_dir.join(target);
        if candidate.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(&candidate) {
                    let mode = metadata.permissions().mode();
                    if mode & 0o111 != 0 {
                        return Some(candidate.display().to_string());
                    }
                }
            }
            #[cfg(not(unix))]
            return Some(candidate.display().to_string());
        }
    }
    None
}

/// Run an application
fn run_application(
    cmd: &str,
    trigger_args: &[String],
    config: &Config,
    platform: &Platform,
    formatter: &OutputFormatter,
    dry_run: bool,
) -> Result<()> {
    let app_config = config.get_app(cmd);
    // Use human-readable description if the user set one in config;
    // auto-discovered entries have empty description so fall back to cmd name.
    let app_name = app_config
        .and_then(|cfg| {
            if !cfg.description.is_empty() {
                Some(cfg.description.as_str())
            } else {
                None
            }
        })
        .unwrap_or(cmd);

    let binary_path = find_in_path(cmd, platform).ok_or_else(|| TriggerError::ExecutionFailed {
        target: cmd.to_string(),
        reason: "Binary not found in PATH".to_string(),
    })?;

    println!(
        "{}",
        formatter.format_launch_header(cmd, "Application", &binary_path)
    );
    println!();

    // Check privilege escalation - dynamic UID check, not hardcoded to "root"
    if platform.is_root && !trigger_args.iter().any(|arg| arg == "--no-sandbox") {
        warn!("Attempt to run application with elevated privileges");
        println!("{}", formatter.format_privilege_warning(app_name));
        println!();
        println!("  → Run without sudo: {} {}", Environment::username(), cmd);
        println!();

        return Err(TriggerError::RootExecutionForbidden {
            app: app_name.to_string(),
            command: format!("{} {}", Environment::username(), cmd),
        });
    }

    if dry_run {
        println!("{}", formatter.format_dry_run_footer());
        return Ok(());
    }

    println!(
        "{}",
        formatter.format_execution_block(
            cmd,
            &Environment::username(),
            if platform.is_root {
                "elevated"
            } else {
                "normal"
            }
        )
    );
    println!();

    // Execute application
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let mut child = Command::new(cmd)
        .args(&trigger_args[1..])
        .current_dir(&home_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                TriggerError::PermissionDenied {
                    target: cmd.to_string(),
                }
            } else {
                TriggerError::ExecutionFailed {
                    target: cmd.to_string(),
                    reason: e.to_string(),
                }
            }
        })?;

    let status = child.wait().map_err(|e| TriggerError::ExecutionFailed {
        target: cmd.to_string(),
        reason: e.to_string(),
    })?;

    if status.success() {
        info!("Application {} executed successfully", cmd);
        let pid = child.id();
        println!("{}", formatter.format_process_started(pid));
        Ok(())
    } else {
        let exit_code = status.code().unwrap_or(-1);
        warn!("Application {} exited with code {}", cmd, exit_code);
        Err(TriggerError::ExecutionFailed {
            target: cmd.to_string(),
            reason: format!("Application exited with code {}", exit_code),
        })
    }
}

/// Run a script with its handler
fn run_script(
    file_path: &str,
    handler: &str,
    trigger_args: &[String],
    formatter: &OutputFormatter,
    dry_run: bool,
) -> Result<()> {
    let canonical_path = fs::canonicalize(file_path).map_err(|_| TriggerError::FileNotFound {
        path: file_path.to_string(),
    })?;

    let _file_exists = fs::metadata(&canonical_path).map_err(|_| TriggerError::FileNotFound {
        path: file_path.to_string(),
    })?;

    // Check if file is executable (for awareness, not blocking)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = _file_exists.permissions().mode();
        if mode & 0o111 == 0 && handler != "python3" && handler != "ruby" && handler != "perl" {
            debug!("File {} is not executable, will pass to handler", file_path);
        }
    }

    let target_type = get_script_type_display(handler);
    println!(
        "{}",
        formatter.format_launch_header(
            file_path,
            &target_type,
            &canonical_path.display().to_string()
        )
    );
    println!();

    if dry_run {
        println!("{}", formatter.format_dry_run_footer());
        return Ok(());
    }

    println!(
        "{}",
        formatter.format_execution_block(
            &canonical_path.display().to_string(),
            &Environment::username(),
            "normal"
        )
    );
    println!();

    let (cmd, args) = parse_handler(handler, file_path, trigger_args);

    let mut child = Command::new(&cmd)
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| TriggerError::ExecutionFailed {
            target: file_path.to_string(),
            reason: e.to_string(),
        })?;

    let status = child.wait().map_err(|e| TriggerError::ExecutionFailed {
        target: file_path.to_string(),
        reason: e.to_string(),
    })?;

    if status.success() {
        info!("Script {} executed successfully", file_path);
        let pid = child.id();
        println!("{}", formatter.format_script_executed(pid));
        Ok(())
    } else {
        let exit_code = status.code().unwrap_or(-1);
        warn!("Script {} exited with code {}", file_path, exit_code);
        Err(TriggerError::ExecutionFailed {
            target: file_path.to_string(),
            reason: format!("Script exited with code {}", exit_code),
        })
    }
}

/// Parse handler string into command and arguments
fn parse_handler(handler: &str, file_path: &str, extra_args: &[String]) -> (String, Vec<String>) {
    let parts: Vec<&str> = handler.split_whitespace().collect();
    let cmd = parts.first().copied().unwrap_or("sh").to_string();
    let mut args: Vec<String> = parts.iter().skip(1).map(|s| s.to_string()).collect();

    args.push(file_path.to_string());
    args.extend_from_slice(&extra_args[1..]);

    (cmd, args)
}

/// Get display type for script based on handler
fn get_script_type_display(handler: &str) -> String {
    match handler {
        "sh" | "bash" | "zsh" => "Shell Script".to_string(),
        "python3" | "python" => "Python Script".to_string(),
        "node" | "nodejs" => "Node.js Script".to_string(),
        "ruby" => "Ruby Script".to_string(),
        "perl" => "Perl Script".to_string(),
        "go" => "Go Script".to_string(),
        "rustc" => "Rust Binary".to_string(),
        _ => format!("{} Script", handler),
    }
}

/// Find similar application names for suggestions
fn find_suggestions(app: &str, config: &Config) -> Vec<String> {
    let threshold = config.levenshtein_threshold;
    config
        .get_apps_list()
        .iter()
        .filter(|known_app| levenshtein(app, known_app) <= threshold)
        .map(|s| s.to_string())
        .collect()
}

// -- Public commands -------------------------------------------------------

/// Print discovered resources to stdout (--list flag)
pub fn list(target: ListTarget) -> ExitCode {
    let config = Config::load().unwrap_or_default();
    let formatter = OutputFormatter::new();

    match target {
        ListTarget::Apps => {
            let mut apps: Vec<(&str, &str)> = config
                .known_apps
                .iter()
                .map(|(name, app): (&String, &crate::config::AppConfig)| {
                    (name.as_str(), app.path.as_deref().unwrap_or(""))
                })
                .collect();
            apps.sort_by_key(|(name, _)| *name);
            println!("{}", formatter.format_list_apps(&apps));
        }
        ListTarget::Handlers => {
            let mut handlers: Vec<(&str, &str)> = config
                .file_handlers
                .iter()
                .map(
                    |(ext, handler): (&String, &crate::config::FileHandlerConfig)| {
                        (ext.as_str(), handler.description.as_str())
                    },
                )
                .collect();
            handlers.sort_by_key(|(ext, _)| *ext);
            println!("{}", formatter.format_list_handlers(&handlers));
        }
        ListTarget::Services => {
            let services = crate::zainium::list_services();
            let service_list: Vec<(&str, &str)> = services
                .iter()
                .map(|svc| (svc.name.as_str(), "Active"))
                .collect();
            println!("{}", formatter.format_list_services(&service_list));
        }
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_handler() {
        let (cmd, args) = parse_handler("python3", "script.py", &["script.py".to_string()]);
        assert_eq!(cmd, "python3");
        assert!(args.contains(&"script.py".to_string()));
    }

    #[test]
    fn test_parse_handler_with_args() {
        let (cmd, args) = parse_handler(
            "go run",
            "main.go",
            &["main.go".to_string(), "arg1".to_string()],
        );
        assert_eq!(cmd, "go");
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "main.go");
        assert_eq!(args[2], "arg1");
    }
}
