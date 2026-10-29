//! Remote file transfer over SSH. SCP can't list folders, so browsing, upload and
//! download use SFTP (same SSH connection and credentials). Pure Rust (`russh`),
//! so it works the same on every OS.
//!
//! Host keys follow trust-on-first-use: the fingerprint is saved on the first
//! connection and a later mismatch refuses to connect (like OpenSSH known_hosts).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use russh::client;
use russh::keys::{HashAlg, PublicKeyOrCertificate};
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::StatusCode;
use serde::Serialize;
use tauri::{Manager, State};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use super::error::{AppError, AppResult, ErrorCode};
use super::file_io::file_name;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

struct Client {
    expected_fingerprint: Option<String>,
    seen_fingerprint: Arc<StdMutex<Option<String>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.fingerprint(HashAlg::Sha256),
            PublicKeyOrCertificate::Certificate(certificate) => {
                certificate.public_key().fingerprint(HashAlg::Sha256)
            }
        }
        .to_string();
        let trusted = self
            .expected_fingerprint
            .as_ref()
            .is_none_or(|expected| *expected == fingerprint);
        if let Ok(mut seen) = self.seen_fingerprint.lock() {
            *seen = Some(fingerprint);
        }
        Ok(trusted)
    }
}

struct Connection {
    session: client::Handle<Client>,
    sftp: SftpSession,
    host: String,
}

#[derive(Default)]
pub struct ScpState(Mutex<Option<Connection>>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectInfo {
    home: String,
    fingerprint: String,
    /// True when this host was never seen before (its key was just trusted).
    new_host: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCrumb {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
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
pub struct RemoteListing {
    path: String,
    crumbs: Vec<RemoteCrumb>,
    entries: Vec<RemoteEntry>,
}

fn sftp_error(error: SftpError) -> AppError {
    let code = match &error {
        SftpError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => ErrorCode::NotFound,
            StatusCode::PermissionDenied => ErrorCode::PermissionDenied,
            _ => ErrorCode::Io,
        },
        SftpError::Timeout => ErrorCode::Timeout,
        _ => ErrorCode::Io,
    };
    AppError::new(code, error.to_string())
}

fn ssh_error(error: russh::Error) -> AppError {
    AppError::new(ErrorCode::Unreachable, error.to_string())
}

/// Remote paths are always POSIX, whatever the local OS.
fn remote_join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn remote_crumbs(host: &str, path: &str) -> Vec<RemoteCrumb> {
    let mut crumbs = vec![RemoteCrumb {
        name: host.to_string(),
        path: "/".into(),
    }];
    let mut walked = String::new();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        walked = format!("{walked}/{part}");
        crumbs.push(RemoteCrumb {
            name: part.to_string(),
            path: walked.clone(),
        });
    }
    crumbs
}

// ── Known hosts (trust on first use) ─────────────────────────

fn known_hosts_path(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join("known_hosts.json"))
}

fn load_known_hosts(path: &Path) -> HashMap<String, String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

// ── Commands ─────────────────────────────────────────────────

async fn connection(
    state: &ScpState,
) -> AppResult<tokio::sync::MutexGuard<'_, Option<Connection>>> {
    let guard = state.0.lock().await;
    if guard.is_none() {
        return Err(AppError::new(ErrorCode::NotConnected, "not connected"));
    }
    Ok(guard)
}

/// Opens the SSH session, verifies the host key against `expected` (if any),
/// authenticates and starts SFTP. Independent of Tauri so it can be tested.
async fn establish(
    host: &str,
    port: u16,
    username: &str,
    password: String,
    expected: Option<String>,
) -> AppResult<(client::Handle<Client>, SftpSession, String, String)> {
    let seen = Arc::new(StdMutex::new(None));
    let handler = Client {
        expected_fingerprint: expected.clone(),
        seen_fingerprint: Arc::clone(&seen),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(600)),
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });

    let connected = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect(config, (host, port), handler),
    )
    .await
    .map_err(|_| AppError::new(ErrorCode::Timeout, "the server didn't answer in time"))?;
    let fingerprint = seen
        .lock()
        .ok()
        .and_then(|seen| seen.clone())
        .unwrap_or_default();
    let mut session = match connected {
        Ok(session) => session,
        Err(_)
            if expected
                .as_ref()
                .is_some_and(|expected| *expected != fingerprint) =>
        {
            return Err(AppError::new(
                ErrorCode::HostKeyChanged,
                format!(
                    "expected {}, got {fingerprint}",
                    expected.unwrap_or_default()
                ),
            ));
        }
        Err(error) => return Err(ssh_error(error)),
    };

    let authenticated = session
        .authenticate_password(username, password)
        .await
        .map_err(ssh_error)?;
    if !authenticated.success() {
        return Err(AppError::new(
            ErrorCode::AuthFailed,
            "wrong user or password",
        ));
    }

    let channel = session.channel_open_session().await.map_err(ssh_error)?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(ssh_error)?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(sftp_error)?;
    let home = sftp.canonicalize(".").await.map_err(sftp_error)?;
    Ok((session, sftp, home, fingerprint))
}

