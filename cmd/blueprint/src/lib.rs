//! blueprint
//!
//! Generate a directory tree blueprint for a target project. The output file is named
//! `{project}_blueprint.txt`, with old blueprints archived as `{project}_blueprint-1.txt`,
//! `{project}_blueprint-2.txt`, etc.

use anyhow::{anyhow, Result};
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "blueprint",
    about = "Generate project blueprint",
    long_about = "Generate project blueprint for directory tree. Archives old blueprints automatically."
)]
struct Args {
    #[arg(help = "Target directory to blueprint")]
    path: Option<String>,
}

/// Entry point for the `blueprint` utility. Parses `args` (including
/// `argv[0]`) with `clap`, walks the target directory tree (`.` by
/// default), and writes a `{project}_blueprint.txt` file describing it —
/// archiving any previous blueprint as `{project}_blueprint-N.txt` first.
///
/// Returns an error if the target path doesn't exist, the output
/// directory isn't writable, or any I/O step (rename/write/walk) fails.
pub fn run(args: Vec<String>) -> Result<()> {
    // Use `parse_from` (not `try_parse_from`) so `--help` / `--version` match the
    // standard clap exit path (process exit 0), same as a standalone binary.
    let cli = Args::parse_from(args);
    let target_path_str = cli.path.as_deref().unwrap_or(".");
    let target_path = Path::new(target_path_str);

    if !target_path.exists() {
        return Err(anyhow!("Path does not exist: {}", target_path.display()));
    }

    let base_name = get_base_name(target_path)?;
    let out_dir = std::env::current_dir()?;

    // Check write permissions on output directory
    if !is_writable(&out_dir)? {
        return Err(anyhow!(
            "No write permission in output directory: {}",
            out_dir.display()
        ));
    }

    println!("→ Generating project blueprint...");

    if target_path_str == "." {
        println!("→ Analyzing project structure...");
    } else {
        let p = target_path_str.trim_end_matches(['/', '\\']);
        println!("→ Analyzing path: {}/", p);
    }

    let plan = plan_blueprint_write(&out_dir, &base_name)?;

    if let Some(ref prev) = plan.previous_found_basename {
        println!("→ Previous blueprint found: {}", prev);
    }

    if let Some((ref from, ref to)) = plan.archive_rename {
        fs::rename(from, to)?;
    }

    let tree_root_label = tree_root_label(target_path, &base_name)?;
    let (tree_output, stats) = generate_tree(target_path, &tree_root_label, &base_name)?;

    fs::write(&plan.write_path, &tree_output)?;

    println!("{}", tree_output);
    println!("Blueprint generated successfully.");
    println!("  Total directories : {}", stats.directories);
    println!("  Total files : {}", stats.files);
    if stats.workspace_crates > 0 {
        println!("  Workspace crates : {}", stats.workspace_crates);
    }

    println!("Ready for development.");

    match (&plan.archive_rename, &plan.archived_basename) {
        (Some(_), Some(arch)) => {
            println!("Previous blueprint archived as: {}", arch);
            println!(
                "New blueprint saved as: {}",
                plan.write_path.file_name().unwrap().to_string_lossy()
            );
        }
        _ => {
            println!(
                "Project blueprint saved as: {}",
                plan.write_path.file_name().unwrap().to_string_lossy()
            );
        }
    }

    println!("\n✔ Blueprint generation complete.");

    Ok(())
}

