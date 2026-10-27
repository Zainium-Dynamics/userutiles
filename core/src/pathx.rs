//! Path helpers with Zainium OS path-byte fidelity.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// Return true if `path` is exactly `-` (stdin/stdout sentinel).
pub fn is_stdin_dash(path: &OsStr) -> bool {
    path.as_bytes() == b"-"
}

/// Expand a user path; does not resolve symlinks.
pub fn to_path_buf(s: impl AsRef<OsStr>) -> PathBuf {
    PathBuf::from(s.as_ref())
}

/// Join base + relative safely (reject absolute `relative` overwriting base).
pub fn safe_join(base: &Path, relative: &Path) -> PathBuf {
    let mut out = base.to_path_buf();
    for c in relative.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => {
                // Absolute component — reset to that path.
                out = PathBuf::from(c.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(p) => out.push(p),
        }
    }
    out
}

/// Display path for errors (lossy UTF-8).
pub fn display_lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// How much of a path has to exist when it is canonicalized (`readlink`/`realpath`
/// `-e`, `-f`/`-E`, `-m`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Need {
    All,
    AllButLast,
    Nothing,
}

/// The absolute form of `path` with `.`, `..` and (when `follow`) symbolic links
/// resolved one component at a time, as GNU's canonicalize_filename_mode does: a
/// missing component is an error unless `need` allows it.
pub fn canonicalize_mode(path: &Path, need: Need, follow: bool) -> std::io::Result<PathBuf> {
    use std::io::{Error, ErrorKind};
    let mut result = if path.is_absolute() { PathBuf::from("/") } else { std::env::current_dir()? };
    let mut pending: Vec<std::ffi::OsString> = Vec::new();
    let push = |pending: &mut Vec<std::ffi::OsString>, p: &Path| {
        for c in p.components().rev() {
            match c {
                Component::Normal(n) => pending.push(n.to_os_string()),
                Component::ParentDir => pending.push("..".into()),
                _ => {}
            }
        }
    };
    push(&mut pending, path);
    let mut links = 0;
    while let Some(c) = pending.pop() {
        if c == ".." {
            result.pop();
            continue;
        }
        let next = result.join(&c);
        if !follow {
            if need != Need::Nothing && (need == Need::All || !pending.is_empty()) && std::fs::symlink_metadata(&next).is_err() {
                return Err(Error::from(ErrorKind::NotFound));
            }
            result = next;
            continue;
        }
        match std::fs::symlink_metadata(&next) {
            Ok(m) if m.file_type().is_symlink() => {
                links += 1;
                if links > 40 {
                    return Err(Error::from_raw_os_error(libc::ELOOP));
                }
                let target = std::fs::read_link(&next)?;
                if target.is_absolute() {
                    result = PathBuf::from("/");
                }
                push(&mut pending, &target);
            }
            Ok(m) => {
                if !pending.is_empty() && !m.is_dir() && need != Need::Nothing {
                    return Err(Error::from_raw_os_error(libc::ENOTDIR));
                }
                result = next;
            }
            Err(e) => match need {
                Need::All => return Err(e),
                Need::AllButLast if !pending.is_empty() => return Err(e),
                _ => result = next,
            },
        }
    }
    Ok(result)
}

/// The backup name for `dest`: `dest~N~` (numbered), `dest` + SUFFIX (simple), or numbered
/// when numbered backups already exist (existing).
pub fn backup_name(dest: &Path, control: &str, suffix: &str) -> PathBuf {
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let highest = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    n.strip_prefix(&format!("{name}.~"))?.strip_suffix('~')?.parse::<u64>().ok()
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let numbered = match control {
        "numbered" | "t" => true,
        "simple" | "never" => false,
        _ => highest > 0,
    };
    if numbered {
        dest.with_file_name(format!("{name}.~{}~", highest + 1))
    } else {
        dest.with_file_name(format!("{name}{suffix}"))
    }
}
