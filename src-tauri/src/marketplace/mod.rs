//! Commands behind the Marketplace: list, validate and download modules. Nothing
//! from a third party is extracted or executed here. See `docs/modules.md`.

mod catalog;
mod manifest;
mod package;
mod source;
mod store;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::State;
use tauri::async_runtime::spawn_blocking;
use tauri::ipc::Channel;

use crate::apps::error::{AppError, AppResult, ErrorCode};
use catalog::Catalog;
use manifest::{Manifest, Platform, invalid};
use package::{MAX_PACKAGE_BYTES, validate_package};
use source::{GitHub, parse_repository_url};
use store::{StateView, Store, TempFile, check_local_package};

pub struct MarketplaceState(Mutex<Store>);

impl MarketplaceState {
    pub fn open(app_data_dir: PathBuf) -> Self {
        Self(Mutex::new(Store::open(app_data_dir.join("marketplace"))))
    }
}

fn lock(store: &Mutex<Store>) -> AppResult<MutexGuard<'_, Store>> {
    store
        .lock()
        .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))
}

fn view(store: &Mutex<Store>) -> AppResult<StateView> {
    Ok(lock(store)?.view(Platform::current()))
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    /// 0–100, or `None` when the server doesn't report the size.
    percent: Option<u8>,
}

async fn validate_in_background(
    temp: TempFile,
    platform: Platform,
) -> AppResult<(TempFile, Manifest)> {
    spawn_blocking(move || {
        let manifest = validate_package(temp.path(), platform)?;
        Ok((temp, manifest))
    })
    .await
    .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?
}

async fn add_repository(
    github: &GitHub,
    store: &Mutex<Store>,
    url: &str,
    platform: Platform,
) -> AppResult<()> {
    let repo = parse_repository_url(url)?;
    let resolved = github.resolve(&repo, platform).await?;
    lock(store)?.add_repository(resolved.manifest, repo.url())
}

async fn add_zip(store: &Mutex<Store>, path: PathBuf, platform: Platform) -> AppResult<()> {
    let temp = lock(store)?.temp_file()?;
    // The copy is what gets validated and kept, so it can't change in between.
    let temp = spawn_blocking(move || -> AppResult<TempFile> {
        check_local_package(&path)?;
        if std::fs::metadata(&path)?.len() > MAX_PACKAGE_BYTES {
            return Err(invalid(format!(
                "the package is larger than the {} MB limit",
                MAX_PACKAGE_BYTES / (1024 * 1024)
            )));
        }
        std::fs::copy(&path, temp.path())?;
        Ok(temp)
    })
    .await
    .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))??;

    let (temp, manifest) = validate_in_background(temp, platform).await?;
    lock(store)?.commit_package(temp, manifest, None)
}

async fn download(
    github: &GitHub,
    store: &Mutex<Store>,
    url: &str,
    id: &str,
    platform: Platform,
    on_progress: impl FnMut(u64, Option<u64>) + Send,
) -> AppResult<()> {
    let repo = parse_repository_url(url)?;
    let resolved = github.resolve(&repo, platform).await?;
    if resolved.manifest.id != id {
        return Err(invalid(format!(
            "expected the module \"{id}\", but the repository contains \"{}\"",
            resolved.manifest.id
        )));
    }

    let temp = lock(store)?.temp_file()?;
    github
        .download(&resolved.asset, temp.path(), on_progress)
        .await?;

    let (temp, manifest) = validate_in_background(temp, platform).await?;
    if manifest.id != resolved.manifest.id || manifest.version != resolved.manifest.version {
        return Err(invalid(format!(
            "the release announces {} {}, but its package contains {} {}",
            resolved.manifest.id, resolved.manifest.version, manifest.id, manifest.version
        )));
    }
    lock(store)?.commit_package(temp, manifest, Some(repo.url()))
}

#[tauri::command]
pub fn marketplace_state(state: State<'_, MarketplaceState>) -> AppResult<StateView> {
    view(&state.0)
}

#[tauri::command]
pub async fn marketplace_catalog() -> AppResult<Catalog> {
    Ok(catalog::load(&GitHub::new()?).await)
}

#[tauri::command]
pub fn marketplace_set_installed(
    state: State<'_, MarketplaceState>,
    id: String,
    installed: bool,
) -> AppResult<StateView> {
    lock(&state.0)?.set_installed(&id, installed)?;
    view(&state.0)
}

#[tauri::command]
pub fn marketplace_set_order(
    state: State<'_, MarketplaceState>,
    ids: Vec<String>,
) -> AppResult<StateView> {
    lock(&state.0)?.set_order(ids)?;
    view(&state.0)
}

