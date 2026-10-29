//! PDF ↔ Word conversion.
//!
//! LibreOffice (headless) gives faithful results when installed; otherwise a
//! pure-Rust fallback keeps the text only. Both paths work on every OS.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, StringFormat, dictionary};
use quick_xml::events::Event;
use serde::Serialize;

use super::error::{AppError, AppResult, ErrorCode};
use super::file_io::{FileInfo, file_stem, new_output_dir};

const LIBREOFFICE_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    libreoffice: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionResult {
    file: FileInfo,
    /// "libreoffice" | "basic"
    engine: &'static str,
}

// ── LibreOffice ──────────────────────────────────────────────

fn find_soffice() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["soffice.exe"]
    } else {
        &["soffice", "libreoffice"]
    };
    let on_path = std::env::var_os("PATH").into_iter().flat_map(|paths| {
        std::env::split_paths(&paths)
            .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
            .collect::<Vec<_>>()
    });

    let mut known: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(base) = std::env::var_os(variable) {
                known.push(PathBuf::from(base).join(r"LibreOffice\program\soffice.exe"));
            }
        }
    } else if cfg!(target_os = "macos") {
        known.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
    } else {
        for path in [
            "/usr/bin/soffice",
            "/usr/lib/libreoffice/program/soffice",
            "/opt/libreoffice/program/soffice",
            "/snap/bin/libreoffice",
            "/var/lib/flatpak/exports/bin/org.libreoffice.LibreOffice",
        ] {
            known.push(path.into());
        }
    }

    on_path.chain(known).find(|candidate| candidate.is_file())
}

/// `file:///` URL for LibreOffice's `-env:UserInstallation`, percent-encoded.
fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let encoded: String = text
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// Runs `soffice --convert-to` with a private profile (so an already open
/// LibreOffice window doesn't swallow the job) and a timeout.
fn run_libreoffice(
    soffice: &Path,
    input: &Path,
    out_dir: &Path,
    convert_to: &str,
    infilter: Option<&str>,
    extension: &str,
) -> AppResult<PathBuf> {
    let profile = std::env::temp_dir().join("minos-libreoffice-profile");
    let mut command = Command::new(soffice);
    command
        .arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .args(["--headless", "--norestore", "--nolockcheck", "--nodefault"]);
    if let Some(filter) = infilter {
        command.arg(format!("--infilter={filter}"));
    }
    command
        .args(["--convert-to", convert_to, "--outdir"])
        .arg(out_dir)
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command.spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > LIBREOFFICE_TIMEOUT {
            let _ = child.kill();
            return Err(AppError::new(
                ErrorCode::Timeout,
                "LibreOffice took too long",
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    };

    let output = out_dir.join(format!("{}.{extension}", file_stem(input)));
    if status.success() && output.is_file() {
        Ok(output)
    } else {
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        Err(AppError::new(
            ErrorCode::ConversionFailed,
            format!("LibreOffice couldn't convert the file. {}", stderr.trim()),
        ))
    }
}

// ── Basic fallback: PDF → DOCX (text only) ───────────────────

fn xml_escape(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            other => other.to_string(),
        })
        .collect()
}

