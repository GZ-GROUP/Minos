import pytest
from docx import Document

from minos_pdf import EngineError
from minos_pdf.pdf_to_docx import convert, fix_symbol_fonts


def test_keeps_text_formatting_tables_and_images(rich_pdf, tmp_path):
    output = tmp_path / "out.docx"
    convert(str(rich_pdf), str(output))
    document = Document(output)

    text = "\n".join(paragraph.text for paragraph in document.paragraphs)
    assert "Informe trimestral" in text
    assert "Señor, ¿cómo está? áéíóú ñ" in text
    assert "Segunda página" in text

    title = next(run for p in document.paragraphs for run in p.runs if "Informe" in run.text)
    assert title.bold
    assert title.font.size.pt == pytest.approx(22, abs=1)
    assert title.font.color.rgb is not None

    assert len(document.tables) == 1
    cells = [[cell.text.strip() for cell in row.cells] for row in document.tables[0].rows]
    assert cells[0] == ["Producto", "Unidades", "Total"]
    assert cells[1][0] == "Lápiz"

    assert document.inline_shapes or document.part.package.image_parts


def test_symbol_font_is_dropped_only_where_word_would_draw_a_box(tmp_path):
    document = Document()
    paragraph = document.add_paragraph()
    for text, font in (("•", "Symbol"), ("α", "SYMBOL"), ("", "Symbol"), ("texto", "Arial")):
        paragraph.add_run(text).font.name = font
    path = tmp_path / "bullets.docx"
    document.save(path)

    fix_symbol_fonts(str(path))

    runs = Document(path).paragraphs[0].runs
    assert [run.font.name for run in runs] == [None, None, "Symbol", "Arial"]
    assert [run.text for run in runs] == ["•", "α", "", "texto"]


def test_rejects_encrypted_pdf(encrypted_pdf, tmp_path):
    with pytest.raises(EngineError) as error:
        convert(str(encrypted_pdf), str(tmp_path / "out.docx"))
    assert error.value.code == "invalid_pdf"


def test_rejects_corrupt_pdf(tmp_path):
    path = tmp_path / "bad.pdf"
    path.write_bytes(b"this is not a pdf")
    with pytest.raises(EngineError) as error:
        convert(str(path), str(tmp_path / "out.docx"))
    assert error.value.code == "invalid_pdf"