#[tauri::command]
pub async fn marketplace_add_repository(
    state: State<'_, MarketplaceState>,
    url: String,
) -> AppResult<StateView> {
    add_repository(&GitHub::new()?, &state.0, &url, Platform::current()).await?;
    view(&state.0)
}

#[tauri::command]
pub async fn marketplace_add_zip(
    state: State<'_, MarketplaceState>,
    path: String,
) -> AppResult<StateView> {
    add_zip(&state.0, PathBuf::from(path), Platform::current()).await?;
    view(&state.0)
}

#[tauri::command]
pub async fn marketplace_download(
    state: State<'_, MarketplaceState>,
    repo: String,
    id: String,
    on_progress: Channel<DownloadProgress>,
) -> AppResult<StateView> {
    let mut last = None;
    let report = move |downloaded: u64, total: Option<u64>| {
        let percent = total
            .filter(|total| *total > 0)
            .map(|total| (downloaded.saturating_mul(100) / total).min(100) as u8);
        // Chunks are small: only send what the progress bar can show.
        if percent != last {
            last = percent;
            let _ = on_progress.send(DownloadProgress { percent });
        }
    };
    download(
        &GitHub::new()?,
        &state.0,
        &repo,
        &id,
        Platform::current(),
        report,
    )
    .await?;
    view(&state.0)
}