/// Minimal WordprocessingML package: one paragraph per line, a page break per PDF page.
fn write_docx(pages: &[Vec<String>], output: &Path) -> AppResult<()> {
    let mut body = String::new();
    for (index, lines) in pages.iter().enumerate() {
        if index > 0 {
            body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
        }
        for line in lines {
            body.push_str(&format!(
                r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                xml_escape(line)
            ));
        }
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr/></w:body></w:document>"#
    );
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

    let mut zip = zip::ZipWriter::new(fs::File::create(output)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let zip_error = |error: zip::result::ZipError| AppError::new(ErrorCode::Io, error.to_string());
    for (name, content) in [
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", rels),
        ("word/document.xml", document.as_str()),
    ] {
        zip.start_file(name, options).map_err(zip_error)?;
        zip.write_all(content.as_bytes())?;
    }
    zip.finish().map_err(zip_error)?;
    Ok(())
}

fn basic_pdf_to_docx(input: &Path, output: &Path) -> AppResult<()> {
    let document = Document::load(input)?;
    let pages: Vec<Vec<String>> = (1..=document.get_pages().len() as u32)
        .map(|page| {
            document
                .extract_text(&[page])
                .unwrap_or_default()
                .lines()
                .map(|line| line.trim_end().to_string())
                .collect()
        })
        .collect();
    if pages.iter().flatten().all(|line| line.trim().is_empty()) {
        return Err(AppError::new(
            ErrorCode::ConversionFailed,
            "the PDF has no extractable text (it may be scanned images)",
        ));
    }
    write_docx(&pages, output)
}

// ── Basic fallback: DOCX → PDF (text only) ───────────────────

/// A paragraph of text, or a forced page break.
#[derive(Debug, PartialEq)]
enum Block {
    Paragraph(String),
    PageBreak,
}

fn read_docx_blocks(input: &Path) -> AppResult<Vec<Block>> {
    let invalid = |message: String| AppError::new(ErrorCode::UnsupportedFormat, message);
    let mut archive =
        zip::ZipArchive::new(fs::File::open(input)?).map_err(|e| invalid(e.to_string()))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| invalid(e.to_string()))?
        .read_to_string(&mut xml)?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(|e| invalid(e.to_string()))? {
            Event::Start(tag) if tag.local_name().as_ref() == "t" => in_text = true,
            Event::End(tag) if tag.local_name().as_ref() == "t" => in_text = false,
            Event::End(tag) if tag.local_name().as_ref() == "p" => {
                let text = std::mem::take(&mut paragraph);
                // The paragraph that only holds a page break adds no blank line on the new page.
                if !(text.is_empty() && blocks.last() == Some(&Block::PageBreak)) {
                    blocks.push(Block::Paragraph(text));
                }
            }
            Event::Empty(tag) => match tag.local_name().as_ref() {
                "tab" => paragraph.push('\t'),
                "br" => {
                    let page = tag
                        .attributes()
                        .flatten()
                        .any(|a| a.value.as_ref() == "page");
                    if page {
                        if !paragraph.is_empty() {
                            blocks.push(Block::Paragraph(std::mem::take(&mut paragraph)));
                        }
                        blocks.push(Block::PageBreak);
                    } else {
                        paragraph.push('\n');
                    }
                }
                _ => {}
            },
            Event::Text(text) if in_text => paragraph.push_str(text.as_ref()),
            Event::GeneralRef(reference) if in_text => {
                if let Ok(Some(character)) = reference.resolve_char_ref() {
                    paragraph.push(character);
                } else {
                    paragraph.push_str(match reference.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(blocks)
}

/// Helvetica advance widths (1/1000 em) for ASCII 32..=126, from the standard AFM.
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

fn text_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| match c as u32 {
            32..=126 => HELVETICA_WIDTHS[c as usize - 32],
            _ => 556,
        } as f32)
        .sum::<f32>()
        * size
        / 1000.0
}

/// Encodes text for a WinAnsi (Windows-1252) Type1 font; unsupported characters become "?".
fn win_ansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            ' '..='~' | '\u{A0}'..='\u{FF}' => c as u8,
            '€' => 0x80,
            '‚' => 0x82,
            '„' => 0x84,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '\t' => b' ',
            _ => b'?',
        })
        .collect()
}

/// Greedy word wrap; words wider than a line are split.
fn wrap(text: &str, size: f32, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        let mut line = String::new();
        for word in raw_line.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if text_width(&candidate, size) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            let mut piece = String::new();
            for c in word.chars() {
                if text_width(&format!("{piece}{c}"), size) > max_width && !piece.is_empty() {
                    lines.push(std::mem::take(&mut piece));
                }
                piece.push(c);
            }
            line = piece;
        }
        lines.push(line);
    }
    lines
}

fn write_text_pdf(blocks: &[Block], output: &Path) -> AppResult<()> {
    const PAGE: (f32, f32) = (595.0, 842.0); // A4 in points
    const MARGIN: f32 = 64.0;
    const SIZE: f32 = 11.0;
    const LEADING: f32 = 15.0;
    const PARAGRAPH_GAP: f32 = 6.0;

    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let resources_id =
        document.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });

    let mut pages: Vec<Vec<Operation>> = vec![Vec::new()];
    let mut y = PAGE.1 - MARGIN;
    let line_op = |x: f32, y: f32, text: &str| {
        vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), SIZE.into()]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(win_ansi(text), StringFormat::Literal)],
            ),
            Operation::new("ET", vec![]),
        ]
    };

    for block in blocks {
        match block {
            Block::PageBreak => {
                pages.push(Vec::new());
                y = PAGE.1 - MARGIN;
            }
            Block::Paragraph(text) => {
                for line in wrap(text, SIZE, PAGE.0 - 2.0 * MARGIN) {
                    if y < MARGIN {
                        pages.push(Vec::new());
                        y = PAGE.1 - MARGIN;
                    }
                    if !line.is_empty() {
                        pages.last_mut().unwrap().extend(line_op(MARGIN, y, &line));
                    }
                    y -= LEADING;
                }
                y -= PARAGRAPH_GAP;
            }
        }
    }

    let kids: Vec<Object> = pages
        .into_iter()
        .map(|operations| {
            let encoded = Content { operations }
                .encode()
                .map_err(|e| AppError::new(ErrorCode::ConversionFailed, e.to_string()))?;
            let content_id = document.add_object(Stream::new(dictionary! {}, encoded));
            Ok(document
                .add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content_id })
                .into())
        })
        .collect::<AppResult<_>>()?;
    let count = kids.len() as u32;
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count, "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), PAGE.0.into(), PAGE.1.into()],
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    document.compress();
    document.save(output)?;
    Ok(())
}

