import re
from urllib.request import OpenerDirector

import pymupdf
import pytest
from pypdf import PdfReader

from minos_pdf import EngineError
from docx import Document
from docx.oxml import parse_xml
from docx.oxml.ns import nsdecls

from minos_pdf.docx_to_pdf import build_html, convert, format_number, write_pdf


def markup_of(document, tmp_path):
    path = tmp_path / "doc.docx"
    document.save(path)
    return build_html(str(path))


def spacing(**attributes):
    values = " ".join(f'w:{name}="{value}"' for name, value in attributes.items())
    return parse_xml(f'<w:spacing {nsdecls("w")} {values}/>')


def test_keeps_run_and_paragraph_formatting(rich_docx):
    markup = build_html(str(rich_docx))
    assert "font-size:18pt" in markup
    assert "color:#c01020" in markup
    assert "font-weight:bold" in markup
    assert "font-style:italic" in markup
    assert "text-decoration:underline" in markup
    assert "'Georgia'" in markup
    assert "text-align:center" in markup


def test_keeps_unicode_text(rich_docx):
    markup = build_html(str(rich_docx))
    assert "Señor, ¿cómo está? Ñandú: áéíóú — € 漢字 かな 한글 Привет" in markup


def test_generates_list_markers(rich_docx):
    markup = build_html(str(rich_docx))
    assert ">•</span>" in markup
    assert re.search(r">1\.</span>.*Uno", markup)
    assert re.search(r">2\.</span>.*Dos", markup)


def test_embeds_images_as_data_urls(rich_docx):
    assert 'src="data:image/png;base64,' in build_html(str(rich_docx))


def test_table_borders_merges_and_shading(rich_docx):
    markup = build_html(str(rich_docx))
    assert "border-collapse:collapse" in markup
    assert re.search(r"border-top:[\d.]+pt solid #000000", markup)
    assert 'colspan="2"' in markup
    assert 'rowspan="2"' in markup
    assert "background-color:#ffcc00" in markup


def test_page_setup_and_margin_boxes(rich_docx):
    markup = build_html(str(rich_docx))
    assert re.search(r"@page s0\{size:[\d.]+pt [\d.]+pt", markup)
    assert "Encabezado del informe" in markup
    assert 'class="page-number"' in markup
    assert "break-after:page" in markup
    # The cached value of the PAGE field must not leak as literal text next to the counter.
    assert not re.search(r'class="page-number"></span>1', markup)


def test_untrusted_values_cannot_inject_markup_or_urls(malicious_docx):
    markup = build_html(str(malicious_docx))
    assert "http://evil.test" not in markup
    assert "url(" not in markup
    assert "<img src=" not in markup
    assert 'src="file:' not in markup
    assert "<script" not in markup
    assert "javascript:" not in markup
    assert "&lt;script&gt;" in markup
    assert "click" in markup


def test_paragraph_mark_formatting_does_not_reach_the_text(tmp_path):
    document = Document()
    paragraph = document.add_paragraph("Texto normal")
    mark = parse_xml(f'<w:rPr {nsdecls("w")}><w:b/><w:sz w:val="48"/></w:rPr>')
    paragraph._p.get_or_add_pPr().append(mark)
    markup = markup_of(document, tmp_path)
    tag = re.search(r"<p [^>]*>(?:(?!</p>).)*Texto normal", markup).group()
    assert "font-weight:bold" not in tag
    assert "font-size:24pt" not in tag


def test_postscript_font_names_resolve_to_the_real_font(tmp_path):
    document = Document()
    document.add_paragraph().add_run("x").font.name = "Arial MT"
    markup = markup_of(document, tmp_path)
    assert re.search(r"font-family:'Arial MT','Arial'", markup)


def test_complex_script_font_does_not_replace_a_missing_latin_font(tmp_path):
    document = Document()
    run = document.add_paragraph().add_run("x")
    run.font.name = "Aptos"
    fonts = run._r.rPr.rFonts
    fonts.set(f"{{{fonts.nsmap['w']}}}cs", "Times New Roman")
    stack = re.search(r"font-family:('Aptos'[^;]*)", markup_of(document, tmp_path)).group(1)
    assert stack.index("'Arial'") < stack.index("'Times New Roman'")