#[tauri::command]
pub fn marketplace_remove(state: State<'_, MarketplaceState>, id: String) -> AppResult<StateView> {
    lock(&state.0)?.remove(&id)?;
    view(&state.0)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::io::Cursor;
    use std::path::Path;
    use std::sync::Arc;

    use tauri::async_runtime::block_on;
    use tiny_http::{Header, Response, Server, StatusCode};

    use super::package::test_support::{manifest_json, write_zip};
    use super::source::test_support::local_github;
    use super::*;

    const LINUX: Platform = Platform::Linux;
    const REPO: &str = "https://github.com/someone/hello";

    enum Reply {
        Ok(Vec<u8>),
        Status(u16),
        Redirect(String),
        /// Announces more bytes than it sends and then goes silent.
        Stalled(Vec<u8>),
    }

    fn release_json(tag: &str, assets: &[(&str, String)]) -> Vec<u8> {
        let assets: Vec<_> = assets
            .iter()
            .map(|(name, url)| serde_json::json!({ "name": name, "browser_download_url": url }))
            .collect();
        serde_json::json!({ "tag_name": tag, "assets": assets })
            .to_string()
            .into_bytes()
    }

    fn package_bytes(manifest: &str) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package.zip");
        write_zip(
            &path,
            &[
                ("minos-module.json", manifest.as_bytes()),
                ("ui/index.html", b"<html></html>"),
                ("ui/icon.svg", b"<svg/>"),
            ],
        );
        fs::read(path).unwrap()
    }

    /// Answers from a table of paths; anything else is a 404.
    fn serve(routes: impl FnOnce(&str) -> HashMap<String, Reply>) -> String {
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let routes = routes(&base);

        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let _ = match routes.get(request.url()) {
                    Some(Reply::Ok(body)) => request.respond(Response::from_data(body.clone())),
                    Some(Reply::Status(code)) => {
                        request.respond(Response::empty(StatusCode(*code)))
                    }
                    Some(Reply::Redirect(target)) => {
                        let location = Header::from_bytes("Location", target.as_str()).unwrap();
                        request.respond(Response::empty(StatusCode(302)).with_header(location))
                    }
                    Some(Reply::Stalled(body)) => request.respond(Response::new(
                        StatusCode(200),
                        Vec::new(),
                        Cursor::new(body.clone()),
                        Some(body.len() + 1000),
                        None,
                    )),
                    None => request.respond(Response::empty(StatusCode(404))),
                };
            }
        });
        base
    }

    const RELEASE: &str = "/repos/someone/hello/releases/latest";
    const RAW_MANIFEST: &str = "/raw/someone/hello/v1.0.0/minos-module.json";
    const ASSET: &str = "/assets/hello-1.0.0-linux.zip";

    /// A repository that passes every check.
    fn valid_routes(base: &str) -> HashMap<String, Reply> {
        let manifest = manifest_json("hello", "1.0.0", &["linux"]);
        HashMap::from([
            (
                RELEASE.to_string(),
                Reply::Ok(release_json(
                    "v1.0.0",
                    &[("hello-1.0.0-linux.zip", format!("{base}{ASSET}"))],
                )),
            ),
            (
                RAW_MANIFEST.to_string(),
                Reply::Ok(manifest.clone().into_bytes()),
            ),
            (ASSET.to_string(), Reply::Ok(package_bytes(&manifest))),
        ])
    }

    fn run_download(base: &str, root: &Path) -> (Mutex<Store>, AppResult<()>) {
        let store = Mutex::new(Store::open(root.to_path_buf()));
        let github = local_github(base);
        let result = block_on(download(&github, &store, REPO, "hello", LINUX, |_, _| {}));
        (store, result)
    }

    fn no_temp_files_left(root: &Path) -> bool {
        fs::read_dir(root.join("tmp"))
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true)
    }

    #[test]
    fn downloads_and_stores_a_valid_module() {
        let base = serve(valid_routes);
        let dir = tempfile::tempdir().unwrap();

        let store = Mutex::new(Store::open(dir.path().to_path_buf()));
        let github = local_github(&base);
        let mut reported = 0;
        block_on(download(
            &github,
            &store,
            REPO,
            "hello",
            LINUX,
            |downloaded, _| {
                reported = downloaded;
            },
        ))
        .unwrap();

        assert!(reported > 0);
        assert!(dir.path().join("downloads/hello/1.0.0.zip").exists());
        assert!(no_temp_files_left(dir.path()));

        let state = lock(&store).unwrap().view(LINUX);
        assert!(state.modules[0].downloaded);
        assert_eq!(state.modules[0].repo.as_deref(), Some(REPO));
    }

    #[test]
    fn follows_redirects_within_allowed_hosts() {
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            let package = routes.remove(ASSET).unwrap();
            routes.insert("/storage/package.zip".into(), package);
            routes.insert(
                ASSET.to_string(),
                Reply::Redirect(format!("{base}/storage/package.zip")),
            );
            routes
        });
        let dir = tempfile::tempdir().unwrap();
        run_download(&base, dir.path()).1.unwrap();
        assert!(dir.path().join("downloads/hello/1.0.0.zip").exists());
    }

    #[test]
    fn refuses_a_redirect_to_another_host() {
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            routes.insert(
                ASSET.to_string(),
                Reply::Redirect("http://localhost:1/evil.zip".into()),
            );
            routes
        });
        let dir = tempfile::tempdir().unwrap();
        let (store, result) = run_download(&base, dir.path());

        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidModule);
        assert!(lock(&store).unwrap().view(LINUX).modules.is_empty());
    }

    #[test]
    fn refuses_an_asset_hosted_on_another_host() {
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            routes.insert(
                RELEASE.to_string(),
                Reply::Ok(release_json(
                    "v1.0.0",
                    &[(
                        "hello-1.0.0-linux.zip",
                        "http://localhost:1/evil.zip".into(),
                    )],
                )),
            );
            routes
        });
        let dir = tempfile::tempdir().unwrap();
        let error = run_download(&base, dir.path()).1.unwrap_err();
        assert!(
            error.message.contains("outside GitHub"),
            "{}",
            error.message
        );
    }

    #[test]
    fn cleans_up_when_the_download_stalls() {
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            let Some(Reply::Ok(body)) = routes.remove(ASSET) else {
                unreachable!()
            };
            routes.insert(ASSET.to_string(), Reply::Stalled(body));
            routes
        });
        let dir = tempfile::tempdir().unwrap();
        let (store, result) = run_download(&base, dir.path());

        assert_eq!(result.unwrap_err().code, ErrorCode::Timeout);
        assert!(no_temp_files_left(dir.path()));
        assert!(!dir.path().join("downloads").exists());
        assert!(lock(&store).unwrap().view(LINUX).modules.is_empty());
    }

    #[test]
    fn rejects_a_package_that_differs_from_the_release() {
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            let other = manifest_json("hello", "2.0.0", &["linux"]);
            routes.insert(ASSET.to_string(), Reply::Ok(package_bytes(&other)));
            routes
        });
        let dir = tempfile::tempdir().unwrap();
        let error = run_download(&base, dir.path()).1.unwrap_err();

        assert!(
            error.message.contains("its package contains"),
            "{}",
            error.message
        );
        assert!(no_temp_files_left(dir.path()));
    }

    #[test]
    fn rejects_a_repository_with_another_module() {
        let base = serve(valid_routes);
        let dir = tempfile::tempdir().unwrap();
        let store = Mutex::new(Store::open(dir.path().to_path_buf()));
        let github = local_github(&base);

        let error =
            block_on(download(&github, &store, REPO, "other", LINUX, |_, _| {})).unwrap_err();
        assert!(
            error.message.contains("expected the module"),
            "{}",
            error.message
        );
    }

    fn add_error(base: &str) -> AppError {
        let dir = tempfile::tempdir().unwrap();
        let store = Mutex::new(Store::open(dir.path().to_path_buf()));
        block_on(add_repository(&local_github(base), &store, REPO, LINUX)).unwrap_err()
    }

    #[test]
    fn explains_why_a_repository_is_not_a_module() {
        // No releases.
        let base = serve(|_| HashMap::new());
        assert!(add_error(&base).message.contains("no published release"));

        // A release, but no manifest.
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            routes.remove(RAW_MANIFEST);
            routes
        });
        assert!(add_error(&base).message.contains("is not a Minos module"));

        // A tag that doesn't match the version.
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            let manifest = manifest_json("hello", "1.0.1", &["linux"]);
            routes.insert(RAW_MANIFEST.to_string(), Reply::Ok(manifest.into_bytes()));
            routes
        });
        assert!(add_error(&base).message.contains("expected tag \"v1.0.1\""));

        // No package for this system.
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            routes.insert(RELEASE.to_string(), Reply::Ok(release_json("v1.0.0", &[])));
            routes
        });
        assert!(
            add_error(&base)
                .message
                .contains("no package for this system")
        );
    }

    #[test]
    fn rejects_a_repository_for_another_system() {
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            let manifest = manifest_json("hello", "1.0.0", &["windows"]);
            routes.insert(RAW_MANIFEST.to_string(), Reply::Ok(manifest.into_bytes()));
            routes
        });
        assert_eq!(add_error(&base).code, ErrorCode::UnsupportedPlatform);
    }

    #[test]
    fn reports_the_rate_limit() {
        let base = serve(|_| HashMap::from([(RELEASE.to_string(), Reply::Status(429))]));
        assert_eq!(add_error(&base).code, ErrorCode::RateLimited);
    }

    #[test]
    fn prefers_the_package_for_this_system_over_the_universal_one() {
        let base = serve(|base| {
            let mut routes = valid_routes(base);
            routes.insert(
                RELEASE.to_string(),
                Reply::Ok(release_json(
                    "v1.0.0",
                    &[
                        ("hello-1.0.0.zip", format!("{base}/missing.zip")),
                        ("hello-1.0.0-linux.zip", format!("{base}{ASSET}")),
                    ],
                )),
            );
            routes
        });
        let dir = tempfile::tempdir().unwrap();
        run_download(&base, dir.path()).1.unwrap();
    }

    #[test]
    fn adds_a_repository_without_downloading_it() {
        let base = serve(valid_routes);
        let dir = tempfile::tempdir().unwrap();
        let store = Mutex::new(Store::open(dir.path().to_path_buf()));
        let github = local_github(&base);

        block_on(add_repository(&github, &store, REPO, LINUX)).unwrap();
        let state = lock(&store).unwrap().view(LINUX);
        assert!(!state.modules[0].downloaded);
        assert!(!dir.path().join("downloads").exists());

        let again = block_on(add_repository(&github, &store, REPO, LINUX)).unwrap_err();
        assert_eq!(again.code, ErrorCode::Exists);
    }

    #[test]
    fn adds_a_local_zip_and_keeps_only_a_validated_copy() {
        let dir = tempfile::tempdir().unwrap();
        let store = Mutex::new(Store::open(dir.path().join("store")));

        let source = dir.path().join("hello.zip");
        let manifest = manifest_json("hello", "1.0.0", &["linux"]);
        fs::write(&source, package_bytes(&manifest)).unwrap();
        block_on(add_zip(&store, source.clone(), LINUX)).unwrap();

        assert!(source.exists(), "the user's file must stay where it was");
        assert!(dir.path().join("store/downloads/hello/1.0.0.zip").exists());

        let bad = dir.path().join("bad.zip");
        fs::write(&bad, b"not a zip").unwrap();
        assert!(block_on(add_zip(&store, bad, LINUX)).is_err());
        assert!(no_temp_files_left(&dir.path().join("store")));
    }

    /// The example contributors copy must always pass the real validation.
    #[test]
    fn the_example_module_is_valid() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/minos-module-hello");
        let bytes = fs::read(example.join("minos-module.json")).unwrap();
        let manifest = Manifest::parse(&bytes).unwrap();

        for platform in [Platform::Windows, Platform::Macos, Platform::Linux] {
            manifest.ensure_supported(platform).unwrap();
            let executable = &manifest.backend.as_ref().unwrap().exec[&platform];

            let dir = tempfile::tempdir().unwrap();
            let package = dir.path().join("hello.zip");
            let entry = fs::read(example.join(&manifest.entry)).unwrap();
            let icon = fs::read(example.join(&manifest.icon)).unwrap();
            write_zip(
                &package,
                &[
                    ("minos-module.json", bytes.as_slice()),
                    (manifest.entry.as_str(), entry.as_slice()),
                    (manifest.icon.as_str(), icon.as_slice()),
                    (executable.as_str(), b"built by the release workflow"),
                ],
            );
            validate_package(&package, platform).unwrap();
        }
    }
}
