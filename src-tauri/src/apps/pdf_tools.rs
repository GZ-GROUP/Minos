//! Crop (keep a selection of pages) and merge PDFs with `lopdf` — pure Rust, so it
//! behaves the same on every OS.

use std::collections::BTreeMap;
use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

use super::error::{AppError, AppResult, ErrorCode};
use super::file_io::{FileInfo, file_stem, new_output_dir};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfInfo {
    pages: u32,
}

/// Parses a page pattern such as `"1-5, 8, 11-13"` into sorted, unique page
/// numbers (1-based). Every page must exist in a document of `page_count` pages.
pub fn parse_pages(pattern: &str, page_count: u32) -> AppResult<Vec<u32>> {
    let invalid = |message: String| AppError::new(ErrorCode::InvalidInput, message);
    let mut pages = Vec::new();

    for part in pattern
        .split([',', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (start, end) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let parse = |value: &str| {
            value
                .parse::<u32>()
                .map_err(|_| invalid(format!("\"{part}\" is not a page or range")))
        };
        let (start, end) = (parse(start)?, parse(end)?);
        if start == 0 || end == 0 || start > end {
            return Err(invalid(format!("\"{part}\" is not a valid range")));
        }
        if end > page_count {
            return Err(invalid(format!(
                "page {end} doesn't exist (the document has {page_count})"
            )));
        }
        pages.extend(start..=end);
    }

    if pages.is_empty() {
        return Err(invalid("no pages selected".into()));
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

fn load(path: &str) -> AppResult<Document> {
    let document = Document::load(path)?;
    if document.is_encrypted() {
        return Err(AppError::new(
            ErrorCode::InvalidPdf,
            "the PDF is password-protected",
        ));
    }
    Ok(document)
}

#[tauri::command]
pub async fn pdf_info(path: String) -> AppResult<PdfInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(PdfInfo {
            pages: load(&path)?.get_pages().len() as u32,
        })
    })
    .await
    .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?
}

#[tauri::command]
pub async fn pdf_crop(path: String, pattern: String) -> AppResult<FileInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut document = load(&path)?;
        let total = document.get_pages().len() as u32;
        let keep = parse_pages(&pattern, total)?;
        let remove: Vec<u32> = (1..=total).filter(|page| !keep.contains(page)).collect();
        document.delete_pages(&remove);
        document.prune_objects();

        let output =
            new_output_dir()?.join(format!("{} (cropped).pdf", file_stem(Path::new(&path))));
        document.save(&output)?;
        FileInfo::read(&output)
    })
    .await
    .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?
}

/// Keys a page may inherit from its ancestors in the page tree (PDF 32000 §7.7.3.4).
const INHERITABLE: &[&[u8]] = &[b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Copies inherited attributes onto the page so it survives being re-parented.
fn with_inherited(document: &Document, page_id: ObjectId) -> AppResult<Dictionary> {
    let mut page = document.get_dictionary(page_id)?.clone();
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    while let Some(parent_id) = parent {
        let node = document.get_dictionary(parent_id)?;
        for key in INHERITABLE {
            if !page.has(key)
                && let Ok(value) = node.get(key)
            {
                page.set(*key, value.clone());
            }
        }
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Ok(page)
}

/// Merges documents in the given order into a single page tree.
pub fn merge(documents: Vec<Document>) -> AppResult<Document> {
    let mut merged = Document::with_version("1.7");
    let mut objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    // A Vec (not a map keyed by object id) keeps the original page order.
    let mut pages: Vec<(ObjectId, Dictionary)> = Vec::new();
    let mut next_id = 1;

    for mut document in documents {
        document.renumber_objects_with(next_id);
        next_id = document.max_id + 1;
        for page_id in document.get_pages().into_values() {
            pages.push((page_id, with_inherited(&document, page_id)?));
        }
        for (id, object) in document.objects {
            // Page tree nodes, catalogs and outlines are rebuilt below.
            match object.type_name().unwrap_or(b"") {
                b"Catalog" | b"Pages" | b"Page" | b"Outlines" | b"Outline" => {}
                _ => {
                    objects.insert(id, object);
                }
            }
        }
    }

    if pages.is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidPdf,
            "the documents have no pages",
        ));
    }

    merged.objects = objects;
    merged.max_id = next_id;
    let pages_id = merged.new_object_id();
    let kids: Vec<Object> = pages.iter().map(|(id, _)| Object::Reference(*id)).collect();
    let count = pages.len() as u32;
    for (id, mut page) in pages {
        page.set("Parent", pages_id);
        merged.objects.insert(id, Object::Dictionary(page));
    }
    merged.objects.insert(
        pages_id,
        Object::Dictionary(
            lopdf::dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count },
        ),
    );
    let catalog_id =
        merged.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    merged.trailer.set("Root", catalog_id);
    merged.renumber_objects();
    merged.prune_objects();
    Ok(merged)
}