def test_line_spacing_multiplies_the_natural_line_height_of_each_font(tmp_path):
    document = Document()
    for text, font in (("a", "Arial"), ("b", "Calibri")):
        paragraph = document.add_paragraph()
        paragraph.add_run(text).font.name = font
        paragraph._p.get_or_add_pPr().append(spacing(line="480", lineRule="auto"))
    markup = markup_of(document, tmp_path)
    assert re.search(r'line-height:2\.300">a<', markup)
    assert re.search(r'line-height:2\.440">b<', markup)


def test_fixed_line_height_keeps_the_strut_on_the_text_baseline(tmp_path):
    document = Document()
    paragraph = document.add_paragraph()
    paragraph.add_run("x").font.name = "Arial"
    paragraph._p.get_or_add_pPr().append(spacing(line="346", lineRule="exact"))
    tag = re.search(r'<p style="[^"]*line-height:17.3pt[^"]*"', markup_of(document, tmp_path)).group()
    assert "font-family:'Arial'" in tag and "font-size:" in tag


def test_automatic_spacing_is_kept_around_a_list_but_not_inside(tmp_path):
    document = Document()
    document.add_paragraph("antes")._p.get_or_add_pPr().append(spacing(afterAutospacing="1"))
    for text in ("uno", "dos"):
        item = document.add_paragraph(text, style="List Bullet")
        item._p.get_or_add_pPr().append(spacing(beforeAutospacing="1", afterAutospacing="1"))
    markup = markup_of(document, tmp_path)
    first_item = re.search(r'<p style="([^"]*)">(?:(?!</p>).)*uno', markup).group(1)
    second_item = re.search(r'<p style="([^"]*)">(?:(?!</p>).)*dos', markup).group(1)
    assert "margin-top:14pt" not in first_item or "margin-bottom:14pt" not in first_item
    assert "margin-top:14pt" not in second_item
    assert "margin-bottom:14pt" in re.search(r'<p style="([^"]*)">(?:(?!</p>).)*antes', markup).group(1)


def spacing_style(tmp_path, compat):
    document = Document()
    document.add_paragraph("a")._p.get_or_add_pPr().append(spacing(before="240", after="160"))
    settings = document.settings.element
    options = parse_xml(f'<w:compat {nsdecls("w")}>{compat}</w:compat>')
    for existing in settings.findall(options.tag):
        settings.remove(existing)
    settings.append(options)
    return re.search(r'<p style="([^"]*)">(?:(?!</p>).)*>a<', markup_of(document, tmp_path)).group(1)


def compatibility_mode(mode):
    return ('<w:compatSetting w:name="compatibilityMode" '
            f'w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/>')


def test_word_2013_mode_uses_the_larger_spacing(tmp_path):
    style = spacing_style(tmp_path, compatibility_mode(15))
    assert "margin-top:12pt" in style
    assert "margin-bottom:8pt" in style


@pytest.mark.parametrize(
    "compat",
    [compatibility_mode(11), compatibility_mode(14), "", "<w:doNotUseHTMLParagraphAutoSpacing/>" + compatibility_mode(15)],
)
def test_older_documents_add_the_spacing(compat, tmp_path):
    style = spacing_style(tmp_path, compat)
    assert "margin-top:12pt" in style
    assert "padding-bottom:8pt" in style


def test_paragraph_after_a_page_break_drops_its_space_above(tmp_path):
    from docx.enum.text import WD_BREAK

    document = Document()
    document.add_paragraph("fin").add_run().add_break(WD_BREAK.PAGE)
    document.add_paragraph("siguiente")._p.get_or_add_pPr().append(spacing(before="240"))
    style = re.search(r'<p style="([^"]*)">(?:(?!</p>).)*siguiente', markup_of(document, tmp_path)).group(1)
    assert "margin-top" not in style


def test_page_break_opening_a_paragraph_is_a_break_before_it(tmp_path):
    from docx.enum.text import WD_BREAK

    document = Document()
    paragraph = document.add_paragraph()
    paragraph.add_run().add_break(WD_BREAK.PAGE)
    paragraph.add_run("Título")
    markup = markup_of(document, tmp_path)
    assert re.search(r'<p style="[^"]*break-before:page[^"]*">(?:(?!</p>).)*Título', markup)
    assert "display:block;break-after:page" not in markup


