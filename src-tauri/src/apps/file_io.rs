//! File helpers shared by the converter apps: info about a user-picked file, a
//! scratch folder for generated results, and "download" (copy a result to a
//! location the user chose in the save dialog).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::{AppError, AppResult, ErrorCode};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
}

impl FileInfo {
    pub fn read(path: &Path) -> AppResult<Self> {
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(AppError::new(ErrorCode::InvalidInput, "not a file"));
        }
        Ok(Self {
            path: path.to_string_lossy().into_owned(),
            name: file_name(path),
            size: metadata.len(),
        })
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into())
}

/// Root of every generated result; `file_export` only copies from here.
fn output_root() -> PathBuf {
    std::env::temp_dir().join("minos-output")
}

/// A fresh, unique folder for one job's results.
pub fn new_output_dir() -> AppResult<PathBuf> {
    let dir = output_root().join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[tauri::command]
pub async fn file_info(path: String) -> AppResult<FileInfo> {
    FileInfo::read(Path::new(&path))
}

/// Writes a plain-text report (e.g. the My PC snapshot) to a path the user chose
/// in the save dialog. Windows line endings on Windows so Notepad shows it right.
#[tauri::command]
pub async fn report_save(path: String, content: String) -> AppResult<()> {
    let path = PathBuf::from(path);
    let is_txt = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"));
    if !is_txt {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "reports are saved as .txt files",
        ));
    }
    let normalized = content.replace("\r\n", "\n");
    let text = if cfg!(windows) {
        normalized.replace('\n', "\r\n")
    } else {
        normalized
    };
    fs::write(&path, text)?;
    Ok(())
}

/// Copies a generated result to `destination`. Sources outside the output folder
/// are rejected so the command can't be used to copy arbitrary files.
#[tauri::command]
pub async fn file_export(source: String, destination: String) -> AppResult<()> {
    let root = fs::canonicalize(output_root())?;
    let source = fs::canonicalize(&source)?;
    if !source.starts_with(&root) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "only generated files can be exported",
        ));
    }
    fs::copy(&source, &destination)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::async_runtime::block_on;

    #[test]
    fn saves_txt_reports_only() {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("snapshot.txt");
        block_on(report_save(report.to_string_lossy().into(), "a\nb".into())).unwrap();
        let expected = if cfg!(windows) { "a\r\nb" } else { "a\nb" };
        assert_eq!(fs::read_to_string(&report).unwrap(), expected);

        let other = dir.path().join("snapshot.exe");
        let error = block_on(report_save(other.to_string_lossy().into(), "x".into())).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(!other.exists());
    }
}
