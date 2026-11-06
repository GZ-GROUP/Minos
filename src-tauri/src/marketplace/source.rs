//! Reads modules from GitHub: the repository link, its latest release and assets.

use std::path::Path;
use std::time::Duration;

use reqwest::{Client, StatusCode, redirect};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use url::Url;

use super::manifest::{MANIFEST_FILE, Manifest, Platform, invalid};
use super::package::{MAX_MANIFEST_BYTES, MAX_PACKAGE_BYTES};
use crate::apps::error::{AppError, AppResult, ErrorCode};

const MAX_API_BYTES: u64 = 4 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

/// A GitHub repository, from a validated link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub owner: String,
    pub repo: String,
}

impl RepoRef {
    pub fn url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
    }
}

/// Accepts only `https://github.com/<owner>/<repo>`, with an optional `.git` or `/`.
pub fn parse_repository_url(input: &str) -> AppResult<RepoRef> {
    let rejected = |reason: &str| {
        AppError::new(
            ErrorCode::InvalidRepositoryUrl,
            format!("expected a link like https://github.com/owner/repository: {reason}"),
        )
    };

    let input = input.trim();
    // The URL parser would resolve these, hiding what was actually written.
    if input.contains('%') || input.split('/').any(|part| part == "." || part == "..") {
        return Err(rejected("the link has an unsafe path"));
    }
    let url = Url::parse(input).map_err(|_| rejected("this is not a link"))?;
    if url.scheme() != "https" {
        return Err(rejected("the link must start with https://"));
    }
    if url.host_str() != Some("github.com") || url.port().is_some() {
        return Err(rejected("the link must point to github.com"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(rejected("the link must not include credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(rejected("the link must not include a query or a fragment"));
    }

    let path = url.path().trim_start_matches('/');
    let path = path.strip_suffix('/').unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    let [owner, repo] = parts.as_slice() else {
        return Err(rejected(
            "the link must point to the repository itself, not to a page inside it",
        ));
    };
    let repo = repo.strip_suffix(".git").unwrap_or(repo);

    let valid_owner = (1..=39).contains(&owner.len())
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let valid_repo = (1..=100).contains(&repo.len())
        && repo != "."
        && repo != ".."
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if !valid_owner || !valid_repo {
        return Err(rejected("the owner or repository name is not valid"));
    }

    Ok(RepoRef {
        owner: (*owner).to_string(),
        repo: repo.to_string(),
    })
}

/// Hosts a module may be fetched from, also after a redirect.
fn is_github_url(url: &Url) -> bool {
    let host_allowed = url.host_str().is_some_and(|host| {
        host == "github.com" || host == "api.github.com" || host.ends_with(".githubusercontent.com")
    });
    url.scheme() == "https" && host_allowed && url.username().is_empty() && url.password().is_none()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// A repository that passed every check that doesn't require downloading the package.
#[derive(Debug)]
pub struct Resolved {
    pub manifest: Manifest,
    pub asset: Asset,
}

#[derive(Clone)]
pub struct GitHub {
    client: Client,
    api_base: String,
    raw_base: String,
    allowed: fn(&Url) -> bool,
}

impl GitHub {
    pub fn new() -> AppResult<Self> {
        Self::with_endpoints(
            "https://api.github.com",
            "https://raw.githubusercontent.com",
            is_github_url,
            Duration::from_secs(30),
        )
    }

    fn with_endpoints(
        api_base: &str,
        raw_base: &str,
        allowed: fn(&Url) -> bool,
        read_timeout: Duration,
    ) -> AppResult<Self> {
        let policy = redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if allowed(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("redirected to a host that is not allowed")
            }
        });
        let client = Client::builder()
            .user_agent(concat!("Minos/", env!("CARGO_PKG_VERSION")))
            .redirect(policy)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(read_timeout)
            .build()
            .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?;
        Ok(Self {
            client,
            api_base: api_base.to_string(),
            raw_base: raw_base.to_string(),
            allowed,
        })
    }

    /// Validates a repository as a module for this system, without downloading it.
    pub async fn resolve(&self, repo: &RepoRef, platform: Platform) -> AppResult<Resolved> {
        let release = self.latest_release(repo).await?;
        let manifest = self.manifest_at(repo, &release.tag_name).await?;

        let expected_tag = format!("v{}", manifest.version);
        if release.tag_name != expected_tag {
            return Err(invalid(format!(
                "the release tag is \"{}\", but {MANIFEST_FILE} has version {} (expected tag \"{expected_tag}\")",
                release.tag_name, manifest.version
            )));
        }
        manifest.ensure_supported(platform)?;
        let asset = select_asset(&release.assets, &manifest, platform)?;
        Ok(Resolved { manifest, asset })
    }

    async fn latest_release(&self, repo: &RepoRef) -> AppResult<Release> {
        let url = format!(
            "{}/repos/{}/{}/releases/latest",
            self.api_base, repo.owner, repo.repo
        );
        let request = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github+json");
        let response = request.send().await.map_err(request_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(invalid(format!(
                "{} doesn't exist, is private, or has no published release",
                repo.url()
            )));
        }
        let response = checked(response)?;
        let bytes = read_limited(response, MAX_API_BYTES).await?;
        serde_json::from_slice(&bytes)
            .map_err(|error| AppError::new(ErrorCode::Unreachable, error.to_string()))
    }

    async fn manifest_at(&self, repo: &RepoRef, tag: &str) -> AppResult<Manifest> {
        let safe_tag = tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'));
        if tag.is_empty() || !safe_tag {
            return Err(invalid(format!("the release tag \"{tag}\" is not valid")));
        }

        let url = format!(
            "{}/{}/{}/{tag}/{MANIFEST_FILE}",
            self.raw_base, repo.owner, repo.repo
        );
        let response = self.client.get(url).send().await.map_err(request_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(invalid(format!(
                "{} is not a Minos module: {MANIFEST_FILE} is missing from the root of release \"{tag}\"",
                repo.url()
            )));
        }
        let bytes = read_limited(checked(response)?, MAX_MANIFEST_BYTES).await?;
        Manifest::parse(&bytes)
    }

    /// Fetches a file that belongs to Minos itself, such as the catalog.
    pub async fn fetch_raw(&self, path: &str, limit: u64, timeout: Duration) -> AppResult<Vec<u8>> {
        let url = format!("{}/{path}", self.raw_base);
        let response = self
            .client
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(request_error)?;
        read_limited(checked(response)?, limit).await
    }

    /// Streams an asset to `target`, reporting `(downloaded, total)` after each chunk.
    pub async fn download(
        &self,
        asset: &Asset,
        target: &Path,
        mut on_progress: impl FnMut(u64, Option<u64>) + Send,
    ) -> AppResult<()> {
        let too_large = || {
            invalid(format!(
                "the package is larger than the {} MB limit",
                MAX_PACKAGE_BYTES / (1024 * 1024)
            ))
        };
        if asset.size > MAX_PACKAGE_BYTES {
            return Err(too_large());
        }
        let url = Url::parse(&asset.browser_download_url)
            .map_err(|_| invalid("the release asset has an invalid link"))?;
        if !(self.allowed)(&url) {
            return Err(invalid(format!(
                "the release asset is hosted outside GitHub: {url}"
            )));
        }

        let mut response = checked(self.client.get(url).send().await.map_err(request_error)?)?;
        let total = response.content_length();
        if total.is_some_and(|total| total > MAX_PACKAGE_BYTES) {
            return Err(too_large());
        }

        let mut file = tokio::fs::File::create(target).await?;
        let mut downloaded = 0_u64;
        while let Some(chunk) = response.chunk().await.map_err(request_error)? {
            downloaded += chunk.len() as u64;
            if downloaded > MAX_PACKAGE_BYTES {
                return Err(too_large());
            }
            file.write_all(&chunk).await?;
            on_progress(downloaded, total);
        }
        file.flush().await?;
        Ok(())
    }
}

/// Picks `<id>-<version>-<os>.zip`, or the universal `<id>-<version>.zip`.
fn select_asset(assets: &[Asset], manifest: &Manifest, platform: Platform) -> AppResult<Asset> {
    let specific = format!(
        "{}-{}-{}.zip",
        manifest.id,
        manifest.version,
        platform.as_str()
    );
    let universal = format!("{}-{}.zip", manifest.id, manifest.version);
    [&specific, &universal]
        .iter()
        .find_map(|name| assets.iter().find(|asset| &&asset.name == name))
        .cloned()
        .ok_or_else(|| {
            invalid(format!(
                "the release has no package for this system: expected an asset named \"{specific}\" or \"{universal}\""
            ))
        })
}

fn request_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::new(ErrorCode::Timeout, error.to_string())
    } else if error.is_redirect() {
        invalid(format!("the download was refused: {error}"))
    } else {
        AppError::new(ErrorCode::Unreachable, error.to_string())
    }
}

fn checked(response: reqwest::Response) -> AppResult<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && response
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|remaining| remaining == "0"));
    if rate_limited {
        return Err(AppError::new(
            ErrorCode::RateLimited,
            "GitHub limits how many requests can be made per hour",
        ));
    }
    Err(AppError::new(
        ErrorCode::Unreachable,
        format!("GitHub answered with HTTP {status}"),
    ))
}