def line_shape(document):
    return parse_xml(
        f'<w:drawing {nsdecls("w", "wp", "a")} '
        'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
        '<wp:anchor behindDoc="0" distT="0" distB="0" distL="0" distR="0">'
        '<wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH>'
        '<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>'
        '<wp:extent cx="1270000" cy="0"/><wp:wrapNone/><a:graphic><a:graphicData>'
        '<wps:wsp><wps:spPr><a:prstGeom prst="line"/><a:ln w="19050"><a:solidFill><a:srgbClr val="1F3864"/>'
        "</a:solidFill></a:ln></wps:spPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing>"
    )


def test_drawn_lines_are_kept(tmp_path):
    document = Document()
    document.add_paragraph().add_run()._r.append(line_shape(document))
    assert "border-top:1.5pt solid #1f3864" in markup_of(document, tmp_path)


def test_spaces_html_would_collapse_are_kept(tmp_path):
    document = Document()
    document.add_paragraph("  dos  espacios")
    markup = markup_of(document, tmp_path)
    assert "\u00a0\u00a0dos \u00a0espacios" in markup
    assert "pre-wrap" not in markup


def group_frame(document):
    """Two pictures in a group, each with its own size and position, anchored behind the text."""
    from io import BytesIO

    from conftest import png_bytes

    rid_a, _ = document.part.get_or_add_image(BytesIO(png_bytes((200, 0, 0))))
    rid_b, _ = document.part.get_or_add_image(BytesIO(png_bytes((0, 0, 200))))

    def picture(rid, x, width):
        return (f'<pic:pic><pic:blipFill><a:blip r:embed="{rid}"/></pic:blipFill><pic:spPr><a:xfrm>'
                f'<a:off x="{x}" y="0"/><a:ext cx="{width}" cy="127000"/></a:xfrm></pic:spPr></pic:pic>')

    return parse_xml(
        f'<w:drawing {nsdecls("w", "wp", "a", "pic", "r")} '
        'xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup">'
        '<wp:anchor behindDoc="1" distT="0" distB="0" distL="0" distR="0">'
        '<wp:positionH relativeFrom="column"><wp:posOffset>-127000</wp:posOffset></wp:positionH>'
        '<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>'
        '<wp:extent cx="1270000" cy="127000"/><wp:wrapNone/><a:graphic><a:graphicData>'
        '<wpg:wgp><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="127000"/>'
        '<a:chOff x="0" y="0"/><a:chExt cx="1270000" cy="127000"/></a:xfrm></wpg:grpSpPr>'
        f'{picture(rid_a, 0, 254000)}{picture(rid_b, 1016000, 254000)}'
        "</wpg:wgp></a:graphicData></a:graphic></wp:anchor></w:drawing>"
    )


def test_grouped_pictures_keep_their_own_size_and_position(tmp_path):
    document = Document()
    document.add_paragraph().add_run()._r.append(group_frame(document))
    markup = markup_of(document, tmp_path)
    images = re.findall(r'<img style="([^"]*)"', markup)
    assert len(images) == 2
    assert all("width:20pt" in image and "height:10pt" in image for image in images)
    assert "left:0pt" in images[0]
    assert "left:80pt" in images[1]
    assert "position:absolute;left:-10pt;top:0pt;z-index:-1" in markup
    assert "position:relative" in markup


def tabbed(document, text_parts, stops, style=None):
    paragraph = document.add_paragraph(style=style)
    tabs = "".join(f'<w:tab w:val="{kind}" w:pos="{position}" w:leader="{leader}"/>' for kind, position, leader in stops)
    paragraph._p.get_or_add_pPr().append(parse_xml(f'<w:tabs {nsdecls("w")}>{tabs}</w:tabs>'))
    for index, text in enumerate(text_parts):
        run = paragraph.add_run(text)
        if index < len(text_parts) - 1:
            run.add_tab()
    return paragraph


def test_right_tab_stop_draws_its_leader_and_right_text(tmp_path):
    document = Document()
    tabbed(document, ["Introducción", "4"], [("right", 9360, "dot")])
    markup = markup_of(document, tmp_path)
    assert re.search(r'class="tab-dot"[^>]*data-right="4"', markup)
    assert "\ufdd0" not in markup


