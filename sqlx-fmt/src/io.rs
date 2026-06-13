use std::path::Path;

/// Write to a temp file, then rename to destination path atomically.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();

    let tmp_file = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));

    if let Err(e) = std::fs::write(&tmp_file, contents) {
        let _ = std::fs::remove_file(&tmp_file);
        return Err(e);
    }
    std::fs::rename(&tmp_file, path)
}

pub fn backup_file(path: &Path) -> std::io::Result<()> {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bk");
    std::fs::copy(path, backup)?;
    Ok(())
}