/// Probe whether `dir` is writable by creating and immediately removing a
/// sentinel file in it (there's no portable `access(2)`-free way to check
/// this without racing an actual write attempt).
fn is_writable(dir: &Path) -> Result<bool> {
    let test_file = dir.join(".blueprint_write_test");
    match fs::File::create(&test_file) {
        Ok(_) => {
            let _ = fs::remove_file(&test_file); // Clean up
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// Label used for the root line of the printed tree: `base_name` when
/// blueprinting the current directory (`.`), otherwise the target path's
/// own final component.
fn tree_root_label(path: &Path, base_name: &str) -> Result<String> {
    if path == Path::new(".") {
        Ok(base_name.to_string())
    } else {
        Ok(path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()))
    }
}

/// Project name used for the blueprint filename: the current directory's
/// name when `path` is `.`, otherwise `path`'s own final component.
fn get_base_name(path: &Path) -> Result<String> {
    if path == Path::new(".") {
        let cwd = std::env::current_dir()?;
        Ok(cwd
            .file_name()
            .ok_or_else(|| anyhow!("Unable to get current directory name"))?
            .to_string_lossy()
            .to_string())
    } else {
        Ok(path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path"))?
            .to_string_lossy()
            .to_string())
    }
}

struct BlueprintPlan {
    write_path: PathBuf,
    /// Basename of the file being superseded (for the "found" line).
    previous_found_basename: Option<String>,
    /// Rename old file to this path.
    archive_rename: Option<(PathBuf, PathBuf)>,
    /// Basename of archived file for console (e.g. user_blueprint-1.txt).
    archived_basename: Option<String>,
}

/// Decide where the new blueprint should be written and, if a previous
/// one exists, how to archive it. `{base}_blueprint.txt` is always
/// preferred for a fresh write; if it (or any numbered
/// `{base}_blueprint-N.txt`) already exists, the highest-numbered
/// existing file is archived to the next number and the new file gets
/// the number after that.
fn plan_blueprint_write(out_dir: &Path, base_name: &str) -> Result<BlueprintPlan> {
    let plain = out_dir.join(format!("{base_name}_blueprint.txt"));
    let max_num = max_numbered_blueprint(out_dir, base_name)?;

    let has_plain = plain.is_file();
    if !has_plain && max_num == 0 {
        return Ok(BlueprintPlan {
            write_path: plain,
            previous_found_basename: None,
            archive_rename: None,
            archived_basename: None,
        });
    }

    if has_plain {
        let archived_num = max_num + 1;
        let new_num = max_num + 2;
        let archived_path = out_dir.join(format!("{base_name}_blueprint-{archived_num}.txt"));
        let write_path = out_dir.join(format!("{base_name}_blueprint-{new_num}.txt"));
        return Ok(BlueprintPlan {
            write_path,
            previous_found_basename: Some(
                plain.file_name().unwrap().to_string_lossy().into_owned(),
            ),
            archive_rename: Some((plain, archived_path.clone())),
            archived_basename: Some(
                archived_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
        });
    }

    // Latest is {base}_blueprint-{max_num}.txt
    let prev_path = out_dir.join(format!("{base_name}_blueprint-{max_num}.txt"));
    let archived_num = max_num + 1;
    let new_num = max_num + 2;
    let archived_path = out_dir.join(format!("{base_name}_blueprint-{archived_num}.txt"));
    let write_path = out_dir.join(format!("{base_name}_blueprint-{new_num}.txt"));

    Ok(BlueprintPlan {
        write_path,
        previous_found_basename: Some(
            prev_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
        archive_rename: Some((prev_path, archived_path.clone())),
        archived_basename: Some(
            archived_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
    })
}

/// Highest `N` among existing `{base_name}_blueprint-N.txt` files in
/// `out_dir` (0 if none exist).
fn max_numbered_blueprint(out_dir: &Path, base_name: &str) -> Result<usize> {
    let prefix = format!("{base_name}_blueprint-");
    let max_num = fs::read_dir(out_dir)?
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_prefix(&prefix)?
                .strip_suffix(".txt")?
                .parse::<usize>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    Ok(max_num)
}

#[derive(Default)]
struct Stats {
    directories: usize,
    files: usize,
    workspace_crates: usize,
}

/// Render the full tree under `root` (labelled `root_label`) as text,
/// alongside directory/file/crate counts. `blueprint_base` is used to
/// skip the tool's own previous output files (see `should_ignore`).
fn generate_tree(root: &Path, root_label: &str, blueprint_base: &str) -> Result<(String, Stats)> {
    let mut output = String::new();
    let mut stats = Stats::default();
    output.push_str("Project Tree\n\n");
    output.push_str(&format!("{}/\n", root_label));
    walk_dir(root, &mut output, &mut stats, "", true, blueprint_base, 0)?;
    Ok((output, stats))
}

/// Recursively append one tree level (children of `dir`) to `output` as
/// `├--`/`└--` connector lines, updating `stats` as it goes. Bounded by
/// `MAX_DEPTH` as a backstop against pathologically deep trees.
fn walk_dir(
    dir: &Path,
    output: &mut String,
    stats: &mut Stats,
    prefix: &str,
    root_level_spacers: bool,
    blueprint_base: &str,
    depth: usize,
) -> Result<()> {
    const MAX_DEPTH: usize = 100; // Prevent infinite recursion in pathological cases
    if depth > MAX_DEPTH {
        return Err(anyhow!(
            "Directory depth exceeds maximum limit of {}",
            MAX_DEPTH
        ));
    }
    let mut visible: Vec<_> = WalkDir::new(dir)
        .min_depth(1)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !should_ignore(e.path(), blueprint_base))
        .collect();

    // Sort by `entry.file_type()` (from `follow_links(false)`, so it
    // reflects `symlink_metadata`, not the symlink's target) rather than
    // `path.is_dir()` (a `stat` that follows symlinks). This keeps the
    // ordering consistent with the recursion decision below.
    visible.sort_by(|a, b| {
        let da = a.file_type().is_dir();
        let db = b.file_type().is_dir();
        da.cmp(&db).then_with(|| {
            a.file_name()
                .to_string_lossy()
                .cmp(&b.file_name().to_string_lossy())
        })
    });

    let total = visible.len();
    for (i, entry) in visible.iter().enumerate() {
        let path = entry.path();
        // `entry.file_type()` (not `path.is_dir()`): the latter follows
        // symlinks, so a directory-symlink would be treated as a real
        // subdirectory and recursed into below — including a
        // self-referential symlink (`dir/self -> dir`), which would
        // recurse forever. `file_type()` reports the symlink's own type
        // (since `follow_links(false)` above), so symlinks are always
        // rendered as leaves here, never walked into.
        let is_dir = entry.file_type().is_dir();

        if root_level_spacers && is_dir && i > 0 {
            output.push_str(&format!("{}│\n", prefix));
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let is_last = i == total - 1;
        let connector = if is_last { "└-- " } else { "├-- " };
        let line_name = if is_dir {
            format!("{}/", name)
        } else {
            name.clone()
        };
        output.push_str(&format!("{}{}{}\n", prefix, connector, line_name));

        if is_dir {
            stats.directories += 1;
            let new_prefix = format!("{}{}", prefix, if is_last { " " } else { "│ " });
            walk_dir(
                path,
                output,
                stats,
                &new_prefix,
                false,
                blueprint_base,
                depth + 1,
            )?;
        } else {
            stats.files += 1;
            if name == "Cargo.toml" {
                stats.workspace_crates += 1;
            }
        }
    }
    Ok(())
}

/// True if `path` should be omitted from the rendered tree: dotfiles,
/// `target`/`node_modules` build-artifact directories, and this tool's
/// own current or archived blueprint output for `blueprint_base`.
fn should_ignore(path: &Path, blueprint_base: &str) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy();
    if name.starts_with('.') || name == "target" || name == "node_modules" {
        return true;
    }
    let plain = format!("{blueprint_base}_blueprint.txt");
    if name.as_ref() == plain.as_str() {
        return true;
    }
    let numbered_prefix = format!("{blueprint_base}_blueprint-");
    name.ends_with(".txt") && name.starts_with(numbered_prefix.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh, empty scratch directory under the system temp dir, unique
    /// per call (so parallel `cargo test` runs don't collide).
    fn scratch_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "user_blueprint_test_{}_{}_{}",
            std::process::id(),
            tag,
            n
        ));
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn should_ignore_dotfiles_and_build_dirs() {
        assert!(should_ignore(Path::new(".git"), "proj"));
        assert!(should_ignore(Path::new("target"), "proj"));
        assert!(should_ignore(Path::new("node_modules"), "proj"));
        assert!(!should_ignore(Path::new("src"), "proj"));
    }

    #[test]
    fn should_ignore_own_blueprint_outputs() {
        assert!(should_ignore(Path::new("proj_blueprint.txt"), "proj"));
        assert!(should_ignore(Path::new("proj_blueprint-3.txt"), "proj"));
        // Different project's blueprint file is not ours to ignore.
        assert!(!should_ignore(Path::new("other_blueprint.txt"), "proj"));
        assert!(!should_ignore(Path::new("readme.txt"), "proj"));
    }

    #[test]
    fn tree_root_label_dot_uses_base_name() {
        assert_eq!(tree_root_label(Path::new("."), "myproj").unwrap(), "myproj");
    }

    #[test]
    fn tree_root_label_explicit_path_uses_final_component() {
        assert_eq!(
            tree_root_label(Path::new("/some/nested/dir"), "myproj").unwrap(),
            "dir"
        );
    }

    #[test]
    fn max_numbered_blueprint_empty_dir_is_zero() {
        let dir = scratch_dir("max_empty");
        assert_eq!(max_numbered_blueprint(&dir, "proj").unwrap(), 0);
    }

    #[test]
    fn max_numbered_blueprint_picks_highest_number() {
        let dir = scratch_dir("max_pick");
        fs::write(dir.join("proj_blueprint-1.txt"), "").unwrap();
        fs::write(dir.join("proj_blueprint-7.txt"), "").unwrap();
        fs::write(dir.join("proj_blueprint-3.txt"), "").unwrap();
        // Different base name / non-matching suffix must not count.
        fs::write(dir.join("other_blueprint-99.txt"), "").unwrap();
        fs::write(dir.join("proj_blueprint-notanumber.txt"), "").unwrap();
        assert_eq!(max_numbered_blueprint(&dir, "proj").unwrap(), 7);
    }

    #[test]
    fn plan_blueprint_write_fresh_directory_writes_plain_file() {
        let dir = scratch_dir("plan_fresh");
        let plan = plan_blueprint_write(&dir, "proj").unwrap();
        assert_eq!(plan.write_path, dir.join("proj_blueprint.txt"));
        assert!(plan.archive_rename.is_none());
        assert!(plan.previous_found_basename.is_none());
    }

    #[test]
    fn plan_blueprint_write_archives_existing_plain_file() {
        let dir = scratch_dir("plan_archive_plain");
        fs::write(dir.join("proj_blueprint.txt"), "old").unwrap();
        let plan = plan_blueprint_write(&dir, "proj").unwrap();
        assert_eq!(plan.write_path, dir.join("proj_blueprint-2.txt"));
        let (from, to) = plan.archive_rename.unwrap();
        assert_eq!(from, dir.join("proj_blueprint.txt"));
        assert_eq!(to, dir.join("proj_blueprint-1.txt"));
    }

    #[test]
    fn plan_blueprint_write_archives_highest_numbered_file() {
        let dir = scratch_dir("plan_archive_numbered");
        fs::write(dir.join("proj_blueprint-1.txt"), "old").unwrap();
        fs::write(dir.join("proj_blueprint-2.txt"), "older").unwrap();
        let plan = plan_blueprint_write(&dir, "proj").unwrap();
        // Latest existing is -2, so it archives to -3 and writes -4.
        let (from, to) = plan.archive_rename.unwrap();
        assert_eq!(from, dir.join("proj_blueprint-2.txt"));
        assert_eq!(to, dir.join("proj_blueprint-3.txt"));
        assert_eq!(plan.write_path, dir.join("proj_blueprint-4.txt"));
    }

    #[test]
    fn generate_tree_counts_files_and_directories() {
        let dir = scratch_dir("gen_tree");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.txt"), "").unwrap();
        fs::write(dir.join("sub").join("Cargo.toml"), "").unwrap();

        let (output, stats) = generate_tree(&dir, "root", "proj").unwrap();
        assert_eq!(stats.directories, 1);
        assert_eq!(stats.files, 2);
        assert_eq!(stats.workspace_crates, 1);
        assert!(output.contains("a.txt"));
        assert!(output.contains("sub/"));
        assert!(output.contains("Cargo.toml"));
    }

    #[test]
    fn generate_tree_ignores_own_blueprint_output() {
        let dir = scratch_dir("gen_tree_ignore");
        fs::write(dir.join("proj_blueprint.txt"), "").unwrap();
        fs::write(dir.join("real.txt"), "").unwrap();

        let (output, stats) = generate_tree(&dir, "root", "proj").unwrap();
        assert_eq!(stats.files, 1);
        assert!(!output.contains("proj_blueprint.txt"));
        assert!(output.contains("real.txt"));
    }

    #[test]
    fn walk_dir_does_not_follow_a_self_referential_symlink() {
        // Regression: walking used to determine "is this a directory to
        // recurse into?" via `path.is_dir()`, which follows symlinks. A
        // symlink pointing back at its own parent directory would then be
        // walked forever (bounded only by the MAX_DEPTH backstop, which
        // would still make this test slow and would still misreport the
        // tree). Using `entry.file_type()` (from `follow_links(false)`)
        // means the symlink is rendered as a leaf and never recursed
        // into, so this resolves quickly and `stats.directories` does not
        // count the symlink.
        let dir = scratch_dir("walk_symlink_loop");
        std::os::unix::fs::symlink(&dir, dir.join("self_link")).expect("create symlink");

        let (output, stats) = generate_tree(&dir, "root", "proj").unwrap();
        assert_eq!(
            stats.directories, 0,
            "symlink must not be counted as a directory to recurse into"
        );
        assert!(output.contains("self_link"));
    }
}
