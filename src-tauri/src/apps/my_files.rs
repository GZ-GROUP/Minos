//! File manager commands. Every command receives the navigation `home` and refuses
//! to touch anything outside it, so the UI restriction is also enforced here.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FsErrorCode {
    NotFound,
    Exists,
    PermissionDenied,
    InvalidName,
    OutsideHome,
    NotEmpty,
    Io,
}

/// Serialized as `{ code, message }`; the frontend translates `code`.
#[derive(Debug, Serialize)]
pub struct FsError {
    code: FsErrorCode,
    message: String,
}

impl FsError {
    fn new(code: FsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<io::Error> for FsError {
    fn from(error: io::Error) -> Self {
        let code = match error.kind() {
            io::ErrorKind::NotFound => FsErrorCode::NotFound,
            io::ErrorKind::AlreadyExists => FsErrorCode::Exists,
            io::ErrorKind::PermissionDenied => FsErrorCode::PermissionDenied,
            io::ErrorKind::DirectoryNotEmpty => FsErrorCode::NotEmpty,
            _ => FsErrorCode::Io,
        };
        Self::new(code, error.to_string())
    }
}

type FsResult<T> = Result<T, FsError>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    name: String,
    path: String,
    /// "dir" | "file"
    kind: &'static str,
    size: u64,
    modified_ms: Option<u64>,
    hidden: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    path: String,
    /// From `home` (first) to the current folder (last).
    crumbs: Vec<Crumb>,
    entries: Vec<Entry>,
}

// ── Path helpers ──────────────────────────────────────────────

/// `canonicalize` on Windows returns verbatim paths (`\\?\C:\…`); strip the prefix
/// so paths shown to the user and sent back by the UI look normal.
fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    text.into_owned()
}

fn canonical_home(home: &str) -> FsResult<PathBuf> {
    Ok(fs::canonicalize(home)?)
}

/// Resolves `path` (symlinks and `..` included) and checks it stays inside `home`.
fn resolve_within(home: &Path, path: &str) -> FsResult<PathBuf> {
    let resolved = fs::canonicalize(path)?;
    if resolved.starts_with(home) {
        Ok(resolved)
    } else {
        Err(FsError::new(
            FsErrorCode::OutsideHome,
            "path is outside the home folder",
        ))
    }
}

const RESERVED_WINDOWS_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// One rule set for every OS so names created on one system stay valid on the others.
fn validate_name(name: &str) -> FsResult<()> {
    let invalid = |reason: &str| Err(FsError::new(FsErrorCode::InvalidName, reason));
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return invalid("name is empty");
    }
    if trimmed == "." || trimmed == ".." {
        return invalid("name is reserved");
    }
    if name
        .chars()
        .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c))
    {
        return invalid("name contains invalid characters");
    }
    if name.ends_with('.') || name.ends_with(' ') || name.starts_with(' ') {
        return invalid("name cannot start with a space or end with a dot or space");
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if RESERVED_WINDOWS_NAMES.contains(&stem.as_str()) {
        return invalid("name is reserved by the system");
    }
    // A single normal component: no traversal can sneak through.
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => invalid("name is not a single path component"),
    }
}

#[cfg(windows)]
fn is_hidden(name: &str, metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    name.starts_with('.')
        || metadata.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _metadata: &fs::Metadata) -> bool {
    name.starts_with('.')
}

fn crumbs(home: &Path, current: &Path) -> Vec<Crumb> {
    let home_name = home
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| display_path(home));
    let mut crumbs = vec![Crumb {
        name: home_name,
        path: display_path(home),
    }];
    let mut walked = home.to_path_buf();
    if let Ok(relative) = current.strip_prefix(home) {
        for part in relative.components() {
            walked.push(part);
            crumbs.push(Crumb {
                name: part.as_os_str().to_string_lossy().into_owned(),
                path: display_path(&walked),
            });
        }
    }
    crumbs
}

fn list(home: &Path, dir: &Path, show_hidden: bool) -> FsResult<Listing> {
    let mut entries: Vec<Entry> = fs::read_dir(dir)?
        .flatten()
        .filter_map(|item| {
            let path = item.path();
            let name = item.file_name().to_string_lossy().into_owned();
            // `fs::metadata` follows symlinks; broken links and unreadable items are skipped.
            let metadata = fs::metadata(&path).ok()?;
            let hidden = is_hidden(&name, &item.metadata().unwrap_or(metadata.clone()));
            if hidden && !show_hidden {
                return None;
            }
            Some(Entry {
                kind: if metadata.is_dir() { "dir" } else { "file" },
                size: if metadata.is_dir() { 0 } else { metadata.len() },
                modified_ms: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as u64),
                path: display_path(&path),
                name,
                hidden,
            })
        })
        .collect();

    // Folders first, then case-insensitive by name.
    entries.sort_by(|a, b| {
        (a.kind != "dir")
            .cmp(&(b.kind != "dir"))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(Listing {
        path: display_path(dir),
        crumbs: crumbs(home, dir),
        entries,
    })
}

// ── Commands ─────────────────────────────────────────────────

#[tauri::command]
pub async fn files_home(app: tauri::AppHandle) -> FsResult<String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|error| FsError::new(FsErrorCode::NotFound, error.to_string()))?;
    Ok(display_path(&fs::canonicalize(home)?))
}

