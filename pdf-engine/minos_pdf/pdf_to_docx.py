"""PDF -> DOCX with pdf2docx."""

import pymupdf
from docx import Document
from docx.oxml.ns import qn
from pdf2docx import Converter

from minos_pdf import EngineError

PRIVATE_USE = range(0xE000, 0xF900)


def check_readable(source: str) -> None:
    """Rejects files pdf2docx would fail on with an unhelpful error."""
    try:
        document = pymupdf.open(source, filetype="pdf")
    except (pymupdf.FileDataError, pymupdf.EmptyFileError, ValueError) as error:
        raise EngineError("invalid_pdf", str(error)) from error
    try:
        if document.needs_pass:
            raise EngineError("invalid_pdf", "the PDF is password-protected")
        if document.page_count == 0:
            raise EngineError("invalid_pdf", "the PDF has no pages")
    finally:
        document.close()


def fix_symbol_fonts(path: str) -> None:
    """Keeps bullets from turning into empty boxes.

    pdf2docx writes the real Unicode character (•) with the PDF's Symbol font, but Word
    only reads that font through private-use codes, so it draws a box. The font is
    dropped from those runs, which then use the document's default font.
    """
    document = Document(path)
    changed = False
    for run in document.element.body.iter(qn("w:r")):
        properties = run.find(qn("w:rPr"))
        fonts = properties.find(qn("w:rFonts")) if properties is not None else None
        if fonts is None or "symbol" not in {value.lower() for value in fonts.attrib.values()}:
            continue
        text = "".join(node.text or "" for node in run.findall(qn("w:t")))
        if text and not any(ord(char) in PRIVATE_USE for char in text):
            properties.remove(fonts)
            changed = True
    if changed:
        document.save(path)


def convert(source: str, output: str) -> None:
    check_readable(source)
    converter = Converter(source)
    try:
        converter.convert(output)
    finally:
        converter.close()
    fix_symbol_fonts(output)