def test_left_tab_stop_reserves_the_width_up_to_it(tmp_path):
    document = Document()
    tabbed(document, ["1.1", "Título", "5"], [("left", 720, "none"), ("right", 9360, "dot")])
    markup = markup_of(document, tmp_path)
    assert re.search(r"min-width:36pt[^>]*>(?:(?!</span></span>).)*1\.1", markup)
    assert 'data-right="5"' in markup


def test_long_text_before_a_tab_can_still_wrap(tmp_path):
    document = Document()
    tabbed(document, ["texto largo " * 8, "fin"], [("left", 720, "none")])
    assert "white-space:nowrap" not in markup_of(document, tmp_path)


def test_tab_inside_a_link_does_not_split_its_tags(tmp_path):
    document = Document()
    paragraph = tabbed(document, ["Entrada", "7"], [("right", 9360, "dot")])
    part = document.part
    rid = part.relate_to("https://example.com/", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink", is_external=True)
    link = parse_xml(f'<w:hyperlink {nsdecls("w", "r")} r:id="{rid}"/>')
    for run in list(paragraph.runs):
        link.append(run._r)
    paragraph._p.append(link)
    markup = markup_of(document, tmp_path)
    assert markup.count("<a ") == markup.count("</a>")
    assert re.search(r'class="tab-dot"[^>]*data-right="7"', markup)


def test_contents_entries_do_not_show_the_hyperlink_style(tmp_path):
    from docx.enum.style import WD_STYLE_TYPE

    document = Document()
    document.styles.add_style("toc 1", WD_STYLE_TYPE.PARAGRAPH)
    hyperlink = document.styles.add_style("Hyperlink", WD_STYLE_TYPE.CHARACTER)
    hyperlink.font.underline = True
    paragraph = document.add_paragraph(style="toc 1")
    paragraph.add_run("Capítulo").style = hyperlink
    assert "underline" not in re.search(r"<p[^>]*>(?:(?!</p>).)*Capítulo", markup_of(document, tmp_path)).group()


def test_numbers_take_the_format_of_the_paragraph_mark(tmp_path):
    document = Document()
    paragraph = document.add_paragraph("Primero", style="List Number")
    mark = parse_xml(f'<w:rPr {nsdecls("w")}><w:b/><w:sz w:val="28"/></w:rPr>')
    paragraph._p.get_or_add_pPr().append(mark)
    marker = re.search(r'<span style="([^"]*)">1\.</span>', markup_of(document, tmp_path)).group(1)
    assert "font-weight:bold" in marker
    assert "font-size:14pt" in marker


def add_section(document, kind):
    from docx.enum.section import WD_SECTION

    return document.add_section(WD_SECTION.CONTINUOUS if kind == "continuous" else WD_SECTION.NEW_PAGE)


def test_continuous_section_stays_on_the_page(tmp_path):
    document = Document()
    document.add_paragraph("primera")
    add_section(document, "continuous")
    columns = document.sections[-1]._sectPr.xpath("./w:cols")[0]
    columns.set(columns.nsmap and f"{{{columns.nsmap['w']}}}num", "2")
    document.add_paragraph("segunda")
    markup = markup_of(document, tmp_path)
    sections = re.findall(r'<section style="([^"]*)"', markup)
    assert len(sections) == 2
    assert "break-before" not in sections[1]
    assert re.search(r"page:(s\d+)", sections[0]).group(1) == re.search(r"page:(s\d+)", sections[1]).group(1)
    assert "column-count:2" in sections[1]


def test_new_page_section_starts_a_new_page(tmp_path):
    document = Document()
    document.add_paragraph("primera")
    add_section(document, "new")
    document.add_paragraph("segunda")
    sections = re.findall(r'<section style="([^"]*)"', markup_of(document, tmp_path))
    assert "break-before:page" in sections[1]
    assert "page:s1" in sections[1]


def test_equations_keep_their_content(tmp_path):
    document = Document()
    equation = parse_xml(
        '<m:oMathPara xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:oMath>'
        "<m:r><m:t>F1=</m:t></m:r><m:f><m:num><m:r><m:t>2</m:t></m:r></m:num><m:den><m:r><m:t>N</m:t></m:r></m:den></m:f>"
        "<m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>3</m:t></m:r></m:sup></m:sSup>"
        "</m:oMath></m:oMathPara>"
    )
    document.add_paragraph()._p.append(equation)
    plain = re.sub(r"<[^>]+>", "", markup_of(document, tmp_path).split("</style>")[1])
    assert "F1=2Nx3" in plain.replace("\u00a0", "").strip()


def test_page_break_ending_a_paragraph_leaves_no_empty_line_behind(tmp_path):
    document = Document()
    document.add_paragraph("fin").add_run().add_break(__import__("docx.enum.text").enum.text.WD_BREAK.PAGE)
    document.add_paragraph("siguiente")
    markup = markup_of(document, tmp_path)
    assert re.search(r'<p style="[^"]*break-after:page[^"]*">(?:(?!</p>).)*fin', markup)
    assert "display:block;break-after:page" not in markup


def test_rejects_files_that_are_not_docx(tmp_path):
    path = tmp_path / "old.doc"
    path.write_bytes(b"\xd0\xcf\x11\xe0 not a zip")
    with pytest.raises(EngineError) as error:
        build_html(str(path))
    assert error.value.code == "unsupported_format"


@pytest.mark.parametrize(
    ("value", "fmt", "expected"),
    [(3, "decimal", "3"), (4, "lowerRoman", "iv"), (9, "upperRoman", "IX"), (2, "lowerLetter", "b"),
     (27, "upperLetter", "AA")],
)
def test_number_formats(value, fmt, expected):
    assert format_number(value, fmt) == expected


def test_renders_a_pdf(rich_docx, tmp_path, needs_pango):
    output = tmp_path / "out.pdf"
    convert(str(rich_docx), str(output))
    # Not pypdf: where no font has the CJK text of this document, which is usual on Linux,
    # it garbles the whole page. PyMuPDF and PDFium (Chrome, Edge) read that PDF fine.
    with pymupdf.open(output) as pdf:
        assert pdf.page_count == 2
        first = pdf[0].get_text()
        assert "Título principal" in first
        assert "Señor" in first
        assert "Encabezado del informe" in first
        assert pdf[0].get_images()
        assert "2" in pdf[1].get_text()


@pytest.mark.parametrize("font", ["Cambria", "Arial", "Calibri", "Times New Roman"])
def test_accents_survive_in_the_text_layer(font, tmp_path, needs_pango):
    document = Document()
    text = "Señor, ¿cómo está? áéíóú Ñandú"
    document.add_paragraph().add_run(text).font.name = font
    source, output = tmp_path / "doc.docx", tmp_path / "out.pdf"
    document.save(source)
    convert(str(source), str(output))
    assert text in PdfReader(output).pages[0].extract_text()


def test_justified_text_is_really_justified(tmp_path, needs_pango):
    document = Document()
    paragraph = document.add_paragraph("palabra " * 60)
    paragraph._p.get_or_add_pPr().append(
        parse_xml(f'<w:jc {nsdecls("w")} w:val="both"/>')
    )
    source, output = tmp_path / "doc.docx", tmp_path / "out.pdf"
    document.save(source)
    convert(str(source), str(output))
    with pymupdf.open(output) as pdf:
        lines = [line for block in pdf[0].get_text("dict")["blocks"] for line in block.get("lines", [])]
    edges = {round(line["bbox"][2]) for line in lines[:-1]}
    assert len(edges) == 1, edges


def test_renderer_only_loads_data_urls(tmp_path, needs_pango):
    secret = tmp_path / "secret.png"
    secret.write_bytes(b"not read")
    opened = []

    # Every protocol goes through this method, so it sees anything actually loaded.
    original = OpenerDirector.open

    def spy(self, request, *args, **kwargs):
        opened.append(getattr(request, "full_url", request))
        return original(self, request, *args, **kwargs)

    OpenerDirector.open = spy
    try:
        output = tmp_path / "out.pdf"
        write_pdf(f'<img src="{secret.as_uri()}"><img src="http://evil.test/x.png"><p>ok</p>', str(output))
    finally:
        OpenerDirector.open = original
    assert not [url for url in opened if not str(url).startswith("data:")]
    assert "ok" in PdfReader(output).pages[0].extract_text()
