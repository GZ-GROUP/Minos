//! PDF ↔ Word conversion.
//!
//! Both directions use the PDF engine (`pdf-engine/`), which ships with the app. If it
//! can't run or fails on a file, a pure-Rust fallback keeps the text only.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, StringFormat, dictionary};
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::error::{AppError, AppResult, ErrorCode};
use super::file_io::{FileInfo, file_stem, new_output_dir};

const PDF_ENGINE_TIMEOUT: Duration = Duration::from_secs(300);
// One bundle per direction, since their libraries conflict; see `pdf-engine/build.py`.
const TO_DOCX: &str = "minos-to-docx";
const TO_PDF: &str = "minos-to-pdf";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pdf_engine: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionResult {
    file: FileInfo,
    /// "minos" | "basic"
    engine: &'static str,
}

/// Waits for a converter, killing it if it takes too long.
fn wait_with_timeout(child: &mut Child, timeout: Duration, name: &str) -> AppResult<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            return Err(AppError::new(
                ErrorCode::Timeout,
                format!("{name} took too long"),
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

// ── PDF engine ───────────────────────────────────────────────

/// Where the engine may be: packaged with the app, or built for development by
/// `pdf-engine/build.py`.
fn pdf_engine_candidates(resource_dir: Option<PathBuf>, name: &str) -> Vec<PathBuf> {
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let bundle = Path::new("pdf-engine-dist")
        .join("minos-pdf")
        .join(name)
        .join(executable);
    let mut candidates: Vec<PathBuf> = resource_dir
        .into_iter()
        .map(|dir| dir.join(&bundle))
        .collect();
    if cfg!(debug_assertions) {
        candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join(&bundle));
    }
    candidates
}

fn find_pdf_engine(app: &AppHandle, name: &str) -> Option<PathBuf> {
    pdf_engine_candidates(app.path().resource_dir().ok(), name)
        .into_iter()
        .find(|path| path.is_file())
}

#[derive(Debug, Deserialize)]
struct PdfEngineReport {
    ok: bool,
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

/// The engine reports on its last line of stdout; libraries may log above it.
fn parse_pdf_engine_report(stdout: &str) -> AppResult<()> {
    let report = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str::<PdfEngineReport>(line).ok())
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::ConversionFailed,
                "the PDF engine didn't report a result",
            )
        })?;
    if report.ok {
        return Ok(());
    }
    let code = match report.code.as_str() {
        "invalid_pdf" => ErrorCode::InvalidPdf,
        "unsupported_format" => ErrorCode::UnsupportedFormat,
        _ => ErrorCode::ConversionFailed,
    };
    Err(AppError::new(code, report.message))
}

fn run_pdf_engine(engine: &Path, conversion: &str, input: &Path, output: &Path) -> AppResult<()> {
    let mut command = Command::new(engine);
    command
        .arg(conversion)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console(&mut command);

    let mut child = command.spawn()?;
    // Read while it runs: a full pipe would otherwise block it until the timeout.
    let mut pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = pipe.as_mut() {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    });
    wait_with_timeout(&mut child, PDF_ENGINE_TIMEOUT, "The PDF engine")?;
    let stdout = reader.join().unwrap_or_default();

    parse_pdf_engine_report(&String::from_utf8_lossy(&stdout))?;
    if output.is_file() {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::ConversionFailed,
            "the PDF engine didn't write the document",
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
            "only .docx files can be converted; save it as .docx first",
        ));
    }
    write_text_pdf(&read_docx_blocks(input)?, output)
}

// ── Commands ─────────────────────────────────────────────────