#[tauri::command]
pub async fn scp_connect(
    app: tauri::AppHandle,
    state: State<'_, ScpState>,
    host: String,
    port: u16,
    username: String,
    password: String,
) -> AppResult<ConnectInfo> {
    let host = host.trim().to_string();
    if host.is_empty() || username.trim().is_empty() || port == 0 {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "host, port and user are required",
        ));
    }

    let hosts_path = known_hosts_path(&app)?;
    let mut known_hosts = load_known_hosts(&hosts_path);
    let host_key = format!("{host}:{port}");
    let expected = known_hosts.get(&host_key).cloned();
    let (session, sftp, home, fingerprint) =
        establish(&host, port, username.trim(), password, expected.clone()).await?;

    let new_host = expected.is_none();
    if new_host {
        known_hosts.insert(host_key, fingerprint.clone());
        if let Ok(text) = serde_json::to_string_pretty(&known_hosts) {
            let _ = fs::write(&hosts_path, text);
        }
    }

    // Replacing an old connection closes it.
    if let Some(previous) = state.0.lock().await.replace(Connection {
        session,
        sftp,
        host,
    }) {
        let _ = previous
            .session
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
    }
    Ok(ConnectInfo {
        home,
        fingerprint,
        new_host,
    })
}

#[tauri::command]
pub async fn scp_list(
    state: State<'_, ScpState>,
    path: String,
    show_hidden: bool,
) -> AppResult<RemoteListing> {
    let guard = connection(&state).await?;
    let connection = guard.as_ref().expect("checked by connection()");
    let sftp = &connection.sftp;
    let dir = sftp.canonicalize(path).await.map_err(sftp_error)?;

    let mut entries = Vec::new();
    for item in sftp.read_dir(dir.clone()).await.map_err(sftp_error)? {
        let name = item.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let hidden = name.starts_with('.');
        if hidden && !show_hidden {
            continue;
        }
        let path = remote_join(&dir, &name);
        let mut metadata = item.metadata();
        // Directory listings don't follow symlinks; resolve them to show folders as folders.
        if metadata.is_symlink()
            && let Ok(target) = sftp.metadata(path.clone()).await
        {
            metadata = target;
        }
        let is_dir = metadata.is_dir();
        entries.push(RemoteEntry {
            kind: if is_dir { "dir" } else { "file" },
            size: if is_dir {
                0
            } else {
                metadata.size.unwrap_or(0)
            },
            modified_ms: metadata.mtime.map(|seconds| u64::from(seconds) * 1000),
            name,
            path,
            hidden,
        });
    }
    entries.sort_by(|a, b| {
        (a.kind != "dir")
            .cmp(&(b.kind != "dir"))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(RemoteListing {
        crumbs: remote_crumbs(&connection.host, &dir),
        path: dir,
        entries,
    })
}

/// Uploads a local file into `remote_dir`. Refuses to replace an existing file
/// unless `overwrite` is set (the UI asks first).
#[tauri::command]
pub async fn scp_upload(
    state: State<'_, ScpState>,
    local_path: String,
    remote_dir: String,
    overwrite: bool,
) -> AppResult<String> {
    let guard = connection(&state).await?;
    let sftp = &guard.as_ref().expect("checked by connection()").sftp;
    let remote_path = remote_join(&remote_dir, &file_name(Path::new(&local_path)));
    if !overwrite
        && sftp
            .try_exists(remote_path.clone())
            .await
            .map_err(sftp_error)?
    {
        return Err(AppError::new(
            ErrorCode::Exists,
            "a file with that name already exists",
        ));
    }
    let mut local = tokio::fs::File::open(&local_path).await?;
    let mut remote = sftp.create(remote_path.clone()).await.map_err(sftp_error)?;
    tokio::io::copy(&mut local, &mut remote).await?;
    remote.shutdown().await?;
    Ok(remote_path)
}

#[tauri::command]
pub async fn scp_download(
    state: State<'_, ScpState>,
    remote_path: String,
    local_path: String,
) -> AppResult<()> {
    let guard = connection(&state).await?;
    let sftp = &guard.as_ref().expect("checked by connection()").sftp;
    let mut remote = sftp.open(remote_path).await.map_err(sftp_error)?;
    let mut local = tokio::fs::File::create(&local_path).await?;
    tokio::io::copy(&mut remote, &mut local).await?;
    local.flush().await?;
    Ok(())
}

#[tauri::command]
pub async fn scp_disconnect(state: State<'_, ScpState>) -> AppResult<()> {
    if let Some(previous) = state.0.lock().await.take() {
        let _ = previous.sftp.close().await;
        let _ = previous
            .session
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_posix_paths_and_crumbs() {
        assert_eq!(remote_join("/", "etc"), "/etc");
        assert_eq!(remote_join("/home/ian", "a.txt"), "/home/ian/a.txt");
        let crumbs = remote_crumbs("server", "/home/ian");
        let names: Vec<_> = crumbs.iter().map(|c| c.name.as_str()).collect();
        let paths: Vec<_> = crumbs.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(names, ["server", "home", "ian"]);
        assert_eq!(paths, ["/", "/home", "/home/ian"]);
    }

    /// Needs a reachable SSH server: `MINOS_SSH_HOST=127.0.0.1 cargo test -- --ignored`.
    /// Checks the handshake, host-key pinning and password rejection without real credentials.
    #[test]
    #[ignore = "requires a running SSH server"]
    fn real_server_handshake_and_key_pinning() {
        let host = std::env::var("MINOS_SSH_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let attempt = |expected: Option<String>| {
            tauri::async_runtime::block_on(establish(
                &host,
                22,
                "minos-test-user",
                "wrong-password".into(),
                expected,
            ))
            .err()
            .map(|error| error.code)
        };
        // Unknown host: the key is accepted (trust on first use) and the wrong password refused.
        assert_eq!(attempt(None), Some(ErrorCode::AuthFailed));
        // A pinned key that does not match must refuse before any password is sent.
        assert_eq!(
            attempt(Some("SHA256:not-the-real-key".into())),
            Some(ErrorCode::HostKeyChanged)
        );
    }
}
