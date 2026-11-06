//! What this device has installed, added and downloaded, under
//! `<app data>/marketplace/`. Downloaded packages are stored but never extracted.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::manifest::{INTEGRATED_APP_IDS, Manifest, Platform, invalid, validate_id};
use crate::apps::error::{AppError, AppResult, ErrorCode};

const STATE_VERSION: u32 = 1;
const STATE_FILE: &str = "state.json";

/// A module added or downloaded on this device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModule {
    pub manifest: Manifest,
    /// Where it came from; `None` for a package loaded from a local .zip.
    pub repo: Option<String>,
    pub downloaded: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateFile {
    version: u32,
    installed: Vec<String>,
    modules: Vec<LocalModule>,
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            installed: Vec::new(),
            modules: Vec::new(),
        }
    }
}

/// What the frontend sees.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateView {
    pub platform: Platform,
    pub installed: Vec<String>,
    pub modules: Vec<LocalModule>,
}

/// A file that's deleted unless it was moved to its final place.
pub struct TempFile(PathBuf);

impl TempFile {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub struct Store {
    root: PathBuf,
    state: StateFile,
}

impl Store {
    /// Never fails: an unreadable state is set aside so that Minos still opens.
    pub fn open(root: PathBuf) -> Self {
        // Left behind only if Minos closed in the middle of a download.
        let _ = fs::remove_dir_all(root.join("tmp"));

        let path = root.join(STATE_FILE);
        let state = match fs::read(&path) {
            Ok(bytes) => parse_state(&bytes).unwrap_or_else(|| {
                let _ = fs::rename(
                    &path,
                    root.join(format!("state.corrupt-{}.json", Uuid::new_v4())),
                );
                StateFile::default()
            }),
            Err(_) => StateFile::default(),
        };

        let mut store = Self { root, state };
        store.sanitize();
        store
    }

    /// Drops whatever no longer holds: unknown apps, invalid modules, missing files.
    fn sanitize(&mut self) {
        self.state
            .installed
            .retain(|id| INTEGRATED_APP_IDS.contains(&id.as_str()));

        let root = self.root.clone();
        self.state.modules.retain_mut(|module| {
            if module.manifest.validate().is_err() {
                return false;
            }
            if module.downloaded && !package_path(&root, &module.manifest).exists() {
                module.downloaded = false;
            }
            module.downloaded || module.repo.is_some()
        });
    }

    pub fn view(&self, platform: Platform) -> StateView {
        StateView {
            platform,
            installed: self.state.installed.clone(),
            modules: self.state.modules.clone(),
        }
    }

    pub fn set_installed(&mut self, id: &str, installed: bool) -> AppResult<()> {
        if !INTEGRATED_APP_IDS.contains(&id) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("\"{id}\" is not an app included in Minos"),
            ));
        }
        self.state.installed.retain(|existing| existing != id);
        if installed {
            self.state.installed.push(id.to_string());
        }
        self.save()
    }

    /// Sets the home screen order. It can only rearrange the installed apps.
    pub fn set_order(&mut self, ids: Vec<String>) -> AppResult<()> {
        let mut requested: Vec<&String> = ids.iter().collect();
        let mut current: Vec<&String> = self.state.installed.iter().collect();
        requested.sort();
        current.sort();
        if requested != current {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "the new order must list exactly the installed apps",
            ));
        }
        self.state.installed = ids;
        self.save()
    }

    pub fn find(&self, id: &str) -> Option<&LocalModule> {
        self.state.modules.iter().find(|m| m.manifest.id == id)
    }

    /// Lists a repository without downloading it.
    pub fn add_repository(&mut self, manifest: Manifest, repo: String) -> AppResult<()> {
        if self.find(&manifest.id).is_some() {
            return Err(already_added(&manifest.id));
        }
        self.state.modules.push(LocalModule {
            manifest,
            repo: Some(repo),
            downloaded: false,
        });
        self.save()
    }

    /// A new temporary file for a package that hasn't been validated yet.
    pub fn temp_file(&self) -> AppResult<TempFile> {
        let dir = self.root.join("tmp");
        fs::create_dir_all(&dir)?;
        Ok(TempFile(dir.join(format!("{}.part", Uuid::new_v4()))))
    }

    /// Moves a validated package to its final place and records it.
    pub fn commit_package(
        &mut self,
        temp: TempFile,
        manifest: Manifest,
        repo: Option<String>,
    ) -> AppResult<()> {
        if self.find(&manifest.id).is_some_and(|m| m.downloaded) {
            return Err(already_added(&manifest.id));
        }

        let target = package_path(&self.root, &manifest);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(temp.path(), &target)?;

        self.state.modules.retain(|m| m.manifest.id != manifest.id);
        self.state.modules.push(LocalModule {
            manifest,
            repo,
            downloaded: true,
        });
        self.save()
    }

    /// Forgets a module and deletes its package.
    pub fn remove(&mut self, id: &str) -> AppResult<()> {
        validate_id(id)?;
        match fs::remove_dir_all(self.root.join("downloads").join(id)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        self.state.modules.retain(|m| m.manifest.id != id);
        self.save()
    }

    /// Writes to a temporary file first, so that a crash can't leave half a state.
    fn save(&self) -> AppResult<()> {
        fs::create_dir_all(&self.root)?;
        let bytes = serde_json::to_vec_pretty(&self.state)
            .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?;
        let temp = self.root.join(format!("{STATE_FILE}.tmp"));
        fs::write(&temp, bytes)?;
        fs::rename(&temp, self.root.join(STATE_FILE))?;
        Ok(())
    }
}

fn parse_state(bytes: &[u8]) -> Option<StateFile> {
    serde_json::from_slice::<StateFile>(bytes)
        .ok()
        .filter(|state| state.version == STATE_VERSION)
}