/// The PDF engine; on failure (or when it's missing) the basic converter.
fn convert(
    input: &str,
    extension: &'static str,
    conversion: &str,
    pdf_engine: Option<&Path>,
    basic: fn(&Path, &Path) -> AppResult<()>,
) -> AppResult<ConversionResult> {
    let input = Path::new(input);
    let out_dir = new_output_dir()?;
    let output = out_dir.join(format!("{}.{extension}", file_stem(input)));

    let pdf_engine_error = match pdf_engine {
        Some(engine) => match run_pdf_engine(engine, conversion, input, &output) {
            Ok(()) => {
                return Ok(ConversionResult {
                    file: FileInfo::read(&output)?,
                    engine: "minos",
                });
            }
            Err(error) => Some(error),
        },
        None => None,
    };

    match basic(input, &output) {
        Ok(()) => Ok(ConversionResult {
            file: FileInfo::read(&output)?,
            engine: "basic",
        }),
        // Prefer the error of the engine the user expected to run.
        Err(error) => Err(pdf_engine_error.unwrap_or(error)),
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
pub async fn convert_engine(app: AppHandle) -> AppResult<EngineInfo> {
    blocking(move || {
        Ok(EngineInfo {
            pdf_engine: find_pdf_engine(&app, TO_DOCX).is_some()
                && find_pdf_engine(&app, TO_PDF).is_some(),
        })
    })
    .await
}

#[tauri::command]
pub async fn convert_pdf_to_word(app: AppHandle, path: String) -> AppResult<ConversionResult> {
    blocking(move || {
        convert(
            &path,
            "docx",
            "pdf-to-docx",
            find_pdf_engine(&app, TO_DOCX).as_deref(),
            basic_pdf_to_docx,
        )
    })
    .await
}

#[tauri::command]
pub async fn convert_word_to_pdf(app: AppHandle, path: String) -> AppResult<ConversionResult> {
    blocking(move || {
        convert(
            &path,
            "pdf",
            "docx-to-pdf",
            find_pdf_engine(&app, TO_PDF).as_deref(),
            basic_docx_to_pdf,
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_engine_report_from_its_last_line() {
        assert!(parse_pdf_engine_report("[INFO] page 1\n{\"ok\": true}\n\n").is_ok());

        let error = parse_pdf_engine_report(
            r#"{"ok": false, "code": "invalid_pdf", "message": "password-protected"}"#,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidPdf);
        assert_eq!(error.message, "password-protected");

        let error =
            parse_pdf_engine_report(r#"{"ok": false, "code": "conversion_failed"}"#).unwrap_err();
        assert_eq!(error.code, ErrorCode::ConversionFailed);

        for broken in [
            "",
            "Traceback (most recent call last):",
            "{\"ok\": true}\ncrash",
        ] {
            let error = parse_pdf_engine_report(broken).unwrap_err();
            assert_eq!(error.code, ErrorCode::ConversionFailed, "{broken:?}");
        }
    }

    #[test]
    fn looks_for_the_packaged_engine_first() {
        let resources = PathBuf::from("resources");
        let candidates = pdf_engine_candidates(Some(resources.clone()), TO_PDF);
        assert!(candidates[0].starts_with(&resources));
        let executable = if cfg!(windows) {
            "minos-to-pdf.exe"
        } else {
            "minos-to-pdf"
        };
        assert!(candidates[0].ends_with(Path::new(TO_PDF).join(executable)));
    }

    #[test]
    fn falls_back_when_the_engine_cannot_run() {
        let dir = tempfile::tempdir().unwrap();
        let docx = dir.path().join("in.docx");
        let pdf = dir.path().join("in.pdf");
        write_docx(&[vec!["Hola".into()]], &docx).unwrap();
        basic_docx_to_pdf(&docx, &pdf).unwrap();

        let missing = dir.path().join("no-engine.exe");
        let result = convert(
            pdf.to_str().unwrap(),
            "docx",
            "pdf-to-docx",
            Some(&missing),
            basic_pdf_to_docx,
        )
        .unwrap();
        assert_eq!(result.engine, "basic");
    }

    #[test]
    #[ignore = "needs the engine built by pdf-engine/build.py"]
    fn converts_both_ways_with_the_real_engine_and_an_accented_path() {
        let find = |name| {
            pdf_engine_candidates(None, name)
                .into_iter()
                .find(|path| path.is_file())
                .expect("build the engine first")
        };
        let (to_docx, to_pdf) = (find(TO_DOCX), find(TO_PDF));
        let dir = tempfile::tempdir().unwrap();
        let docx = dir.path().join("in.docx");
        let pdf = dir.path().join("Documentación señor ñ.pdf");
        write_docx(&[vec!["Hola señor".into()]], &docx).unwrap();
        basic_docx_to_pdf(&docx, &pdf).unwrap();

        let result = convert(
            pdf.to_str().unwrap(),
            "docx",
            "pdf-to-docx",
            Some(&to_docx),
            basic_pdf_to_docx,
        )
        .unwrap();
        assert_eq!(result.engine, "minos");

        let accented_docx = dir.path().join("Informe señor ñ.docx");
        fs::copy(&docx, &accented_docx).unwrap();
        let result = convert(
            accented_docx.to_str().unwrap(),
            "pdf",
            "docx-to-pdf",
            Some(&to_pdf),
            basic_docx_to_pdf,
        )
        .unwrap();
        assert_eq!(result.engine, "minos");

        let corrupt = dir.path().join("corrupto.pdf");
        fs::write(&corrupt, b"not a pdf").unwrap();
        let error = run_pdf_engine(
            &to_docx,
            "pdf-to-docx",
            &corrupt,
            &dir.path().join("x.docx"),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidPdf);

        let legacy = dir.path().join("antiguo.doc");
        fs::write(&legacy, b"\xd0\xcf\x11\xe0 legacy").unwrap();
        let error = convert(
            legacy.to_str().unwrap(),
            "pdf",
            "docx-to-pdf",
            Some(&to_pdf),
            basic_docx_to_pdf,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedFormat);
    }

    #[test]
    fn rejects_formats_other_than_docx_without_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["antiguo.doc", "texto.odt", "texto.rtf"] {
            let path = dir.path().join(name);
            fs::write(&path, b"x").unwrap();
            let error = convert(
                path.to_str().unwrap(),
                "pdf",
                "docx-to-pdf",
                None,
                basic_docx_to_pdf,
            )
            .unwrap_err();
            assert_eq!(error.code, ErrorCode::UnsupportedFormat, "{name}");
        }
    }

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
}
