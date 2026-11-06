//! The `minos-module.json` format (API 1) and its validation. See `docs/modules.md`.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::apps::error::{AppError, AppResult, ErrorCode};

/// Module API version this build of Minos understands.
pub const MINOS_API: u32 = 1;

/// Name of the manifest, at the root of a module's repository and package.
pub const MANIFEST_FILE: &str = "minos-module.json";

/// Apps compiled into Minos (`src/apps/`). A test keeps this list in sync.
pub const INTEGRATED_APP_IDS: &[&str] = &[
    "crop-pdf",
    "merge-pdf",
    "my-disks",
    "my-files",
    "my-network",
    "my-pc",
    "pdf-to-word",
    "ping-pong",
    "processes",
    "share-network",
    "share-scp",
    "word-to-pdf",
];

const RESERVED_WINDOWS_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

const MAX_NAME_CHARS: usize = 60;
const MAX_DESCRIPTION_CHARS: usize = 300;
const MAX_AUTHOR_CHARS: usize = 80;
const MAX_PATH_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

impl Platform {
    /// The system Minos runs on. Its release builds only target these three.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Documents,
    Files,
    System,
    Network,
    Utilities,
}

/// Text per language code. `en` is required; it's the fallback for other languages.
pub type Localized = BTreeMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backend {
    pub exec: BTreeMap<Platform, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub minos_api: u32,
    pub name: Localized,
    pub description: Localized,
    pub author: String,
    pub category: Category,
    pub platforms: Vec<Platform>,
    pub entry: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<Backend>,
}

pub fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidModule, message)
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> AppResult<Self> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| invalid(format!("{MANIFEST_FILE} is not valid: {error}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> AppResult<()> {
        validate_id(&self.id)?;
        semver::Version::parse(&self.version).map_err(|error| {
            invalid(format!(
                "`version` must be a semantic version like 1.0.0: {error}"
            ))
        })?;
        if self.minos_api != MINOS_API {
            return Err(invalid(format!(
                "this module needs Minos module API {}, but this version of Minos supports API {MINOS_API}",
                self.minos_api
            )));
        }
        validate_display(&self.name, &self.description, &self.author, &self.platforms)?;

        validate_relative_path("entry", &self.entry)?;
        if !self.entry.ends_with(".html") {
            return Err(invalid("`entry` must point to an .html file"));
        }
        validate_relative_path("icon", &self.icon)?;
        if !(self.icon.ends_with(".svg") || self.icon.ends_with(".png")) {
            return Err(invalid("`icon` must point to an .svg or .png file"));
        }

        if let Some(backend) = &self.backend {
            for platform in &self.platforms {
                if !backend.exec.contains_key(platform) {
                    return Err(invalid(format!(
                        "`backend.exec` is missing an executable for {}",
                        platform.as_str()
                    )));
                }
            }
            for (platform, path) in &backend.exec {
                if !self.platforms.contains(platform) {
                    return Err(invalid(format!(
                        "`backend.exec` lists {}, which is not in `platforms`",
                        platform.as_str()
                    )));
                }
                validate_relative_path("backend.exec", path)?;
            }
        }
        Ok(())
    }

    pub fn ensure_supported(&self, platform: Platform) -> AppResult<()> {
        if self.platforms.contains(&platform) {
            return Ok(());
        }
        let supported: Vec<_> = self.platforms.iter().map(|p| p.as_str()).collect();
        Err(AppError::new(
            ErrorCode::UnsupportedPlatform,
            format!(
                "\"{}\" only supports {}, and this system is {}",
                self.id,
                supported.join(", "),
                platform.as_str()
            ),
        ))
    }
}

/// Checks the fields shared by a module manifest and a catalog entry.
pub fn validate_display(
    name: &Localized,
    description: &Localized,
    author: &str,
    platforms: &[Platform],
) -> AppResult<()> {
    validate_localized("name", name, MAX_NAME_CHARS)?;
    validate_localized("description", description, MAX_DESCRIPTION_CHARS)?;
    validate_text("author", author, MAX_AUTHOR_CHARS)?;

    if platforms.is_empty() {
        return Err(invalid("`platforms` must list at least one system"));
    }
    let mut seen = HashSet::new();
    for platform in platforms {
        if !seen.insert(platform.as_str()) {
            return Err(invalid(format!(
                "`platforms` lists {} more than once",
                platform.as_str()
            )));
        }
    }
    Ok(())
}

/// An id becomes a folder name, so it must be safe on every system.
pub fn validate_id(id: &str) -> AppResult<()> {
    let well_formed = (2..=40).contains(&id.len())
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        });
    if !well_formed {
        return Err(invalid(format!(
            "`id` \"{id}\" must have 2 to 40 characters: lowercase letters and digits, separated by single hyphens"
        )));
    }
    if RESERVED_WINDOWS_NAMES.contains(&id) {
        return Err(invalid(format!("`id` \"{id}\" is a reserved name")));
    }
    if INTEGRATED_APP_IDS.contains(&id) {
        return Err(invalid(format!(
            "`id` \"{id}\" is already used by an app included in Minos"
        )));
    }
    Ok(())
}

/// Accepts only plain relative paths with forward slashes, on every system.
pub fn validate_relative_path(field: &str, path: &str) -> AppResult<()> {
    let safe = !path.is_empty()
        && path.chars().count() <= MAX_PATH_CHARS
        && !path
            .chars()
            .any(|c| c == '\\' || c == ':' || c.is_control())
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if safe {
        Ok(())
    } else {
        Err(invalid(format!(
            "`{field}` has an unsafe path \"{path}\": use a relative path with forward slashes, without \"..\""
        )))
    }
}