fn package_path(root: &Path, manifest: &Manifest) -> PathBuf {
    root.join("downloads")
        .join(&manifest.id)
        .join(format!("{}.zip", manifest.version))
}

fn already_added(id: &str) -> AppError {
    AppError::new(ErrorCode::Exists, format!("\"{id}\" was already added"))
}

/// A package a user picked must at least look like one before it's copied.
pub fn check_local_package(path: &Path) -> AppResult<()> {
    let is_zip = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    if !is_zip {
        return Err(invalid("the file must have a .zip extension"));
    }
    if !fs::metadata(path)?.is_file() {
        return Err(invalid("the path is not a file"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::package::test_support::manifest_json;

    fn manifest(id: &str) -> Manifest {
        Manifest::parse(manifest_json(id, "1.0.0", &["linux"]).as_bytes()).unwrap()
    }

    fn view(store: &Store) -> StateView {
        store.view(Platform::Linux)
    }

    fn temp_with_contents(store: &Store) -> TempFile {
        let temp = store.temp_file().unwrap();
        fs::write(temp.path(), b"zip").unwrap();
        temp
    }

    #[test]
    fn starts_empty_without_a_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("marketplace"));
        assert!(view(&store).installed.is_empty());
        assert!(view(&store).modules.is_empty());
    }

    #[test]
    fn keeps_installed_apps_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        store.set_installed("my-files", true).unwrap();
        store.set_installed("my-files", true).unwrap();
        store.set_installed("processes", true).unwrap();
        store.set_installed("processes", false).unwrap();

        let reopened = Store::open(dir.path().to_path_buf());
        assert_eq!(view(&reopened).installed, ["my-files"]);
    }

    #[test]
    fn keeps_the_home_order_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        store.set_installed("my-files", true).unwrap();
        store.set_installed("my-pc", true).unwrap();
        store.set_installed("processes", true).unwrap();

        store
            .set_order(vec!["processes".into(), "my-files".into(), "my-pc".into()])
            .unwrap();

        let reopened = Store::open(dir.path().to_path_buf());
        assert_eq!(
            view(&reopened).installed,
            ["processes", "my-files", "my-pc"]
        );
    }

    #[test]
    fn the_order_can_only_rearrange_installed_apps() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        store.set_installed("my-files", true).unwrap();
        store.set_installed("my-pc", true).unwrap();

        for ids in [
            vec!["my-files"],
            vec!["my-files", "my-pc", "processes"],
            vec!["my-files", "processes"],
            vec!["my-files", "my-files"],
        ] {
            let ids = ids.into_iter().map(String::from).collect();
            let error = store.set_order(ids).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidInput);
        }
        assert_eq!(view(&store).installed, ["my-files", "my-pc"]);
    }

    #[test]
    fn only_integrated_apps_can_be_installed_directly() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        let error = store.set_installed("hello", true).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn sets_a_corrupt_state_aside_and_keeps_working() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(STATE_FILE), b"{ not json").unwrap();

        let mut store = Store::open(dir.path().to_path_buf());
        assert!(view(&store).installed.is_empty());

        let set_aside = fs::read_dir(dir.path()).unwrap().any(|entry| {
            let name = entry.unwrap().file_name();
            name.to_string_lossy().starts_with("state.corrupt-")
        });
        assert!(set_aside);

        store.set_installed("my-pc", true).unwrap();
        assert_eq!(
            view(&Store::open(dir.path().to_path_buf())).installed,
            ["my-pc"]
        );
    }

    #[test]
    fn drops_stale_entries_when_opening() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        store
            .commit_package(temp_with_contents(&store), manifest("from-zip"), None)
            .unwrap();
        store
            .commit_package(
                temp_with_contents(&store),
                manifest("from-repo"),
                Some("https://github.com/a/b".into()),
            )
            .unwrap();
        fs::remove_dir_all(dir.path().join("downloads")).unwrap();

        let reopened = Store::open(dir.path().to_path_buf());
        let modules = view(&reopened).modules;
        // The .zip can't be recovered; the repository can be downloaded again.
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].manifest.id, "from-repo");
        assert!(!modules[0].downloaded);
    }

    #[test]
    fn stores_a_package_and_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        let temp = temp_with_contents(&store);
        let temp_path = temp.path().to_path_buf();

        store.commit_package(temp, manifest("hello"), None).unwrap();
        let package = dir.path().join("downloads/hello/1.0.0.zip");
        assert!(package.exists());
        assert!(!temp_path.exists());

        let duplicate = temp_with_contents(&store);
        let error = store
            .commit_package(duplicate, manifest("hello"), None)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Exists);

        store.remove("hello").unwrap();
        assert!(!package.exists());
        assert!(view(&store).modules.is_empty());
    }

    #[test]
    fn remove_never_leaves_the_downloads_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().to_path_buf());
        fs::write(dir.path().join("keep.txt"), b"x").unwrap();

        assert!(store.remove("..").is_err());
        assert!(store.remove("../keep.txt").is_err());
        assert!(dir.path().join("keep.txt").exists());
    }

    #[test]
    fn a_temp_file_is_deleted_unless_committed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf());
        let temp = temp_with_contents(&store);
        let path = temp.path().to_path_buf();
        drop(temp);
        assert!(!path.exists());
    }

    #[test]
    fn checks_local_package_paths() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("Module.ZIP");
        let text = dir.path().join("module.txt");
        fs::write(&zip, b"x").unwrap();
        fs::write(&text, b"x").unwrap();

        assert!(check_local_package(&zip).is_ok());
        assert!(check_local_package(&text).is_err());
        assert!(check_local_package(&dir.path().join("missing.zip")).is_err());
    }
}
