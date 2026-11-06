//! Validates a module package (.zip) without extracting it.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use zip::ZipArchive;

use super::manifest::{MANIFEST_FILE, Manifest, Platform, invalid, validate_relative_path};
use crate::apps::error::AppResult;

/// Largest package accepted, compressed.
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
/// Largest manifest accepted, also when fetched from a repository.
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;

/// Checks a package's structure, limits and integrity and returns its manifest.
/// Every entry is decompressed to a sink, so nothing is written to disk.
pub fn validate_package(path: &Path, platform: Platform) -> AppResult<Manifest> {
    if File::open(path)?.metadata()?.len() > MAX_PACKAGE_BYTES {
        return Err(invalid(format!(
            "the package is larger than the {} MB limit",
            MAX_PACKAGE_BYTES / (1024 * 1024)
        )));
    }

    let mut archive = ZipArchive::new(File::open(path)?)
        .map_err(|error| invalid(format!("the file is not a valid .zip: {error}")))?;
    if archive.len() > MAX_ENTRIES {
        return Err(invalid(format!(
            "the package has more than {MAX_ENTRIES} entries"
        )));
    }

    let files = read_entries(&mut archive)?;
    let manifest = read_manifest(&mut archive, &files)?;
    manifest.ensure_supported(platform)?;

    let mut required = vec![("entry", manifest.entry.as_str())];
    required.push(("icon", manifest.icon.as_str()));
    if let Some(executable) = manifest
        .backend
        .as_ref()
        .and_then(|backend| backend.exec.get(&platform))
    {
        required.push(("backend.exec", executable));
    }
    for (field, path) in required {
        if !files.contains(path) {
            return Err(invalid(format!(
                "`{field}` points to \"{path}\", which is not in the package"
            )));
        }
    }
    Ok(manifest)
}

/// Checks every entry and returns the names of the regular files.
fn read_entries(archive: &mut ZipArchive<File>) -> AppResult<HashSet<String>> {
    let mut files = HashSet::new();
    let mut seen = HashSet::new();
    let mut unpacked = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| invalid(format!("the package can't be read: {error}")))?;
        let name = entry
            .name()
            .map_err(|error| invalid(format!("the package has an invalid entry name: {error}")))?
            .into_owned();

        let path = name.strip_suffix('/').unwrap_or(&name);
        validate_relative_path("entry name", path)
            .map_err(|_| invalid(format!("the package has an unsafe path: \"{name}\"")))?;
        if entry.is_symlink() {
            return Err(invalid(format!(
                "the package has a symbolic link, which is not allowed: \"{name}\""
            )));
        }
        // Would overwrite each other once extracted on a case-insensitive disk.
        if !seen.insert(path.to_lowercase()) {
            return Err(invalid(format!(
                "the package has a duplicated path: \"{name}\""
            )));
        }
        if entry.is_dir() {
            continue;
        }

        // Limited to the real output, not the size each entry declares.
        let remaining = MAX_UNPACKED_BYTES - unpacked;
        let read = io::copy(&mut (&mut entry).take(remaining + 1), &mut io::sink())
            .map_err(|error| invalid(format!("\"{name}\" is corrupted: {error}")))?;
        if read > remaining {
            return Err(invalid(format!(
                "the package unpacks to more than {} GB",
                MAX_UNPACKED_BYTES / (1024 * 1024 * 1024)
            )));
        }
        unpacked += read;
        files.insert(path.to_string());
    }
    Ok(files)
}

fn read_manifest(archive: &mut ZipArchive<File>, files: &HashSet<String>) -> AppResult<Manifest> {
    if !files.contains(MANIFEST_FILE) {
        let nested = files
            .iter()
            .find(|name| name.ends_with(&format!("/{MANIFEST_FILE}")));
        return Err(match nested {
            Some(path) => invalid(format!(
                "{MANIFEST_FILE} must be at the root of the .zip, but it is at \"{path}\""
            )),
            None => invalid(format!("the package has no {MANIFEST_FILE}")),
        });
    }

    let entry = archive
        .by_name(MANIFEST_FILE)
        .map_err(|error| invalid(format!("{MANIFEST_FILE} can't be read: {error}")))?;
    let mut bytes = Vec::new();
    entry
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| invalid(format!("{MANIFEST_FILE} can't be read: {error}")))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(invalid(format!(
            "{MANIFEST_FILE} is larger than {} KB",
            MAX_MANIFEST_BYTES / 1024
        )));
    }
    Manifest::parse(&bytes)
}

#[cfg(test)]
pub(super) mod test_support {
    use std::io::Write;
    use std::path::Path;

    use zip::write::SimpleFileOptions;