fn validate_localized(field: &str, values: &Localized, max_chars: usize) -> AppResult<()> {
    if !values.contains_key("en") {
        return Err(invalid(format!("`{field}` must include an \"en\" text")));
    }
    for (language, value) in values {
        let valid_language =
            language.len() == 2 && language.bytes().all(|byte| byte.is_ascii_lowercase());
        if !valid_language {
            return Err(invalid(format!(
                "`{field}` has an invalid language code \"{language}\""
            )));
        }
        validate_text(&format!("{field}.{language}"), value, max_chars)?;
    }
    Ok(())
}

fn validate_text(field: &str, value: &str, max_chars: usize) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(invalid(format!("`{field}` must not be empty")));
    }
    if value.chars().count() > max_chars {
        return Err(invalid(format!(
            "`{field}` must have at most {max_chars} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(format!(
            "`{field}` must not contain control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn valid() -> Value {
        json!({
            "id": "hello",
            "version": "1.0.0",
            "minosApi": 1,
            "name": { "en": "Hello", "es": "Hola" },
            "description": { "en": "Example" },
            "author": "Someone",
            "category": "utilities",
            "platforms": ["windows", "linux"],
            "entry": "ui/index.html",
            "icon": "ui/icon.svg",
            "backend": { "exec": { "windows": "bin/a.exe", "linux": "bin/a" } }
        })
    }

    fn parse_with(field: &str, value: Value) -> AppResult<Manifest> {
        let mut manifest = valid();
        manifest[field] = value;
        Manifest::parse(manifest.to_string().as_bytes())
    }

    fn error_for(field: &str, value: Value) -> String {
        parse_with(field, value).unwrap_err().message
    }

    #[test]
    fn accepts_a_valid_manifest() {
        let manifest = parse_with("id", json!("hello-2")).unwrap();
        assert_eq!(manifest.platforms, [Platform::Windows, Platform::Linux]);
    }

    #[test]
    fn backend_is_optional() {
        let mut manifest = valid();
        manifest.as_object_mut().unwrap().remove("backend");
        assert!(Manifest::parse(manifest.to_string().as_bytes()).is_ok());
    }

    #[test]
    fn rejects_invalid_ids() {
        for id in [
            "", "a", "Hello", "my_app", "-a", "a-", "a--b", "../up", "con", "my-files",
        ] {
            assert!(parse_with("id", json!(id)).is_err(), "accepted {id:?}");
        }
    }

    #[test]
    fn rejects_invalid_fields_with_a_clear_message() {
        for (field, value, expected) in [
            ("version", json!("1.0"), "semantic version"),
            ("minosApi", json!(99), "API 99"),
            ("name", json!({ "es": "Hola" }), "\"en\""),
            ("name", json!({ "en": "   " }), "must not be empty"),
            (
                "name",
                json!({ "en": "Hi", "english": "Hi" }),
                "language code",
            ),
            ("author", json!("a\nb"), "control characters"),
            ("category", json!("games"), "unknown variant"),
            ("platforms", json!([]), "at least one"),
            ("platforms", json!(["win"]), "unknown variant"),
            ("platforms", json!(["linux", "linux"]), "more than once"),
            ("entry", json!("ui/app.js"), ".html"),
            ("icon", json!("ui/icon.exe"), ".svg"),
        ] {
            let message = error_for(field, value);
            assert!(message.contains(expected), "{field}: {message}");
        }
    }

    #[test]
    fn rejects_unsafe_paths() {
        for path in [
            "../index.html",
            "ui/../../index.html",
            "/index.html",
            "C:/index.html",
            "ui\\index.html",
            "ui//index.html",
            "./index.html",
        ] {
            let message = error_for("entry", json!(path));
            assert!(message.contains("unsafe path"), "{path}: {message}");
        }
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(error_for("platform", json!(["linux"])).contains("unknown field"));
    }

    #[test]
    fn backend_must_match_platforms() {
        let missing = json!({ "exec": { "linux": "bin/a" } });
        assert!(error_for("backend", missing).contains("missing an executable for windows"));

        let extra = json!({ "exec": {
            "windows": "bin/a.exe", "linux": "bin/a", "macos": "bin/a"
        }});
        assert!(error_for("backend", extra).contains("not in `platforms`"));

        let unsafe_path = json!({ "exec": { "windows": "../a.exe", "linux": "bin/a" } });
        assert!(error_for("backend", unsafe_path).contains("unsafe path"));
    }

    #[test]
    fn ensures_the_platform_is_supported() {
        let manifest = parse_with("id", json!("hello")).unwrap();
        assert!(manifest.ensure_supported(Platform::Linux).is_ok());

        let error = manifest.ensure_supported(Platform::Macos).unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedPlatform);
        assert!(error.message.contains("windows, linux"));
    }

    /// The list above decides which ids modules can't take and which apps can be
    /// installed, so it must match the folders in `src/apps/`.
    #[test]
    fn integrated_ids_match_the_app_folders() {
        let apps = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/apps");
        let mut folders: Vec<String> = std::fs::read_dir(apps)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().join("manifest.ts").exists())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        folders.sort();
        assert_eq!(folders, INTEGRATED_APP_IDS);
    }
}