#[tauri::command]
pub async fn pdf_merge(paths: Vec<String>) -> AppResult<FileInfo> {
    if paths.len() < 2 {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "choose at least two PDFs",
        ));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let documents = paths
            .iter()
            .map(|path| load(path))
            .collect::<AppResult<Vec<_>>>()?;
        let mut merged = merge(documents)?;
        let output = new_output_dir()?.join("merged.pdf");
        merged.compress();
        merged.save(&output)?;
        FileInfo::read(&output)
    })
    .await
    .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{Stream, dictionary};

    /// Builds a document whose pages each print their label.
    fn sample(labels: &[&str]) -> Document {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        // Resources live on the Pages node so merging must copy them down (inheritance).
        let resources_id =
            document.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
        let kids: Vec<Object> = labels
            .iter()
            .map(|label| {
                let content = Content {
                    operations: vec![
                        Operation::new("BT", vec![]),
                        Operation::new("Tf", vec!["F1".into(), 24.into()]),
                        Operation::new("Td", vec![72.into(), 720.into()]),
                        Operation::new("Tj", vec![Object::string_literal(*label)]),
                        Operation::new("ET", vec![]),
                    ],
                };
                let content_id = document.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
                document
                    .add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content_id })
                    .into()
            })
            .collect();
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => labels.len() as u32,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );
        let catalog_id =
            document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog_id);
        document
    }

    fn page_texts(document: &Document) -> Vec<String> {
        (1..=document.get_pages().len() as u32)
            .map(|page| document.extract_text(&[page]).unwrap().trim().to_string())
            .collect()
    }

    #[test]
    fn parses_page_patterns() {
        assert_eq!(parse_pages("1-3, 5", 6).unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(parse_pages(" 4 ,2-2; 4", 4).unwrap(), vec![2, 4]);
        for bad in ["", "0", "3-1", "a", "1-", "7", "2-9"] {
            assert!(parse_pages(bad, 6).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn crops_to_selected_pages() {
        let mut document = sample(&["one", "two", "three", "four"]);
        document.delete_pages(&[2, 4]);
        document.prune_objects();
        assert_eq!(page_texts(&document), vec!["one", "three"]);
    }

    #[test]
    fn merges_in_order_with_inherited_resources() {
        let merged = merge(vec![
            sample(&["a1", "a2"]),
            sample(&["b1"]),
            sample(&["c1", "c2"]),
        ])
        .unwrap();
        let mut bytes = Vec::new();
        let mut merged = merged;
        merged.save_to(&mut bytes).unwrap();
        let reloaded = Document::load_mem(&bytes).unwrap();
        assert_eq!(page_texts(&reloaded), vec!["a1", "a2", "b1", "c1", "c2"]);
        let first = *reloaded.get_pages().get(&1).unwrap();
        assert!(reloaded.get_dictionary(first).unwrap().has(b"Resources"));
    }
}
