"""Generated fixtures, so the repository holds no binary documents."""

import io
import os
import struct
import sys
import zlib
from pathlib import Path

import pytest
from docx import Document
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.oxml import OxmlElement, parse_xml
from docx.oxml.ns import nsdecls, qn
from docx.shared import Pt, RGBColor

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


def png_bytes(color=(200, 30, 30), size=8) -> bytes:
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    row = b"\x00" + bytes(color) * size
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(row * size))
        + chunk(b"IEND", b"")
    )


def add_page_field(paragraph):
    run = paragraph.add_run()
    for kind, text in (("begin", None), (None, " PAGE "), ("separate", None), ("cached", "1"), ("end", None)):
        if kind in ("begin", "separate", "end"):
            element = OxmlElement("w:fldChar")
            element.set(qn("w:fldCharType"), kind)
        elif kind == "cached":
            element = OxmlElement("w:t")
            element.text = text
        else:
            element = OxmlElement("w:instrText")
            element.text = text
        run._r.append(element)


@pytest.fixture
def rich_docx(tmp_path) -> Path:
    document = Document()
    section = document.sections[0]
    section.header.paragraphs[0].text = "Encabezado del informe"
    add_page_field(section.footer.paragraphs[0])

    document.add_heading("Título principal", level=1)

    paragraph = document.add_paragraph()
    paragraph.alignment = WD_ALIGN_PARAGRAPH.CENTER
    run = paragraph.add_run("Texto rojo en negrita ")
    run.bold = True
    run.font.color.rgb = RGBColor(0xC0, 0x10, 0x20)
    run.font.size = Pt(18)
    run.font.name = "Georgia"
    paragraph.add_run("cursiva").italic = True
    paragraph.add_run(" y subrayado").underline = True

    document.add_paragraph("Señor, ¿cómo está? Ñandú: áéíóú — € 漢字 かな 한글 Привет")
    document.add_paragraph("Primero", style="List Bullet")
    document.add_paragraph("Uno", style="List Number")
    document.add_paragraph("Dos", style="List Number")
    document.add_picture(io.BytesIO(png_bytes()), width=Pt(72))

    table = document.add_table(rows=3, cols=3, style="Table Grid")
    for row, values in enumerate((("A", "B", "C"), ("1", "2", "3"), ("x", "y", "z"))):
        for column, value in enumerate(values):
            table.cell(row, column).text = value
    table.cell(0, 0).merge(table.cell(0, 1))
    table.cell(1, 2).merge(table.cell(2, 2))
    shading = parse_xml(f'<w:shd {nsdecls("w")} w:val="clear" w:color="auto" w:fill="FFCC00"/>')
    table.cell(1, 0)._tc.get_or_add_tcPr().append(shading)

    document.add_page_break()
    document.add_paragraph("Segunda página")
    path = tmp_path / "rich.docx"
    document.save(path)
    return path


@pytest.fixture
def malicious_docx(tmp_path) -> Path:
    """Tries to reach the outside and to inject markup."""
    document = Document()
    document.add_paragraph('<img src="file:///C:/secret.png"><script>alert(1)</script>')
    paragraph = document.add_paragraph()
    run = paragraph.add_run("caption")
    # Attribute values that would break out of a style="" if they weren't validated.
    color = OxmlElement("w:color")
    color.set(qn("w:val"), 'red;background:url("http://evil.test/x")')
    fonts = OxmlElement("w:rFonts")
    fonts.set(qn("w:ascii"), "x'}</style><img src=http://evil.test/y>")
    run._r.get_or_add_rPr().append(color)
    run._r.get_or_add_rPr().append(fonts)

    # An image linked (not embedded) to a remote file.
    part = document.part
    rid_remote = part.relate_to("http://evil.test/z.png", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image", is_external=True)
    rid_local = part.relate_to("file:///C:/secret.png", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image", is_external=True)
    for rid in (rid_remote, rid_local):
        drawing = parse_xml(
            f'<w:drawing {nsdecls("w", "wp", "a", "pic", "r")}><wp:inline><wp:extent cx="100" cy="100"/>'
            '<a:graphic><a:graphicData><pic:pic><pic:blipFill>'
            f'<a:blip r:embed="{rid}" r:link="{rid}"/>'
            "</pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"
        )
        document.add_paragraph().add_run()._r.append(drawing)

    # A javascript: hyperlink.
    rid_link = part.relate_to("javascript:alert(1)", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink", is_external=True)
    link = parse_xml(f'<w:hyperlink {nsdecls("w", "r")} r:id="{rid_link}"><w:r><w:t>click</w:t></w:r></w:hyperlink>')
    document.add_paragraph()._p.append(link)
    path = tmp_path / "malicious.docx"
    document.save(path)
    return path


@pytest.fixture
def rich_pdf(tmp_path) -> Path:
    import pymupdf

    document = pymupdf.open()
    page = document.new_page(width=595, height=842)
    page.insert_text((72, 90), "Informe trimestral", fontname="hebo", fontsize=22, color=(0.75, 0.06, 0.12))
    page.insert_text((72, 125), "Señor, ¿cómo está? áéíóú ñ", fontname="helv", fontsize=12)
    page.insert_text((72, 145), "Texto en cursiva", fontname="heit", fontsize=12)

    # A 3x3 table drawn with real lines.
    left, top, cell_w, cell_h = 72, 180, 120, 28
    for row in range(4):
        page.draw_line((left, top + row * cell_h), (left + 3 * cell_w, top + row * cell_h), width=1)
    for column in range(4):
        page.draw_line((left + column * cell_w, top), (left + column * cell_w, top + 3 * cell_h), width=1)
    for row, values in enumerate((("Producto", "Unidades", "Total"), ("Lápiz", "10", "25,00"), ("Goma", "4", "6,50"))):
        for column, value in enumerate(values):
            page.insert_text((left + column * cell_w + 6, top + row * cell_h + 18), value, fontname="helv", fontsize=11)

    page.insert_image(pymupdf.Rect(72, 300, 172, 400), stream=png_bytes((30, 90, 200), 32))
    document.new_page(width=595, height=842).insert_text((72, 90), "Segunda página", fontname="helv", fontsize=12)
    path = tmp_path / "rich.pdf"
    document.save(path)
    document.close()
    return path


@pytest.fixture
def encrypted_pdf(tmp_path) -> Path:
    import pymupdf

    document = pymupdf.open()
    document.new_page().insert_text((72, 90), "secreto")
    path = tmp_path / "encrypted.pdf"
    document.save(path, encryption=pymupdf.PDF_ENCRYPT_AES_256, user_pw="clave", owner_pw="dueño")
    document.close()
    return path


def pango_available() -> bool:
    try:
        import weasyprint  # noqa: F401
    except OSError:
        return False
    return True


@pytest.fixture
def needs_pango():
    """Rendering needs native Pango. CI has it, so a skip there would hide a real failure."""
    if not pango_available():
        if os.environ.get("CI"):
            pytest.fail("Pango is required in CI")
        pytest.skip("Pango isn't installed")