fn basic_docx_to_pdf(input: &Path, output: &Path) -> AppResult<()> {
    let is_docx = input
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("docx"));
    if !is_docx {
        return Err(AppError::new(
            ErrorCode::UnsupportedFormat,
            "without LibreOffice only .docx files can be converted",
        ));
    }
    write_text_pdf(&read_docx_blocks(input)?, output)
}

// ── Commands ─────────────────────────────────────────────────

/// LibreOffice first; on failure (or when missing) the basic converter.
fn convert(
    input: &str,
    extension: &'static str,
    convert_to: &str,
    infilter: Option<&str>,
    basic: fn(&Path, &Path) -> AppResult<()>,
) -> AppResult<ConversionResult> {
    let input = Path::new(input);
    let out_dir = new_output_dir()?;

    let libreoffice_error = match find_soffice() {
        Some(soffice) => {
            match run_libreoffice(&soffice, input, &out_dir, convert_to, infilter, extension) {
                Ok(output) => {
                    return Ok(ConversionResult {
                        file: FileInfo::read(&output)?,
                        engine: "libreoffice",
                    });
                }
                Err(error) => Some(error),
            }
        }
        None => None,
    };

    let output = out_dir.join(format!("{}.{extension}", file_stem(input)));
    match basic(input, &output) {
        Ok(()) => Ok(ConversionResult {
            file: FileInfo::read(&output)?,
            engine: "basic",
        }),
        // Prefer LibreOffice's error: it's the engine the user expected to run.
        Err(error) => Err(libreoffice_error.unwrap_or(error)),
    }
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|error| AppError::new(ErrorCode::Io, error.to_string()))?
}

#[tauri::command]
pub async fn convert_engine() -> AppResult<EngineInfo> {
    blocking(|| {
        Ok(EngineInfo {
            libreoffice: find_soffice().is_some(),
        })
    })
    .await
}

#[tauri::command]
pub async fn convert_pdf_to_word(path: String) -> AppResult<ConversionResult> {
    blocking(move || {
        convert(
            &path,
            "docx",
            r#"docx:MS Word 2007 XML"#,
            Some("writer_pdf_import"),
            basic_pdf_to_docx,
        )
    })
    .await
}

#[tauri::command]
pub async fn convert_word_to_pdf(path: String) -> AppResult<ConversionResult> {
    blocking(move || convert(&path, "pdf", "pdf", None, basic_docx_to_pdf)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_round_trip_through_basic_converters() {
        let dir = tempfile::tempdir().unwrap();
        let docx = dir.path().join("in.docx");
        write_docx(
            &[
                vec!["Hola señor & <amigo>".into(), "Second line".into()],
                vec!["Page two".into()],
            ],
            &docx,
        )
        .unwrap();

        let blocks = read_docx_blocks(&docx).unwrap();
        assert_eq!(
            blocks,
            vec![
                Block::Paragraph("Hola señor & <amigo>".into()),
                Block::Paragraph("Second line".into()),
                Block::PageBreak,
                Block::Paragraph("Page two".into()),
            ]
        );

        let pdf = dir.path().join("out.pdf");
        basic_docx_to_pdf(&docx, &pdf).unwrap();
        let document = Document::load(&pdf).unwrap();
        assert_eq!(document.get_pages().len(), 2);
        assert!(document.extract_text(&[2]).unwrap().contains("Page two"));

        let back = dir.path().join("back.docx");
        basic_pdf_to_docx(&pdf, &back).unwrap();
        assert!(fs::metadata(&back).unwrap().len() > 0);
    }

    #[test]
    fn wraps_long_text_and_encodes_win_ansi() {
        let lines = wrap(&"word ".repeat(200), 11.0, 467.0);
        assert!(lines.len() > 5);
        assert!(lines.iter().all(|line| text_width(line, 11.0) <= 467.0));
        assert_eq!(win_ansi("ñ€ 漢"), vec![0xF1, 0x80, b' ', b'?']);
    }

    #[test]
    fn builds_file_urls() {
        assert_eq!(file_url(Path::new("/tmp/a b")), "file:///tmp/a%20b");
        assert_eq!(file_url(Path::new(r"C:\Temp\x")), "file:///C:/Temp/x");
    }
}