    /// Writes a package with the given entries, in order.
    pub fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, contents) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
    }

    pub fn manifest_json(id: &str, version: &str, platforms: &[&str]) -> String {
        serde_json::json!({
            "id": id,
            "version": version,
            "minosApi": 1,
            "name": { "en": "Hello" },
            "description": { "en": "Example" },
            "author": "Someone",
            "category": "utilities",
            "platforms": platforms,
            "entry": "ui/index.html",
            "icon": "ui/icon.svg"
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::test_support::{manifest_json, write_zip};
    use super::*;
    use crate::apps::error::ErrorCode;

    const LINUX: Platform = Platform::Linux;

    fn check(entries: &[(&str, &[u8])]) -> AppResult<Manifest> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.zip");
        write_zip(&path, entries);
        validate_package(&path, LINUX)
    }

    fn manifest() -> String {
        manifest_json("hello", "1.0.0", &["linux"])
    }

    fn with_extra(extra: (&'static str, &'static [u8])) -> String {
        let manifest = manifest();
        check(&[
            ("minos-module.json", manifest.as_bytes()),
            ("ui/index.html", b"<html></html>"),
            ("ui/icon.svg", b"<svg/>"),
            extra,
        ])
        .unwrap_err()
        .message
    }

    #[test]
    fn accepts_a_valid_package() {
        let manifest = manifest();
        let parsed = check(&[
            ("minos-module.json", manifest.as_bytes()),
            ("ui/index.html", b"<html></html>"),
            ("ui/icon.svg", b"<svg/>"),
        ])
        .unwrap();
        assert_eq!(parsed.id, "hello");
    }

    #[test]
    fn rejects_a_file_that_is_not_a_zip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.zip");
        std::fs::write(&path, b"not a zip").unwrap();
        let error = validate_package(&path, LINUX).unwrap_err();
        assert!(error.message.contains("not a valid .zip"));
    }

    #[test]
    fn rejects_unsafe_paths() {
        for name in [
            "../evil.txt",
            "ui/../../evil.txt",
            "/etc/evil.txt",
            "C:/evil.txt",
            "ui\\..\\evil.txt",
        ] {
            let manifest = manifest();
            let message = check(&[
                ("minos-module.json", manifest.as_bytes()),
                ("ui/index.html", b""),
                ("ui/icon.svg", b""),
                (name, b"x"),
            ])
            .unwrap_err()
            .message;
            assert!(message.contains("unsafe path"), "{name}: {message}");
        }
    }

    #[test]
    fn rejects_duplicates_ignoring_case() {
        assert!(with_extra(("UI/INDEX.html", b"x")).contains("duplicated path"));
    }

    #[test]
    fn rejects_symbolic_links() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file("minos-module.json", options).unwrap();
        zip.write_all(manifest().as_bytes()).unwrap();
        zip.add_symlink("ui/index.html", "/etc/passwd", options)
            .unwrap();
        zip.finish().unwrap();

        let error = validate_package(&path, LINUX).unwrap_err();
        assert!(error.message.contains("symbolic link"), "{}", error.message);
    }

    #[test]
    fn explains_a_missing_or_nested_manifest() {
        let error = check(&[("ui/index.html", b"")]).unwrap_err();
        assert!(error.message.contains("has no minos-module.json"));

        let manifest = manifest();
        let error = check(&[("hello/minos-module.json", manifest.as_bytes())]).unwrap_err();
        assert!(error.message.contains("must be at the root"));
    }

    #[test]
    fn requires_the_files_the_manifest_points_to() {
        let manifest = manifest();
        let error = check(&[
            ("minos-module.json", manifest.as_bytes()),
            ("ui/index.html", b""),
        ])
        .unwrap_err();
        assert!(error.message.contains("`icon` points to"));
    }

    #[test]
    fn requires_the_executable_for_this_system_only() {
        let mut value: serde_json::Value =
            serde_json::from_str(&manifest_json("hello", "1.0.0", &["linux", "windows"])).unwrap();
        value["backend"] = serde_json::json!({
            "exec": { "linux": "bin/app", "windows": "bin/app.exe" }
        });
        let manifest = value.to_string();
        let base: [(&str, &[u8]); 3] = [
            ("minos-module.json", manifest.as_bytes()),
            ("ui/index.html", b""),
            ("ui/icon.svg", b""),
        ];

        let error = check(&base).unwrap_err();
        assert!(
            error
                .message
                .contains("`backend.exec` points to \"bin/app\"")
        );

        let mut with_exec = base.to_vec();
        with_exec.push(("bin/app", b""));
        assert!(check(&with_exec).is_ok());
    }

    #[test]
    fn rejects_a_package_for_another_system() {
        let manifest = manifest_json("hello", "1.0.0", &["windows"]);
        let error = check(&[
            ("minos-module.json", manifest.as_bytes()),
            ("ui/index.html", b""),
            ("ui/icon.svg", b""),
        ])
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedPlatform);
    }

    #[test]
    fn rejects_an_oversized_manifest() {
        let padding = vec![b' '; MAX_MANIFEST_BYTES as usize + 1];
        let error = check(&[("minos-module.json", padding.as_slice())]).unwrap_err();
        assert!(error.message.contains("larger than"), "{}", error.message);
    }

    #[test]
    fn rejects_too_many_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("module.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for index in 0..=MAX_ENTRIES {
            zip.start_file(format!("f{index}"), SimpleFileOptions::default())
                .unwrap();
        }
        zip.finish().unwrap();

        let error = validate_package(&path, LINUX).unwrap_err();
        assert!(error.message.contains("more than"), "{}", error.message);
    }
}