#[tauri::command]
pub async fn files_list(home: String, path: String, show_hidden: bool) -> FsResult<Listing> {
    let home = canonical_home(&home)?;
    let dir = resolve_within(&home, &path)?;
    list(&home, &dir, show_hidden)
}

#[tauri::command]
pub async fn files_create_file(home: String, parent: String, name: String) -> FsResult<String> {
    let home = canonical_home(&home)?;
    let parent = resolve_within(&home, &parent)?;
    validate_name(&name)?;
    let target = parent.join(&name);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)?;
    Ok(display_path(&target))
}

#[tauri::command]
pub async fn files_create_dir(home: String, parent: String, name: String) -> FsResult<String> {
    let home = canonical_home(&home)?;
    let parent = resolve_within(&home, &parent)?;
    validate_name(&name)?;
    let target = parent.join(&name);
    fs::create_dir(&target)?;
    Ok(display_path(&target))
}

#[tauri::command]
pub async fn files_rename(home: String, path: String, new_name: String) -> FsResult<String> {
    let home = canonical_home(&home)?;
    let source = resolve_within(&home, &path)?;
    if source == home {
        return Err(FsError::new(
            FsErrorCode::OutsideHome,
            "the home folder can't be renamed",
        ));
    }
    validate_name(&new_name)?;
    let parent = source
        .parent()
        .ok_or_else(|| FsError::new(FsErrorCode::NotFound, "missing parent folder"))?;
    let target = parent.join(&new_name);
    // On case-insensitive file systems "a.txt" → "A.txt" resolves to the same file: allow it.
    if target.exists() && fs::canonicalize(&target)? != source {
        return Err(FsError::new(
            FsErrorCode::Exists,
            "an item with that name already exists",
        ));
    }
    fs::rename(&source, &target)?;
    Ok(display_path(&target))
}

#[tauri::command]
pub async fn files_delete(home: String, path: String) -> FsResult<()> {
    let home = canonical_home(&home)?;
    let target = resolve_within(&home, &path)?;
    if target == home {
        return Err(FsError::new(
            FsErrorCode::OutsideHome,
            "the home folder can't be deleted",
        ));
    }
    // Permanent: nothing goes to the recycle bin (the UI warns about it).
    if fs::symlink_metadata(&target)?.is_dir() {
        fs::remove_dir_all(&target)?;
    } else {
        fs::remove_file(&target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::async_runtime::block_on;

    fn setup() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let home = display_path(&fs::canonicalize(dir.path()).unwrap());
        (dir, home)
    }

    #[test]
    fn rejects_invalid_names() {
        for name in [
            "", " ", ".", "..", "a/b", r"a\b", "a:b", "what?", "CON", "nul.txt", "end.", "end ",
            "\u{7}",
        ] {
            assert!(validate_name(name).is_err(), "{name:?} should be rejected");
        }
        for name in [
            "notes.txt",
            "My Folder",
            ".gitignore",
            "con-artist.md",
            "año 2026",
        ] {
            assert!(validate_name(name).is_ok(), "{name:?} should be accepted");
        }
    }

    #[test]
    fn create_rename_delete_round_trip() {
        let (_dir, home) = setup();
        let folder = block_on(files_create_dir(home.clone(), home.clone(), "Docs".into())).unwrap();
        let file = block_on(files_create_file(
            home.clone(),
            folder.clone(),
            "a.txt".into(),
        ))
        .unwrap();

        let exists = block_on(files_create_file(
            home.clone(),
            folder.clone(),
            "a.txt".into(),
        ))
        .unwrap_err();
        assert_eq!(exists.code, FsErrorCode::Exists);

        let renamed = block_on(files_rename(home.clone(), file, "b.txt".into())).unwrap();
        let listing = block_on(files_list(home.clone(), folder.clone(), false)).unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "b.txt");
        assert_eq!(listing.crumbs.len(), 2);
        assert_eq!(listing.crumbs[1].name, "Docs");

        block_on(files_delete(home.clone(), renamed)).unwrap();
        block_on(files_delete(home.clone(), folder)).unwrap();
        assert!(
            block_on(files_list(home.clone(), home, false))
                .unwrap()
                .entries
                .is_empty()
        );
    }

    #[test]
    fn stays_inside_home() {
        let (_dir, home) = setup();
        let inner = block_on(files_create_dir(home.clone(), home.clone(), "inner".into())).unwrap();

        let parent = Path::new(&inner).join("..").join("..");
        let error = block_on(files_list(
            inner.clone(),
            parent.to_string_lossy().into(),
            false,
        ))
        .unwrap_err();
        assert_eq!(error.code, FsErrorCode::OutsideHome);

        let error = block_on(files_delete(home.clone(), home.clone())).unwrap_err();
        assert_eq!(error.code, FsErrorCode::OutsideHome);
    }

    #[test]
    fn hides_dotfiles_unless_requested() {
        let (_dir, home) = setup();
        block_on(files_create_file(
            home.clone(),
            home.clone(),
            ".secret".into(),
        ))
        .unwrap();
        block_on(files_create_file(
            home.clone(),
            home.clone(),
            "visible".into(),
        ))
        .unwrap();
        assert_eq!(
            block_on(files_list(home.clone(), home.clone(), false))
                .unwrap()
                .entries
                .len(),
            1
        );
        assert_eq!(
            block_on(files_list(home.clone(), home, true))
                .unwrap()
                .entries
                .len(),
            2
        );
    }
}
