use std::{fs, path::Path};

use filetime::FileTime;

use crate::error::{io_err, Result};

/// Copy all POSIX metadata from `src` to `dest`.
pub fn preserve(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::metadata(src).map_err(|e| io_err(src, e))?;

    fs::set_permissions(dest, meta.permissions()).map_err(|e| io_err(dest, e))?;

    let atime = FileTime::from_last_access_time(&meta);
    let mtime = FileTime::from_last_modification_time(&meta);
    filetime::set_file_times(dest, atime, mtime).map_err(|e| io_err(dest, e))?;

    for attr_name in xattr::list(src).map_err(|e| io_err(src, e))? {
        if let Some(value) = xattr::get(src, &attr_name).map_err(|e| io_err(src, e))? {
            let _ = xattr::set(dest, &attr_name, &value);
        }
    }

    Ok(())
}