async fn read_limited(mut response: reqwest::Response, limit: u64) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(request_error)? {
        if (bytes.len() + chunk.len()) as u64 > limit {
            return Err(invalid("the response from GitHub is larger than expected"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;

    fn is_loopback_ip(url: &Url) -> bool {
        url.host_str() == Some("127.0.0.1")
    }

    /// A client for a local test server, which rejects any other host.
    pub fn local_github(base: &str) -> GitHub {
        let read_timeout = Duration::from_secs(2);
        GitHub::with_endpoints(base, &format!("{base}/raw"), is_loopback_ip, read_timeout).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_repository_links() {
        for link in [
            "https://github.com/GZ-GROUP/Minos",
            "https://github.com/GZ-GROUP/Minos/",
            "https://github.com/GZ-GROUP/Minos.git",
            "  https://github.com/GZ-GROUP/Minos  ",
        ] {
            let parsed = parse_repository_url(link).unwrap();
            assert_eq!(parsed.url(), "https://github.com/GZ-GROUP/Minos", "{link}");
        }
        assert_eq!(
            parse_repository_url("https://github.com/a/my.repo_1")
                .unwrap()
                .repo,
            "my.repo_1"
        );
    }

    #[test]
    fn rejects_other_links() {
        for link in [
            "",
            "GZ-GROUP/Minos",
            "git@github.com:GZ-GROUP/Minos.git",
            "http://github.com/GZ-GROUP/Minos",
            "https://gitlab.com/GZ-GROUP/Minos",
            "https://github.com.evil.com/GZ-GROUP/Minos",
            "https://www.github.com/GZ-GROUP/Minos",
            "https://github.com:8443/GZ-GROUP/Minos",
            "https://user:token@github.com/GZ-GROUP/Minos",
            "https://github.com/GZ-GROUP",
            "https://github.com/GZ-GROUP/Minos/tree/main",
            "https://github.com/GZ-GROUP/Minos?tab=readme",
            "https://github.com/GZ-GROUP/Minos#readme",
            "https://github.com/GZ-GROUP/../Minos",
            "https://github.com/GZ-GROUP/%2e%2e",
            "https://github.com/-bad/Minos",
            "https://github.com/a/.git",
        ] {
            let error = parse_repository_url(link).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidRepositoryUrl, "{link}");
        }
    }

    #[test]
    fn only_allows_github_hosts() {
        for allowed in [
            "https://github.com/a/b/releases/download/v1/a.zip",
            "https://api.github.com/repos/a/b",
            "https://objects.githubusercontent.com/x",
            "https://release-assets.githubusercontent.com/x",
        ] {
            assert!(is_github_url(&Url::parse(allowed).unwrap()), "{allowed}");
        }
        for rejected in [
            "http://github.com/a",
            "https://github.com.evil.com/a",
            "https://evilgithubusercontent.com/a",
            "https://user@github.com/a",
            "https://example.com/a",
        ] {
            assert!(!is_github_url(&Url::parse(rejected).unwrap()), "{rejected}");
        }
    }
}
