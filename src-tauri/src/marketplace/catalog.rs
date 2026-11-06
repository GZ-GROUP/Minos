//! The list of modules offered in the Marketplace (`marketplace/catalog.json`).

use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::manifest::{Category, Localized, Platform, invalid, validate_display, validate_id};
use super::source::{GitHub, parse_repository_url};
use crate::apps::error::AppResult;

const SCHEMA: u32 = 1;
const REMOTE_PATH: &str = "GZ-GROUP/Minos/main/marketplace/catalog.json";
const BUNDLED: &str = include_str!("../../../marketplace/catalog.json");
const MAX_BYTES: u64 = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub repo: String,
    pub name: Localized,
    pub description: Localized,
    pub author: String,
    pub category: Category,
    pub platforms: Vec<Platform>,
}

impl CatalogEntry {
    fn validate(&self) -> AppResult<()> {
        validate_id(&self.id)?;
        parse_repository_url(&self.repo)?;
        validate_display(&self.name, &self.description, &self.author, &self.platforms)
    }
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    schema: u32,
    modules: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub modules: Vec<CatalogEntry>,
    /// True when the online catalog couldn't be read and the bundled copy is shown.
    pub offline: bool,
}

/// Parses a catalog, dropping entries that aren't valid so that one bad entry (or
/// one written for a newer Minos) doesn't hide the rest. Returns the dropped ones.
fn parse(bytes: &[u8]) -> AppResult<(Vec<CatalogEntry>, Vec<String>)> {
    let file: CatalogFile = serde_json::from_slice(bytes)
        .map_err(|error| invalid(format!("the catalog is not valid: {error}")))?;
    if file.schema != SCHEMA {
        return Err(invalid(format!(
            "unsupported catalog schema {}",
            file.schema
        )));
    }

    let mut entries = Vec::new();
    let mut rejected = Vec::new();
    let mut ids = HashSet::new();
    for value in file.modules {
        let parsed = serde_json::from_value::<CatalogEntry>(value)
            .map_err(|error| error.to_string())
            .and_then(|entry| {
                entry.validate().map_err(|error| error.message)?;
                Ok(entry)
            });
        match parsed {
            Ok(entry) if ids.insert(entry.id.clone()) => entries.push(entry),
            Ok(entry) => rejected.push(format!("duplicated id \"{}\"", entry.id)),
            Err(reason) => rejected.push(reason),
        }
    }
    Ok((entries, rejected))
}

fn bundled() -> Vec<CatalogEntry> {
    parse(BUNDLED.as_bytes())
        .map(|(entries, _)| entries)
        .unwrap_or_default()
}

/// Reads the online catalog, falling back to the copy shipped with this build.
pub async fn load(github: &GitHub) -> Catalog {
    let remote = github.fetch_raw(REMOTE_PATH, MAX_BYTES, TIMEOUT).await;
    match remote.and_then(|bytes| parse(&bytes)) {
        Ok((modules, _)) => Catalog {
            modules,
            offline: false,
        },
        Err(_) => Catalog {
            modules: bundled(),
            offline: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entry(id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "repo": "https://github.com/someone/module",
            "name": { "en": "Module" },
            "description": { "en": "Does things" },
            "author": "Someone",
            "category": "utilities",
            "platforms": ["linux"]
        })
    }

    /// Guards pull requests that add modules: CI runs this over the real file.
    #[test]
    fn the_bundled_catalog_is_valid() {
        let (_, rejected) = parse(BUNDLED.as_bytes()).unwrap();
        assert!(rejected.is_empty(), "invalid entries: {rejected:?}");
    }

    #[test]
    fn drops_invalid_entries_and_keeps_the_rest() {
        let mut bad_repo = entry("bad-repo");
        bad_repo["repo"] = json!("https://example.com/a/b");
        let mut bad_category = entry("bad-category");
        bad_category["category"] = json!("games");

        let file = json!({
            "schema": 1,
            "modules": [
                entry("good"), entry("good"), bad_repo, bad_category, entry("my-files")
            ]
        });
        let (entries, rejected) = parse(file.to_string().as_bytes()).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "good");
        assert_eq!(rejected.len(), 4);
    }

    #[test]
    fn rejects_an_unknown_schema() {
        let file = json!({ "schema": 2, "modules": [] });
        assert!(parse(file.to_string().as_bytes()).is_err());
    }

    #[test]
    fn falls_back_to_the_bundled_copy_when_the_download_fails() {
        tauri::async_runtime::block_on(async {
            // Nothing listens on this port, so the connection is refused.
            let github = super::super::source::test_support::local_github("http://127.0.0.1:9");
            let catalog = load(&github).await;
            assert!(catalog.offline);
        });
    }
}
