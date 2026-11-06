"""DOCX -> PDF: walks the document with python-docx, builds HTML/CSS, renders with WeasyPrint.

The document is untrusted input. Text goes through `html.escape`, every value that
reaches CSS goes through a validator below, and the renderer may only load `data:` URLs.
"""

import base64
import html
import re
import zipfile
from dataclasses import dataclass, field

from docx import Document
from docx.enum.style import WD_STYLE_TYPE
from docx.opc.constants import RELATIONSHIP_TYPE as RT
from docx.opc.exceptions import PackageNotFoundError
from lxml import etree

from minos_pdf import EngineError
from minos_pdf.bundle import use_bundled_libraries

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {
    "w": W,
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "wpg": "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
    "pic": "http://schemas.openxmlformats.org/drawingml/2006/picture",
    "v": "urn:schemas-microsoft-com:vml",
    "mc": "http://schemas.openxmlformats.org/markup-compatibility/2006",
    "m": "http://schemas.openxmlformats.org/officeDocument/2006/math",
    "wps": "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
}
R_ID = f"{{{NS['r']}}}id"
R_EMBED = f"{{{NS['r']}}}embed"

BORDER_STYLES = {
    "single": "solid", "thick": "solid", "double": "double", "dotted": "dotted",
    "dashed": "dashed", "dashSmallGap": "dashed", "dotDash": "dashed", "dotDotDash": "dashed",
}
HIGHLIGHTS = {
    "yellow": "#ffff00", "green": "#00ff00", "cyan": "#00ffff", "magenta": "#ff00ff",
    "blue": "#0000ff", "red": "#ff0000", "darkBlue": "#000080", "darkCyan": "#008080",
    "darkGreen": "#008000", "darkMagenta": "#800080", "darkRed": "#800000",
    "darkYellow": "#808000", "darkGray": "#808080", "lightGray": "#c0c0c0", "black": "#000000",
    "white": "#ffffff",
}
ALIGNMENTS = {"left": "left", "start": "left", "center": "center", "right": "right", "end": "right",
              "both": "justify", "distribute": "justify"}
IMAGE_TYPES = {"image/png", "image/jpeg", "image/gif", "image/bmp", "image/tiff", "image/webp"}
# Private-use glyphs from symbol fonts, as Word stores common bullets.
BULLETS = {"": "•", "": "▪", "": "➢", "": "✓", "": "❖", "": "◦"}
DEFAULT_TAB_PT = 36.0
DEFAULT_FONT_PT = 10.0  # Word's size when neither the document nor its styles set one.
AUTO_SPACING_PT = 14.0  # What Word uses for `beforeAutospacing` / `afterAutospacing`.
EMU_PER_PT = 12700
# Marks where a tab goes while a paragraph's HTML is built. A noncharacter, so that no
# document text can be mistaken for it.
TAB = "\ufdd0"
PAGE_BREAK = '<span style="display:block;break-after:page"></span>'
# A word processor's leader characters, for CSS `leader()`.
LEADERS = {"dot": "tab-dot", "middleDot": "tab-dot", "hyphen": "tab-hyphen", "underscore": "tab-line",
           "heavy": "tab-line"}
# Longer text before a tab is ordinary running text, and mustn't be kept on one line.
MAX_TAB_LABEL_CHARS = 40

# Fonts the document may name but the system may lack, closest first.
FONT_SUBSTITUTES = {
    # Aptos is a cloud font that's usually only on machines with recent Office. It's wider
    # than Calibri, so Arial wraps text at about the same places.
    "aptos": ["Arial", "Liberation Sans"],
    "aptos display": ["Arial", "Liberation Sans"],
    "aptos narrow": ["Calibri", "Carlito"],
    "calibri": ["Carlito"],
    "calibri light": ["Calibri", "Carlito"],
    "cambria": ["Caladea"],
    "arial": ["Liberation Sans", "Helvetica"],
    "times new roman": ["Liberation Serif", "Times"],
    "courier new": ["Liberation Mono", "Courier"],
}
# From `w:family` in the font table, for fonts the list above doesn't know.
FAMILY_SUBSTITUTES = {
    "swiss": ["Arial", "Liberation Sans"],
    "roman": ["Times New Roman", "Liberation Serif"],
    "modern": ["Courier New", "Liberation Mono"],
}
# A font's natural line height as a multiple of its size. Word's "single" spacing is
# this height, while a CSS multiple would be relative to the size alone.
NATURAL_LINE_HEIGHT = {
    "arial": 1.15, "times new roman": 1.15, "liberation sans": 1.15, "liberation serif": 1.15,
    "calibri": 1.22, "calibri light": 1.22, "carlito": 1.22, "aptos": 1.22, "aptos display": 1.22,
    "cambria": 1.17, "caladea": 1.17, "courier new": 1.13, "georgia": 1.14, "verdana": 1.22,
    "tahoma": 1.21, "segoe ui": 1.33,
}
DEFAULT_LINE_HEIGHT = 1.2

# Compiled because python-docx's own `.xpath()` doesn't know the drawing namespaces.
FIND_BLIPS = etree.XPath(".//a:blip[not(ancestor::mc:Fallback)]", namespaces=NS)
FIND_FRAMES = etree.XPath(".//*[self::wp:inline or self::wp:anchor][not(ancestor::mc:Fallback)]", namespaces=NS)
FIND_VML_IMAGES = etree.XPath(".//v:imagedata[not(ancestor::mc:Fallback)]", namespaces=NS)
FIND_TEXT_BOXES = etree.XPath(".//w:txbxContent[not(ancestor::mc:Fallback)]", namespaces=NS)


def w(name: str) -> str:
    return f"{{{W}}}{name}"


# ── Validators for values that reach CSS ─────────────────────


def number(value, default=None):
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def css_color(value):
    if value and re.fullmatch(r"[0-9a-fA-F]{6}", value):
        return f"#{value.lower()}"
    return None


def css_font(name: str) -> str:
    return re.sub(r"[^\w .\-]", "", name, flags=re.UNICODE).strip()


def plain_font_name(name: str) -> str:
    """Drops the suffix of PostScript-style names: "Arial MT" is Arial to Word."""
    return re.sub(r"[ -]?(PS)?MT$", "", name)


def is_on(element) -> bool:
    """A toggle like `<w:b/>`; `w:val` may turn it off."""
    return element is not None and element.get(w("val"), "1") not in ("0", "false", "off", "none")


# ── Formatting chains ────────────────────────────────────────
#
# Word resolves a property by looking through layers. Both helpers take the
# layers from strongest to weakest and return the first value found.


def first(chain, tag):
    for layer in chain:
        found = layer.find(w(tag))
        if found is not None:
            return found
    return None


def attr(chain, tag, name):
    for layer in chain:
        found = layer.find(w(tag))
        if found is not None and found.get(w(name)) is not None:
            return found.get(w(name))
    return None


def to_roman(value: int) -> str:
    out = ""
    for arabic, roman in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"),
                          (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")]:
        while value >= arabic:
            out += roman
            value -= arabic
    return out


def to_letters(value: int) -> str:
    # Word's a..z, aa, bb, cc scheme.
    return chr(ord("a") + (value - 1) % 26) * ((value - 1) // 26 + 1) if value > 0 else ""


def format_number(value: int, fmt: str) -> str:
    return {
        "lowerLetter": to_letters(value),
        "upperLetter": to_letters(value).upper(),
        "lowerRoman": to_roman(value),
        "upperRoman": to_roman(value).upper(),
        "none": "",
    }.get(fmt, str(value))


def plain_text(markup: str) -> str:
    return html.unescape(re.sub(r"<[^>]+>", "", markup))


@dataclass
class Level:
    fmt: str
    text: str
    start: int
    ppr: object
    rpr: object


class Numbering:
    """Tracks list counters, since markers are generated rather than left to CSS."""

    def __init__(self, document):
        self.definitions = {}
        self.counters = {}
        try:
            root = document.part.numbering_part.element
        except (KeyError, NotImplementedError):
            return
        abstract = {a.get(w("abstractNumId")): a for a in root.findall(w("abstractNum"))}
        for num in root.findall(w("num")):
            ref = num.find(w("abstractNumId"))
            base = abstract.get(ref.get(w("val"))) if ref is not None else None
            if base is None:
                continue
            levels = {}
            for lvl in base.findall(w("lvl")):
                levels[lvl.get(w("ilvl"))] = self._level(lvl)
            for override in num.findall(w("lvlOverride")):
                lvl = override.find(w("lvl"))
                if lvl is not None:
                    levels[override.get(w("ilvl"))] = self._level(lvl)
            self.definitions[num.get(w("numId"))] = (ref.get(w("val")), levels)

    @staticmethod
    def _level(lvl) -> Level:
        def value(tag):
            found = lvl.find(w(tag))
            return found.get(w("val")) if found is not None else None

        return Level(value("numFmt") or "decimal", value("lvlText") or "",
                     int(number(value("start"), 1)), lvl.find(w("pPr")), lvl.find(w("rPr")))

    def level(self, num_id, ilvl):
        definition = self.definitions.get(num_id)
        return definition[1].get(str(ilvl)) if definition else None

    def next_marker(self, num_id, ilvl: int):
        abstract_id, levels = self.definitions[num_id]
        counters = self.counters.setdefault(abstract_id, {})
        level = levels.get(str(ilvl))
        if level is None:
            return ""
        counters[ilvl] = counters[ilvl] + 1 if ilvl in counters else level.start
        for deeper in [key for key in counters if key > ilvl]:
            del counters[deeper]
        if level.fmt == "bullet":
            return "".join(BULLETS.get(char, char) for char in level.text) or "•"

        def substitute(match):
            index = int(match.group(1)) - 1
            parent = levels.get(str(index))
            value = counters.get(index, parent.start if parent else 1)
            return format_number(value, parent.fmt if parent else "decimal")

        return re.sub(r"%(\d)", substitute, level.text)


# ── Converter ────────────────────────────────────────────────


@dataclass
class Story:
    """Where content lives: its package part (relationships differ) and table styling."""

    part: object
    table_ppr: list = field(default_factory=list)
    table_rpr: list = field(default_factory=list)
    in_header: bool = False


@dataclass
class Page:
    """Measurements of the current section, in points, for positioned drawings."""

    width: float = 612.0
    left: float = 72.0
    right: float = 72.0
    top: float = 72.0
    header: float = 36.0


class HtmlBuilder:
    def __init__(self, document):
        self.document = document
        self.numbering = Numbering(document)
        self.theme = self._theme_fonts()
        self.font_table = self._font_table()
        self.additive_spacing = self._uses_additive_spacing()
        self.page = Page()
        defaults = document.styles.element.find(w("docDefaults"))
        self.default_ppr = self._defaults(defaults, "pPrDefault", "pPr")
        self.default_rpr = self._defaults(defaults, "rPrDefault", "rPr")
        self.notes = self._notes()

    @staticmethod
    def _defaults(defaults, wrapper, inner):
        if defaults is None:
            return []
        found = defaults.find(f"{w(wrapper)}/{w(inner)}")
        return [found] if found is not None else []

    def _theme_fonts(self):
        fonts = {}
        try:
            root = etree.fromstring(self.document.part.part_related_by(RT.THEME).blob)
        except (KeyError, etree.XMLSyntaxError):
            return fonts
        for kind in ("major", "minor"):
            latin = root.find(f".//a:{kind}Font/a:latin", NS)
            if latin is not None:
                fonts[kind] = latin.get("typeface", "")
        return fonts

    def _uses_additive_spacing(self) -> bool:
        """Since Word 2013 (compatibility mode 15), the spacing between two paragraphs is the
        larger of the two. Documents in older modes add them, and a missing mode is an old
        document too. Measured on real documents, against Word's own output."""
        compat = self.document.settings.element.find(w("compat"))
        if compat is None:
            return True
        if compat.find(w("doNotUseHTMLParagraphAutoSpacing")) is not None:
            return True
        mode = next((number(item.get(w("val"))) for item in compat.findall(w("compatSetting"))
                     if item.get(w("name")) == "compatibilityMode"), None)
        return mode is None or mode < 15

    def _font_table(self):
        """Alternate names and generic families the document declares, by font."""
        fonts = {}
        try:
            root = etree.fromstring(self.document.part.part_related_by(RT.FONT_TABLE).blob)
        except (KeyError, etree.XMLSyntaxError):
            return fonts
        for font in root.findall(w("font")):
            alternate, family = font.find(w("altName")), font.find(w("family"))
            fonts[font.get(w("name"), "").lower()] = (
                alternate.get(w("val"), "") if alternate is not None else "",
                family.get(w("val"), "") if family is not None else "",
            )
        return fonts

    def _notes(self):
        """Plain text of footnotes and endnotes, by (kind, id)."""
        notes = {}
        for kind, rel in (("footnote", RT.FOOTNOTES), ("endnote", RT.ENDNOTES)):
            try:
                root = etree.fromstring(self.document.part.part_related_by(rel).blob)
            except (KeyError, etree.XMLSyntaxError):
                continue
            for note in root.findall(w(kind)):
                text = "".join(node.text or "" for node in note.iter(w("t")))
                notes[(kind, note.get(w("id")))] = text.strip()
        return notes

    # ── Styles ───────────────────────────────────────────────

    def _style_chain(self, style_id, style_type, tag):
        """Formatting elements (`pPr`, `rPr`...) of a style and everything it's based on."""
        chain, seen = [], set()
        styles = self.document.styles.element
        current = style_id
        while current and current not in seen:
            seen.add(current)
            style = next((s for s in styles.findall(w("style")) if s.get(w("styleId")) == current
                          and s.get(w("type")) == style_type), None)
            if style is None:
                break
            found = style.find(w(tag))
            if found is not None:
                chain.append(found)
            based = style.find(w("basedOn"))
            current = based.get(w("val")) if based is not None else None
        return chain

    def _style_name(self, style_id) -> str:
        """A style's standard name. Its id can't be used, since Word translates those."""
        style = next((s for s in self.document.styles.element.findall(w("style"))
                      if s.get(w("styleId")) == style_id), None)
        name = style.find(w("name")) if style is not None else None
        return (name.get(w("val"), "") if name is not None else "").lower()

    def _is_contents_link(self, character_style, p) -> bool:
        """Word doesn't show the Hyperlink style in a table of contents, though it's stored."""
        return (self._style_name(character_style) == "hyperlink"
                and self._style_name(self._paragraph_style_id(p)).startswith("toc"))

    def _paragraph_style_id(self, p):
        ppr = p.find(w("pPr"))
        style = ppr.find(w("pStyle")) if ppr is not None else None
        if style is not None:
            return style.get(w("val"))
        return self.document.styles.default(WD_STYLE_TYPE.PARAGRAPH).style_id

    def _ppr_chain(self, p, story, level=None):
        direct = p.find(w("pPr"))
        style_id = self._paragraph_style_id(p)
        chain = [direct] if direct is not None else []
        if level is not None and level.ppr is not None:
            chain.append(level.ppr)
        return chain + self._style_chain(style_id, "paragraph", "pPr") + story.table_ppr + self.default_ppr

    def _rpr_chain(self, r, p, story):
        chain = []
        direct = r.find(w("rPr")) if r is not None else None
        if direct is not None:
            chain.append(direct)
            style = direct.find(w("rStyle"))
            if style is not None and not self._is_contents_link(style.get(w("val")), p):
                chain += self._style_chain(style.get(w("val")), "character", "rPr")
        style_id = self._paragraph_style_id(p)
        return chain + self._style_chain(style_id, "paragraph", "rPr") + story.table_rpr + self.default_rpr

    def _font_stack(self, chain):
        """The Latin fonts and their substitutes first: the East Asian and complex-script
        fonts are only for those scripts, so they mustn't stand in for a missing Latin font."""
        names = []
        for kind in ("ascii", "hAnsi", "eastAsia", "cs"):
            name = ""
            # The strongest layer that sets this font decides; within it, a theme
            # reference beats a literal name.
            for layer in chain:
                fonts = layer.find(w("rFonts"))
                if fonts is None:
                    continue
                theme = fonts.get(w(f"{kind}Theme"))
                literal = fonts.get(w(kind))
                if theme or literal:
                    name = literal or ""
                    if theme:
                        name = self.theme.get("major" if theme.startswith("major") else "minor", name)
                    break
            name = css_font(name)
            if not name:
                continue
            for candidate in [name, *(self._substitutes(name) if kind in ("ascii", "hAnsi") else [])]:
                if candidate.lower() not in (known.lower() for known in names):
                    names.append(candidate)
        return names

    def _substitutes(self, name: str) -> list:
        plain = plain_font_name(name)
        alternate, family = self.font_table.get(name.lower(), ("", ""))
        known = FONT_SUBSTITUTES.get(plain.lower())
        return [plain, *([css_font(alternate)] if alternate else []),
                *(known if known is not None else FAMILY_SUBSTITUTES.get(family, []))]

    def _run_css(self, chain, line=None) -> str:
        """CSS of a run. `line` is the paragraph's multiple of the natural line height;
        it's set per run because that height depends on each run's font."""
        css = []
        fonts = self._font_stack(chain)
        if fonts:
            css.append("font-family:" + ",".join(f"'{name}'" for name in fonts if name)
                       + ",'Arial','Liberation Sans',sans-serif")
            # Cambria splits accented letters into two glyphs, which garbles the text
            # layer of the PDF, and with it searching and copying.
            if any(name.lower().startswith("cambria") for name in fonts):
                css.append("font-feature-settings:'ccmp' 0")
        size = (number(attr(chain, "sz", "val")) or DEFAULT_FONT_PT * 2) / 2
        css.append(f"font-size:{size:g}pt")
        if line is not None:
            natural = next((NATURAL_LINE_HEIGHT[plain_font_name(name).lower()] for name in fonts
                            if plain_font_name(name).lower() in NATURAL_LINE_HEIGHT), DEFAULT_LINE_HEIGHT)
            css.append(f"line-height:{line * natural:.3f}")
        color = css_color(attr(chain, "color", "val"))
        if color:
            css.append(f"color:{color}")
        if is_on(first(chain, "b")):
            css.append("font-weight:bold")
        if is_on(first(chain, "i")):
            css.append("font-style:italic")
        decorations = []
        underline = attr(chain, "u", "val")
        if underline and underline != "none":
            decorations.append("underline")
        if is_on(first(chain, "strike")) or is_on(first(chain, "dstrike")):
            decorations.append("line-through")
        if decorations:
            css.append("text-decoration:" + " ".join(decorations))
        if is_on(first(chain, "caps")):
            css.append("text-transform:uppercase")
        if is_on(first(chain, "smallCaps")):
            css.append("font-variant:small-caps")
        if is_on(first(chain, "vanish")):
            css.append("display:none")
        vertical = attr(chain, "vertAlign", "val")
        if vertical in ("superscript", "subscript"):
            css.append(f"vertical-align:{'super' if vertical == 'superscript' else 'sub'};font-size:{size * 0.7:g}pt")
        highlight = HIGHLIGHTS.get(attr(chain, "highlight", "val"))
        background = highlight or css_color(attr(chain, "shd", "fill"))
        if background:
            css.append(f"background-color:{background}")
        return ";".join(css)

    @staticmethod
    def _border_css(element) -> str:
        """CSS for a border element; `none` when it's explicitly removed."""
        style = BORDER_STYLES.get(element.get(w("val")))
        if style is None:
            return "none"
        width = max(number(element.get(w("sz")), 4) / 8, 0.25)
        color = css_color(element.get(w("color"))) or "#000000"
        return f"{width:g}pt {style} {color}"

    # ── Paragraphs ───────────────────────────────────────────

    @staticmethod
    def _line_multiple(chain):
        """Multiple of the natural line height, or None when the height is a fixed value."""
        line = number(attr(chain, "spacing", "line"))
        rule = attr(chain, "spacing", "lineRule") or "auto"
        if line and rule == "auto":
            return line / 240
        return None if line and rule == "exact" else 1.0

    def _spacing_css(self, chain, suppress_before, suppress_after, boxed) -> list:
        """Margins behave like Word: they collapse into the larger of the two, and the one
        above a paragraph disappears at the top of a page. If the document asks for added
        spacing instead, the space after becomes padding, which doesn't collapse, except
        with borders or shading, since that spacing belongs outside the box."""
        css = []
        for side, tag, automatic, suppressed in (("top", "before", "beforeAutospacing", suppress_before),
                                                 ("bottom", "after", "afterAutospacing", suppress_after)):
            if suppressed:
                continue
            if attr(chain, "spacing", automatic) in ("1", "true", "on"):
                css.append(f"margin-{side}:{AUTO_SPACING_PT:g}pt")
                continue
            value = number(attr(chain, "spacing", tag), 0) / 20
            if value:
                pad = self.additive_spacing and side == "bottom" and not boxed
                css.append(f"{'padding' if pad else 'margin'}-{side}:{value:g}pt")
        return css

    def _paragraph_css(self, chain, suppress_before=False, suppress_after=False) -> list:
        css = []
        align = ALIGNMENTS.get(attr(chain, "jc", "val"))
        if align:
            css.append(f"text-align:{align}")
        boxed = first(chain, "pBdr") is not None or css_color(attr(chain, "shd", "fill")) is not None
        css += self._spacing_css(chain, suppress_before, suppress_after, boxed)
        line = number(attr(chain, "spacing", "line"))
        if line and (attr(chain, "spacing", "lineRule") or "auto") != "auto":
            # With "atLeast" the runs set their own height, so this only acts as a minimum.
            css.append(f"line-height:{line / 20:g}pt")
        left = number(attr(chain, "ind", "left"), number(attr(chain, "ind", "start"), 0))
        right = number(attr(chain, "ind", "right"), number(attr(chain, "ind", "end"), 0))
        hanging = number(attr(chain, "ind", "hanging"), 0)
        first_line = -hanging if hanging else number(attr(chain, "ind", "firstLine"), 0)
        if left:
            css.append(f"margin-left:{left / 20:g}pt")
        if right:
            css.append(f"margin-right:{right / 20:g}pt")
        if first_line:
            css.append(f"text-indent:{first_line / 20:g}pt")
        fill = css_color(attr(chain, "shd", "fill"))
        if fill:
            css.append(f"background-color:{fill}")
        borders = first(chain, "pBdr")
        if borders is not None:
            for side in ("top", "left", "bottom", "right"):
                edge = borders.find(w(side))
                if edge is not None:
                    css.append(f"border-{side}:{self._border_css(edge)}")
        if is_on(first(chain, "keepNext")):
            css.append("break-after:avoid")
        if is_on(first(chain, "keepLines")):
            css.append("break-inside:avoid")
        if is_on(first(chain, "pageBreakBefore")):
            css.append("break-before:page")
        return css

    def _list_level(self, p):
        """(numId, ilvl) of a paragraph, directly or through its style."""
        ppr = p.find(w("pPr"))
        num_pr = ppr.find(w("numPr")) if ppr is not None else None
        if num_pr is None:
            num_pr = first(self._style_chain(self._paragraph_style_id(p), "paragraph", "pPr"), "numPr")
        if num_pr is None:
            return None
        num_id = num_pr.find(w("numId"))
        if num_id is None or num_id.get(w("val")) in (None, "0"):
            return None
        ilvl = num_pr.find(w("ilvl"))
        return num_id.get(w("val")), int(number(ilvl.get(w("val")), 0)) if ilvl is not None else 0

    def _suppressed_spacing(self, p, neighbor, chain) -> bool:
        """Whether the spacing towards a neighboring paragraph doesn't apply."""
        if neighbor is None or neighbor.tag != w("p"):
            return False
        # Word drops the space above a paragraph that starts a page after a manual break.
        if neighbor is p.getprevious() and self._ends_with_page_break(neighbor):
            return True
        if (is_on(first(chain, "contextualSpacing"))
                and self._paragraph_style_id(p) == self._paragraph_style_id(neighbor)):
            return True
        # Automatic spacing is only kept around a list, not between its items.
        automatic = "beforeAutospacing", "afterAutospacing"
        return (any(attr(chain, "spacing", name) in ("1", "true", "on") for name in automatic)
                and self._list_level(p) is not None and self._list_level(neighbor) is not None)

    @staticmethod
    def _ends_with_page_break(p) -> bool:
        runs = p.findall(w("r"))
        if not runs:
            return False
        last = [child for child in runs[-1] if child.tag != w("rPr")]
        return bool(last) and last[-1].tag == w("br") and last[-1].get(w("type")) == "page"

    def paragraph(self, p, story) -> str:
        listed = self._list_level(p)
        level = self.numbering.level(*listed) if listed else None
        chain = self._ppr_chain(p, story, level)
        css = self._paragraph_css(chain, self._suppressed_spacing(p, p.getprevious(), chain),
                                  self._suppressed_spacing(p, p.getnext(), chain))
        line = self._line_multiple(chain)

        marker = ""
        if level is not None:
            text = self.numbering.next_marker(*listed)
            # A number looks like its paragraph mark, unless the list level says otherwise.
            marker_chain = ([level.rpr] if level.rpr is not None else []) + self._mark_chain(p, story)
            hanging = number(attr(chain, "ind", "hanging"), 0) / 20
            width = f"min-width:{hanging:g}pt;" if hanging else "padding-right:0.5em;"
            # Symbol fonts are dropped: bullets were mapped to Unicode already.
            marker_css = re.sub(r"font-family:[^;]*;?", "", self._run_css(marker_chain, line))
            marker = f'<span style="display:inline-block;text-indent:0;{width}{marker_css}">{html.escape(text)}</span>'

        boxes = []
        left = number(attr(chain, "ind", "left"), number(attr(chain, "ind", "start"), 0)) / 20
        state = {"in_instruction": False, "field": "", "line": line, "indent": left, "anchored": False}
        content = self._inlines(p, p, story, boxes, state)
        # At either end of a paragraph, a break belongs to the paragraph. It mustn't leave an
        # empty line behind, and CSS merges it with a break next to it, as Word's layout does.
        ending = re.search(re.escape(PAGE_BREAK) + r"(?:</span>|</a>)*$", content)
        if ending:
            content = content[:ending.start()] + ending.group()[len(PAGE_BREAK):]
            css.append("break-after:page")
        opening = re.match(r"((?:<span[^>]*>|<a [^>]*>)*)" + re.escape(PAGE_BREAK), content)
        if opening:
            content = opening.group(1) + content[opening.end():]
            css.append("break-before:page")
        hanging = number(attr(chain, "ind", "hanging"), 0) / 20
        first_line = -hanging if hanging else number(attr(chain, "ind", "firstLine"), 0) / 20
        content = self._apply_tabs(content, self._tab_stops(chain), left if marker else left + first_line, left,
                                   p, css, state)
        if state["anchored"]:
            css.append("position:relative")
        if line is None or (attr(chain, "spacing", "lineRule") or "auto") == "atLeast":
            # With a fixed height, the line's own strut has to match its text. If it sat on
            # a different baseline, the line would come out taller than the height asked for.
            first_run = p.find(w("r"))
            source = self._rpr_chain(first_run, p, story) if first_run is not None else self._mark_chain(p, story)
            css.append(self._run_css(source))
        direction = ' dir="rtl"' if is_on(first(chain, "bidi")) else ""
        style = ";".join(css)
        if marker or content:
            body = marker + content
        else:
            # Only here does the paragraph mark's formatting show: it decides how tall an
            # empty line is. It never applies to the text of the paragraph.
            body = f'<span style="{self._run_css(self._mark_chain(p, story), line)}">&#160;</span>'
        return f'<p{direction} style="{style}">{body}</p>' + "".join(boxes)

    def _mark_chain(self, p, story):
        ppr = p.find(w("pPr"))
        mark = ppr.find(w("rPr")) if ppr is not None else None
        return ([mark] if mark is not None else []) + self._rpr_chain(None, p, story)

    def _inlines(self, container, p, story, boxes, state=None) -> str:
        """Runs and the wrappers around them. `state` tracks complex fields (PAGE, TOC...)."""
        state = state if state is not None else {"in_instruction": False, "field": ""}
        out = []
        for child in container:
            if child.tag == w("r"):
                out.append(self._run(child, p, story, boxes, state))
            elif child.tag == w("hyperlink"):
                inner = self._inlines(child, p, story, boxes, state)
                # Links close around tabs, which are laid out later at the paragraph level.
                out.append(TAB.join(self._link(child, story, part) if part else ""
                                    for part in inner.split(TAB)))
            elif child.tag == f"{{{NS['m']}}}oMath":
                out.append(self._equation(child, p, story, state))
            elif child.tag == f"{{{NS['m']}}}oMathPara":
                equations = "".join(self._equation(item, p, story, state)
                                    for item in child.findall(f"{{{NS['m']}}}oMath"))
                out.append(f'<span style="display:block;text-align:center">{equations}</span>')
            elif child.tag == w("fldSimple"):
                inner = self._inlines(child, p, story, boxes, state)
                out.append(self._field(child.get(w("instr"), "")) or inner)
            elif child.tag == w("sdt"):
                content = child.find(w("sdtContent"))
                if content is not None:
                    out.append(self._inlines(content, p, story, boxes, state))
            elif child.tag in (w("ins"), w("smartTag"), w("customXml"), w("moveTo")):
                out.append(self._inlines(child, p, story, boxes, state))
        return "".join(out)

    def _equation(self, element, p, story, state) -> str:
        css = self._run_css(self._rpr_chain(None, p, story), state.get("line", 1.0))
        return (f'<span style="{css};font-family:\'Cambria Math\',\'Cambria\',\'Times New Roman\',serif;'
                f"font-feature-settings:'ccmp' 0\">{self._math(element)}</span>")

    def _math(self, element) -> str:
        """An equation as inline text: readable, though not typeset like Word's."""
        m = f"{{{NS['m']}}}"
        out = []

        def part(node, name):
            found = node.find(f"{m}{name}")
            return self._math(found) if found is not None else ""

        for child in element:
            tag = child.tag
            if tag == f"{m}r":
                out.append("".join(html.escape(t.text or "") for t in child.findall(f"{m}t")))
            elif tag == f"{m}f":
                out.append('<span style="display:inline-block;vertical-align:middle;text-align:center">'
                           f'<span style="display:block;border-bottom:0.5pt solid">{part(child, "num")}</span>'
                           f'<span style="display:block">{part(child, "den")}</span></span>')
            elif tag == f"{m}sSup":
                out.append(f'{part(child, "e")}<sup>{part(child, "sup")}</sup>')
            elif tag == f"{m}sSub":
                out.append(f'{part(child, "e")}<sub>{part(child, "sub")}</sub>')
            elif tag == f"{m}sSubSup":
                out.append(f'{part(child, "e")}<sub>{part(child, "sub")}</sub><sup>{part(child, "sup")}</sup>')
            elif tag == f"{m}nary":
                symbol = child.find(f"{m}naryPr/{m}chr")
                out.append(html.escape(symbol.get(f"{m}val", "∫")) if symbol is not None else "∫")
                out.append(f'<sub>{part(child, "sub")}</sub><sup>{part(child, "sup")}</sup> {part(child, "e")}')
            elif tag == f"{m}rad":
                out.append(f'√({part(child, "e")})')
            elif tag == f"{m}d":
                properties = child.find(f"{m}dPr")
                begin = properties.find(f"{m}begChr") if properties is not None else None
                end = properties.find(f"{m}endChr") if properties is not None else None
                body = "".join(self._math(item) for item in child.findall(f"{m}e"))
                out.append(html.escape(begin.get(f"{m}val", "(") if begin is not None else "(") + body
                           + html.escape(end.get(f"{m}val", ")") if end is not None else ")"))
            elif not tag.endswith("Pr"):
                out.append(self._math(child))
        return "".join(out)

    @staticmethod
    def _tab_stops(chain) -> list:
        """(position, kind, leader) of the tab stops in effect, sorted by position."""
        stops = {}
        for layer in reversed(chain):
            for tab in layer.findall(f"{w('tabs')}/{w('tab')}"):
                position = number(tab.get(w("pos")))
                if position is None:
                    continue
                if tab.get(w("val")) == "clear":
                    stops.pop(position, None)
                else:
                    stops[position] = (tab.get(w("val"), "left"), tab.get(w("leader"), "none"))
        return [(position / 20, *stops[position]) for position in sorted(stops)]

    def _apply_tabs(self, content, stops, start, left, p, css, state) -> str:
        """Lays out the tabs of a paragraph. Text widths aren't known here, so each tab takes
        the next stop in order, which holds for the usual cases: tables of contents,
        numbered entries and left, center and right headers."""
        segments = content.split(TAB)
        if len(segments) == 1:
            return content
        ahead = [stop for stop in stops if stop[0] > start + 0.1]
        if start < left and not any(stop[0] <= left + 0.1 for stop in ahead):
            ahead.insert(0, (left, "left", "none"))  # A hanging indent acts as a tab stop.

        spacer = f'<span style="display:inline-block;width:{DEFAULT_TAB_PT:g}pt"></span>'
        out, cursor, index = [], start, 0
        while index < len(segments) - 1:
            segment = segments[index]
            position, kind, leader = ahead.pop(0) if ahead else (None, "left", "none")
            if kind in ("right", "end", "decimal") and position is not None:
                out.append(segment)
                out.append(self._right_tab(spacer.join(segments[index + 1:]), position, leader, p, css))
                return "".join(out)
            if kind == "center" and position is not None:
                out.append(segment)
                state["anchored"] = True
                out.append(f'<span style="position:absolute;top:0;left:{-left:g}pt;width:{2 * position:g}pt;'
                           f'text-align:center">{segments[index + 1]}</span>')
                segments[index + 1] = ""
            elif position is None or len(plain_text(segment)) > MAX_TAB_LABEL_CHARS:
                out.append(segment + spacer)
            else:
                out.append(f'<span style="display:inline-block;white-space:nowrap;'
                           f'min-width:{position - cursor:g}pt">{segment}</span>')
                cursor = position
            index += 1
        out.append(segments[-1])
        return "".join(out)

    def _right_tab(self, rest, position, leader, p, css) -> str:
        """Text aligned to the right, with its leader filling the space before it."""
        if p.getparent().tag != w("tc"):
            inside = self.page.width - self.page.left - self.page.right - position
            if inside > 1:
                css.append(f"margin-right:{inside:g}pt")
        kind = LEADERS.get(leader)
        if "<img" in rest or 'class="page-' in rest or kind is None:
            return f'<span style="float:right">{rest}</span>'
        run_style = re.search(r'style="([^"]*)"', rest)
        style = f' style="{run_style.group(1)}"' if run_style else ""
        return (f'<span class="{kind}"{style} '
                f'data-right="{html.escape(plain_text(rest), quote=True)}"></span>')

    @staticmethod
    def _link(element, story, inner: str) -> str:
        target = None
        rid = element.get(R_ID)
        if rid and rid in story.part.rels and story.part.rels[rid].is_external:
            target = story.part.rels[rid].target_ref
        if target and re.match(r"(https?:|mailto:)", target, re.IGNORECASE):
            return f'<a href="{html.escape(target, quote=True)}">{inner}</a>'
        return inner

    @staticmethod
    def _field(instruction: str):
        name = instruction.strip().split(" ")[0].upper() if instruction.strip() else ""
        if name == "PAGE":
            return '<span class="page-number"></span>'
        if name == "NUMPAGES":
            return '<span class="page-count"></span>'
        return None

    def _run(self, r, p, story, boxes, state) -> str:
        parts = []
        for child in r:
            tag = child.tag
            if tag == w("fldChar"):
                kind = child.get(w("fldCharType"))
                if kind == "begin":
                    state.update(in_instruction=True, field="", replaced=False)
                elif kind == "separate":
                    state["in_instruction"] = False
                    replacement = self._field(state["field"])
                    if replacement:
                        parts.append(replacement)
                        state["replaced"] = True
                elif kind == "end":
                    if state["in_instruction"]:  # A field with no cached result.
                        parts.append(self._field(state["field"]) or "")
                    state.update(in_instruction=False, replaced=False)
            elif tag == w("instrText"):
                state["field"] += child.text or ""
            elif state["in_instruction"] or state.get("replaced"):
                continue
            elif tag == w("t"):
                parts.append(self._preserve_spaces(child.text or "", state))
            elif tag == w("tab"):
                parts.append(TAB)
            elif tag in (w("br"), w("cr")):
                kind = child.get(w("type"))
                if kind == "page":
                    parts.append(PAGE_BREAK)
                elif kind == "column":
                    parts.append('<span style="display:block;break-after:column"></span>')
                else:
                    parts.append("<br>")
            elif tag == w("noBreakHyphen"):
                parts.append("‑")
            elif tag == w("softHyphen"):
                parts.append("­")
            elif tag in (w("footnoteReference"), w("endnoteReference")):
                kind = "footnote" if tag == w("footnoteReference") else "endnote"
                parts.append(self._note(kind, child.get(w("id"))))
            elif tag in (w("drawing"), w("pict"), f"{{{NS['mc']}}}AlternateContent"):
                parts.append(self._images(child, story, state))
                self._text_boxes(child, story, boxes)
        content = "".join(parts)
        if not content:
            return ""
        css = self._run_css(self._rpr_chain(r, p, story), state.get("line", 1.0))
        return TAB.join(f'<span style="{css}">{part}</span>' if part else "" for part in content.split(TAB))

    @staticmethod
    def _preserve_spaces(text: str, state) -> str:
        """Keeps the spaces HTML would collapse: repeated ones and those starting a paragraph.
        A real space stays first, so that a line can still break there."""
        text = text.replace(TAB, "")
        if not state.get("started"):
            stripped = text.lstrip(" ")
            text = "\u00a0" * (len(text) - len(stripped)) + stripped
            state["started"] = bool(text)
        return re.sub(r" {2,}", lambda spaces: " " + "\u00a0" * (len(spaces.group()) - 1), html.escape(text))

    def _note(self, kind, note_id) -> str:
        text = self.notes.get((kind, note_id))
        if not text:
            return ""
        return f'<span class="note">{html.escape(text)}</span>'

    def _images(self, element, story, state) -> str:
        # Alternate content repeats itself in mc:Fallback; only mc:Choice counts.
        out = [self._frame(frame, story, state) for frame in FIND_FRAMES(element)]
        for image in FIND_VML_IMAGES(element):
            out.append(self._image(image.get(R_ID), story, None))
        return "".join(out)

    def _frame(self, frame, story, state) -> str:
        """A drawing: one picture or a group of them, inline or anchored to the page."""
        extent = frame.find(f"{{{NS['wp']}}}extent")
        width = number(extent.get("cx"), 0) / EMU_PER_PT if extent is not None else 0
        height = number(extent.get("cy"), 0) / EMU_PER_PT if extent is not None else 0
        pictures = self._pictures(frame, width, height)
        if not pictures:
            line = self._line_shape(frame, height)
            if line and frame.tag == f"{{{NS['wp']}}}anchor":
                return f'<span style="{self._anchor_css(frame, width, height, story, state)}width:{width:g}pt;{line}"></span>'
            return ""  # Any other shape: the text it may hold is handled as a text box.

        inline = frame.tag == f"{{{NS['wp']}}}inline"
        if len(pictures) == 1 and inline:
            rid, _, _, picture_width, picture_height = pictures[0]
            return self._image(rid, story, (picture_width, picture_height))

        content = "".join(
            self._image(rid, story, (w_, h_), f"position:absolute;left:{x:g}pt;top:{y:g}pt;")
            for rid, x, y, w_, h_ in pictures
        )
        size = f"width:{width:g}pt;height:{height:g}pt;"
        if inline:
            return f'<span style="display:inline-block;position:relative;{size}">{content}</span>'
        css = self._anchor_css(frame, width, height, story, state)
        return f'<span style="{css}{size}">{content}</span>'

    @staticmethod
    def _line_shape(frame, height) -> str:
        """CSS for a drawn straight line, the shape documents use the most as a divider."""
        shape = frame.find(f".//{{{NS['wps']}}}wsp")
        geometry = shape.find(f".//{{{NS['a']}}}prstGeom") if shape is not None else None
        if geometry is None or "line" not in geometry.get("prst", "").lower() and "connector" not in geometry.get("prst", "").lower():
            return ""
        outline = shape.find(f".//{{{NS['a']}}}ln")
        thickness = number(outline.get("w"), 9525) / EMU_PER_PT if outline is not None else 0.75
        color = None
        if outline is not None:
            fill = outline.find(f"{{{NS['a']}}}solidFill/{{{NS['a']}}}srgbClr")
            color = css_color(fill.get("val")) if fill is not None else None
        return f"height:{height:g}pt;border-top:{thickness:g}pt solid {color or '#000000'};"

    @staticmethod
    def _pictures(frame, width, height) -> list:
        """(relationship id, x, y, width, height) of each picture, in points from the frame."""
        graphic = frame.find(f".//{{{NS['a']}}}graphicData")
        if graphic is None:
            return []
        group = graphic.find(f"{{{NS['wpg']}}}wgp")
        if group is None:
            blip = next(iter(FIND_BLIPS(graphic)), None)
            return [(blip.get(R_EMBED), 0, 0, width, height)] if blip is not None else []

        a, pic, wpg = f"{{{NS['a']}}}", f"{{{NS['pic']}}}", f"{{{NS['wpg']}}}"

        def transform(xfrm, parent):
            """Composes a group's own box onto its parent: (x, y, scale_x, scale_y)."""
            x, y, scale_x, scale_y = parent
            off, ext = xfrm.find(f"{a}off"), xfrm.find(f"{a}ext")
            child_off, child_ext = xfrm.find(f"{a}chOff"), xfrm.find(f"{a}chExt")
            ratio_x = number(ext.get("cx"), 1) / (number(child_ext.get("cx"), 1) or 1)
            ratio_y = number(ext.get("cy"), 1) / (number(child_ext.get("cy"), 1) or 1)
            return (x + (number(off.get("x"), 0) - number(child_off.get("x"), 0) * ratio_x) * scale_x,
                    y + (number(off.get("y"), 0) - number(child_off.get("y"), 0) * ratio_y) * scale_y,
                    scale_x * ratio_x, scale_y * ratio_y)

        def walk(container, parent, found):
            for child in container:
                if child.tag == f"{pic}pic":
                    xfrm = child.find(f"{pic}spPr/{a}xfrm")
                    blip = child.find(f"{pic}blipFill/{a}blip")
                    if xfrm is None or blip is None:
                        continue
                    off, ext = xfrm.find(f"{a}off"), xfrm.find(f"{a}ext")
                    x, y, scale_x, scale_y = parent
                    found.append((blip.get(R_EMBED), x + number(off.get("x"), 0) * scale_x,
                                  y + number(off.get("y"), 0) * scale_y,
                                  number(ext.get("cx"), 0) * scale_x, number(ext.get("cy"), 0) * scale_y))
                elif child.tag == f"{wpg}grpSp":
                    xfrm = child.find(f"{wpg}grpSpPr/{a}xfrm")
                    if xfrm is not None:
                        walk(child, transform(xfrm, parent), found)
            return found

        xfrm = group.find(f"{wpg}grpSpPr/{a}xfrm")
        if xfrm is None:
            return []
        # The top group's box maps onto the frame, in points.
        ext = xfrm.find(f"{a}ext")
        off = xfrm.find(f"{a}off")
        to_frame = (-number(off.get("x"), 0) * width / (number(ext.get("cx"), 1) or 1),
                    -number(off.get("y"), 0) * height / (number(ext.get("cy"), 1) or 1),
                    width / (number(ext.get("cx"), 1) or 1), height / (number(ext.get("cy"), 1) or 1))
        return walk(group, transform(xfrm, to_frame), [])

    def _anchor_css(self, frame, width, height, story, state) -> str:
        """Where a floating drawing goes. Pages aren't known while building the HTML, so
        vertical offsets from the page only hold in headers; in the body the drawing
        stays by its paragraph."""
        page, wp = self.page, f"{{{NS['wp']}}}"
        column = page.width - page.left - page.right
        horizontal, vertical = frame.find(f"{wp}positionH"), frame.find(f"{wp}positionV")

        def offset(position):
            found = position.find(f"{wp}posOffset") if position is not None else None
            return number(found.text if found is not None else None, 0) / EMU_PER_PT

        x = offset(horizontal)
        relative = horizontal.get("relativeFrom", "column") if horizontal is not None else "column"
        align = horizontal.find(f"{wp}align") if horizontal is not None else None
        span, origin = (page.width, -page.left) if relative == "page" else (column, 0.0)
        if align is not None:
            x = {"center": (span - width) / 2, "right": span - width}.get(align.text, 0.0)
        x += origin - state["indent"]

        y = offset(vertical)
        relative = vertical.get("relativeFrom", "paragraph") if vertical is not None else "paragraph"
        if relative in ("page", "margin", "topMargin"):
            base = page.top if relative == "margin" else 0.0
            y = y + base - page.header if story.in_header else 0.0

        distance = {side: number(frame.get(f"dist{side}"), 0) / EMU_PER_PT for side in "TBLR"}
        if frame.find(f"{wp}wrapTopAndBottom") is not None:
            return (f"display:block;position:relative;margin-left:{x:g}pt;"
                    f"margin-top:{distance['T']:g}pt;margin-bottom:{distance['B']:g}pt;")
        if any(frame.find(f"{wp}{wrap}") is not None for wrap in ("wrapSquare", "wrapTight", "wrapThrough")):
            side = "left" if x + width / 2 < column / 2 else "right"
            return (f"display:block;position:relative;float:{side};margin:{distance['T']:g}pt "
                    f"{distance['R']:g}pt {distance['B']:g}pt {distance['L']:g}pt;")

        state["anchored"] = True
        layer = "z-index:-1;" if frame.get("behindDoc") in ("1", "true") else "z-index:1;"
        return f"display:block;position:absolute;left:{x:g}pt;top:{y:g}pt;{layer}"

    @staticmethod
    def _image(rid, story, size, extra="") -> str:
        # Only embedded parts: a linked image (`r:link`) would mean opening an outside URL.
        part = story.part.related_parts.get(rid) if rid else None
        if part is None or part.content_type not in IMAGE_TYPES:
            return ""
        data = base64.b64encode(part.blob).decode("ascii")
        css = f"width:{size[0]:g}pt;height:{size[1]:g}pt;" if size and size[0] and size[1] else "max-width:100%;"
        return f'<img style="{extra}{css}" src="data:{part.content_type};base64,{data}" alt="">'

    def _text_boxes(self, element, story, boxes) -> None:
        """Floating text boxes can't be positioned here, but their text is kept after the paragraph."""
        for content in FIND_TEXT_BOXES(element):
            boxes.append(f'<div class="text-box">{self.blocks(content, story)}</div>')

    # ── Tables ───────────────────────────────────────────────

    def _table_style_layers(self, tbl, row_index, row_count, cell_index, cell_count):
        """Table style formatting for one cell: (pPr layers, rPr layers, tcPr layers)."""
        tbl_pr = tbl.find(w("tblPr"))
        style = tbl_pr.find(w("tblStyle")) if tbl_pr is not None else None
        style_id = style.get(w("val")) if style is not None else None
        if style_id is None:
            default = self.document.styles.default(WD_STYLE_TYPE.TABLE)
            style_id = default.style_id if default is not None else None

        conditions = []
        look = tbl_pr.find(w("tblLook")) if tbl_pr is not None else None

        def enabled(name, bit):
            if look is None:
                return True
            value = look.get(w(name))
            if value is not None:
                return value in ("1", "true", "on")
            return bool(int(look.get(w("val"), "0"), 16) & bit)

        no_h_band = not enabled("noHBand", 0x0200) if look is not None else True
        if enabled("firstRow", 0x0020) and row_index == 0:
            conditions.append("firstRow")
        if enabled("lastRow", 0x0040) and row_index == row_count - 1 and row_count > 1:
            conditions.append("lastRow")
        if enabled("firstColumn", 0x0080) and cell_index == 0:
            conditions.append("firstCol")
        if enabled("lastColumn", 0x0100) and cell_index == cell_count - 1:
            conditions.append("lastCol")
        if no_h_band:
            offset = 1 if enabled("firstRow", 0x0020) else 0
            if row_index >= offset:
                conditions.append("band1Horz" if (row_index - offset) % 2 == 0 else "band2Horz")

        ppr, rpr, tcpr = [], [], []
        seen = set()
        current = style_id
        styles = self.document.styles.element
        while current and current not in seen:
            seen.add(current)
            definition = next((s for s in styles.findall(w("style")) if s.get(w("styleId")) == current), None)
            if definition is None:
                break
            # Strongest condition first; those beat the style's own plain formatting.
            for condition in conditions:
                for conditional in definition.findall(w("tblStylePr")):
                    if conditional.get(w("type")) == condition:
                        for tag, target in (("pPr", ppr), ("rPr", rpr), ("tcPr", tcpr)):
                            found = conditional.find(w(tag))
                            if found is not None:
                                target.append(found)
            for tag, target in (("pPr", ppr), ("rPr", rpr), ("tcPr", tcpr)):
                found = definition.find(w(tag))
                if found is not None:
                    target.append(found)
            based = definition.find(w("basedOn"))
            current = based.get(w("val")) if based is not None else None
        return ppr, rpr, tcpr

    def _table_borders(self, tbl):
        """Table-level borders, direct over style."""
        tbl_pr = tbl.find(w("tblPr"))
        layers = []
        if tbl_pr is not None:
            layers.append(tbl_pr)
            style = tbl_pr.find(w("tblStyle"))
            if style is not None:
                layers += self._style_chain(style.get(w("val")), "table", "tblPr")
        return layers

    def _cell_margins(self, layers):
        margins = {"top": 0.0, "left": 5.4, "bottom": 0.0, "right": 5.4}
        for layer in reversed(layers):
            found = layer.find(w("tblCellMar"))
            if found is None:
                continue
            for side, names in (("top", ("top",)), ("left", ("left", "start")),
                                ("bottom", ("bottom",)), ("right", ("right", "end"))):
                for name in names:
                    edge = found.find(w(name))
                    value = number(edge.get(w("w"))) if edge is not None else None
                    if value is not None:
                        margins[side] = value / 20
        return margins

    def table(self, tbl, story) -> str:
        layers = self._table_borders(tbl)
        table_borders = first(layers, "tblBorders")
        margins = self._cell_margins(layers)
        grid = [number(col.get(w("w")), 0) / 20 for col in tbl.findall(f"{w('tblGrid')}/{w('gridCol')}")]
        rows = tbl.findall(w("tr"))
        spans = self._row_spans(rows)

        css = ["border-collapse:collapse", "table-layout:fixed"]
        if grid and sum(grid):
            css.append(f"width:{sum(grid):g}pt")
        align = attr(layers, "jc", "val")
        if align == "center":
            css.append("margin-left:auto;margin-right:auto")
        elif align in ("right", "end"):
            css.append("margin-left:auto")
        else:
            indent = number(attr(layers, "tblInd", "w"), 0)
            if indent:
                css.append(f"margin-left:{indent / 20:g}pt")

        head, body = [], []
        colgroup = "".join(f'<col style="width:{width:g}pt">' for width in grid)
        for row_index, row in enumerate(rows):
            html_row = self._row(row, row_index, rows, tbl, story, table_borders, margins, spans)
            tr_pr = row.find(w("trPr"))
            is_header = tr_pr is not None and is_on(tr_pr.find(w("tblHeader")))
            (head if is_header and not body else body).append(html_row)
        thead = f"<thead>{''.join(head)}</thead>" if head else ""
        return (f'<table style="{";".join(css)}"><colgroup>{colgroup}</colgroup>{thead}'
                f"<tbody>{''.join(body)}</tbody></table>")

    @staticmethod
    def _row_spans(rows):
        """Rowspan of each vMerge restart, keyed by (row, grid column)."""
        spans = {}
        open_spans = {}
        for row_index, row in enumerate(rows):
            column = 0
            tr_pr = row.find(w("trPr"))
            before = tr_pr.find(w("gridBefore")) if tr_pr is not None else None
            if before is not None:
                column += int(number(before.get(w("val")), 0))
            for tc in row.findall(w("tc")):
                tc_pr = tc.find(w("tcPr"))
                span = tc_pr.find(w("gridSpan")) if tc_pr is not None else None
                width = int(number(span.get(w("val")), 1)) if span is not None else 1
                merge = tc_pr.find(w("vMerge")) if tc_pr is not None else None
                if merge is not None and merge.get(w("val")) != "restart":
                    start = open_spans.get(column)
                    if start is not None:
                        spans[start] += 1
                elif merge is not None:
                    open_spans[column] = (row_index, column)
                    spans[(row_index, column)] = 1
                else:
                    open_spans.pop(column, None)
                column += width
        return spans

    def _row(self, row, row_index, rows, tbl, story, table_borders, margins, spans) -> str:
        tr_pr = row.find(w("trPr"))
        css = []
        if tr_pr is not None:
            if is_on(tr_pr.find(w("cantSplit"))):
                css.append("break-inside:avoid")
            height = tr_pr.find(w("trHeight"))
            value = number(height.get(w("val"))) if height is not None else None
            if value:
                css.append(f"height:{value / 20:g}pt")

        cells = []
        column = 0
        before = tr_pr.find(w("gridBefore")) if tr_pr is not None else None
        if before is not None:
            column = int(number(before.get(w("val")), 0))
            if column:
                cells.append(f'<td colspan="{column}"></td>')
        tcs = row.findall(w("tc"))
        for cell_index, tc in enumerate(tcs):
            tc_pr = tc.find(w("tcPr"))
            span = tc_pr.find(w("gridSpan")) if tc_pr is not None else None
            width = int(number(span.get(w("val")), 1)) if span is not None else 1
            merge = tc_pr.find(w("vMerge")) if tc_pr is not None else None
            if merge is not None and merge.get(w("val")) != "restart":
                column += width
                continue
            rowspan = spans.get((row_index, column), 1)
            cells.append(self._cell(tc, tc_pr, tbl, story, table_borders, margins, row_index, len(rows),
                                    cell_index, len(tcs), width, rowspan))
            column += width
        style = f' style="{";".join(css)}"' if css else ""
        return f"<tr{style}>{''.join(cells)}</tr>"

    def _cell(self, tc, tc_pr, tbl, story, table_borders, margins, row_index, row_count,
              cell_index, cell_count, colspan, rowspan) -> str:
        ppr, rpr, tcpr = self._table_style_layers(tbl, row_index, row_count, cell_index, cell_count)
        layers = ([tc_pr] if tc_pr is not None else []) + tcpr

        css = [f"padding:{margins['top']:g}pt {margins['right']:g}pt {margins['bottom']:g}pt {margins['left']:g}pt"]
        edges = {
            "top": "top" if row_index == 0 else "insideH",
            "bottom": "bottom" if row_index == row_count - 1 else "insideH",
            "left": "left" if cell_index == 0 else "insideV",
            "right": "right" if cell_index == cell_count - 1 else "insideV",
        }
        cell_borders = first(layers, "tcBorders")
        for side, table_edge in edges.items():
            element = cell_borders.find(w(side)) if cell_borders is not None else None
            if element is None and table_borders is not None:
                element = table_borders.find(w(table_edge))
            if element is not None:
                css.append(f"border-{side}:{self._border_css(element)}")
        fill = css_color(attr(layers, "shd", "fill"))
        if fill:
            css.append(f"background-color:{fill}")
        vertical = {"center": "middle", "bottom": "bottom"}.get(attr(layers, "vAlign", "val"), "top")
        css.append(f"vertical-align:{vertical}")

        cell_story = Story(story.part, ppr, rpr)
        attributes = f' style="{";".join(css)}"'
        if colspan > 1:
            attributes += f' colspan="{colspan}"'
        if rowspan > 1:
            attributes += f' rowspan="{rowspan}"'
        return f"<td{attributes}>{self.blocks(tc, cell_story)}</td>"

    # ── Document ─────────────────────────────────────────────

    def blocks(self, container, story) -> str:
        out = []
        for child in container:
            if child.tag == w("p"):
                out.append(self.paragraph(child, story))
            elif child.tag == w("tbl"):
                out.append(self.table(child, story))
            elif child.tag == w("sdt"):
                content = child.find(w("sdtContent"))
                if content is not None:
                    out.append(self.blocks(content, story))
            elif child.tag in (w("ins"), w("customXml"), w("moveTo")):
                out.append(self.blocks(child, story))
        return "".join(out)

    def _body_sections(self):
        """Splits the body at section breaks: a paragraph's sectPr closes its section."""
        body = self.document.element.body
        sections, current = [], []
        for child in body:
            if child.tag == w("sectPr"):
                continue
            current.append(child)
            if child.tag == w("p"):
                ppr = child.find(w("pPr"))
                if ppr is not None and ppr.find(w("sectPr")) is not None:
                    sections.append(current)
                    current = []
        sections.append(current)
        return sections

    @staticmethod
    def _is_continuous(section) -> bool:
        kind = section._sectPr.find(w("type"))
        return kind is not None and kind.get(w("val")) == "continuous"

    def _margin_box(self, proxy, name, css_class) -> str:
        """A header/footer as a running element for `@page` margin boxes."""
        story = Story(proxy.part, in_header=True)
        content = self.blocks(proxy._element, story)
        return f'<div class="{css_class}" style="position:running({name})">{content}</div>'

    def _page_rule(self, index, section) -> tuple:
        """@page rules for one section plus its running header/footer elements."""
        sect_pr = section._sectPr
        size = sect_pr.find(w("pgSz"))
        margin = sect_pr.find(w("pgMar"))
        page_w = number(size.get(w("w")), 12240) / 20 if size is not None else 612
        page_h = number(size.get(w("h")), 15840) / 20 if size is not None else 792

        def edge(name, default):
            value = margin.get(w(name)) if margin is not None else None
            return abs(number(value, default * 20)) / 20

        self.page = Page(page_w, edge("left", 72), edge("right", 72), edge("top", 72), edge("header", 36))
        rule = [f"size:{page_w:g}pt {page_h:g}pt",
                f"margin:{edge('top', 72):g}pt {edge('right', 72):g}pt {edge('bottom', 72):g}pt {edge('left', 72):g}pt"]
        elements = [self._margin_box(section.header, f"header{index}", "page-header"),
                    self._margin_box(section.footer, f"footer{index}", "page-footer")]
        rule.append(f"@top-left{{content:element(header{index});width:100%;vertical-align:top;"
                    f"padding-top:{edge('header', 36):g}pt}}")
        rule.append(f"@bottom-left{{content:element(footer{index});width:100%;vertical-align:bottom;"
                    f"padding-bottom:{edge('footer', 36):g}pt}}")
        css = f"@page s{index}{{{';'.join(rule)}}}"

        if index == 0 and section.different_first_page_header_footer:
            elements += [self._margin_box(section.first_page_header, "headerFirst", "page-header"),
                         self._margin_box(section.first_page_footer, "footerFirst", "page-footer")]
            css += ("@page s0:first{@top-left{content:element(headerFirst)}"
                    "@bottom-left{content:element(footerFirst)}}")
        return css, "".join(elements)

    def build(self) -> str:
        story = Story(self.document.part)
        rules, sections_html = [], []
        groups = self._body_sections()
        word_sections = list(self.document.sections)
        page_index = 0
        for index, group in enumerate(groups):
            section = word_sections[min(index, len(word_sections) - 1)]
            continuous = index > 0 and self._is_continuous(section)
            if continuous:
                # It stays on the current page, which keeps its size. Reusing the running
                # names gives later pages this section's header, as Word does.
                running = (self._margin_box(section.header, f"header{page_index}", "page-header")
                           + self._margin_box(section.footer, f"footer{page_index}", "page-footer"))
                position = ""
            else:
                page_index = index
                css, running = self._page_rule(index, section)
                rules.append(css)
                position = "break-before:page;" if index > 0 else ""
            columns = section._sectPr.find(w("cols"))
            count = int(number(columns.get(w("num")), 1)) if columns is not None else 1
            column_css = ""
            if count > 1:
                gap = number(columns.get(w("space")), 720) / 20
                column_css = f"column-count:{count};column-gap:{gap:g}pt;"
            content = self.blocks(group, story)
            sections_html.append(
                f'<section style="page:s{page_index};{position}{column_css}">{running}{content}</section>'
            )

        base = self._run_css(self.default_rpr)
        stylesheet = f"""
        {''.join(rules)}
        html {{ {base} }}
        body {{ margin:0; }}
        p {{ margin:0; font-size:1pt; overflow-wrap:break-word; }}
        td > p:first-child {{ margin-top:0 !important; }}
        td > p:last-child {{ margin-bottom:0 !important; }}
        a {{ color:inherit; text-decoration:none; }}
        sup, sub {{ font-size:0.7em; line-height:0; }}
        .tab-dot::after {{ content:leader('.') attr(data-right); }}
        .tab-hyphen::after {{ content:leader('-') attr(data-right); }}
        .tab-line::after {{ content:leader('_') attr(data-right); }}
        table {{ max-width:100%; }}
        img {{ vertical-align:baseline; }}
        .note {{ float:footnote; font-size:0.85em; white-space:normal; }}
        .text-box {{ margin:6pt 0; }}
        .page-number::after {{ content:counter(page); }}
        .page-count::after {{ content:counter(pages); }}
        """
        return (f'<!doctype html><html><head><meta charset="utf-8"><style>{stylesheet}</style></head>'
                f"<body>{''.join(sections_html)}</body></html>")


def load(source: str):
    try:
        return Document(source)
    except (PackageNotFoundError, zipfile.BadZipFile, KeyError, ValueError, etree.XMLSyntaxError) as error:
        raise EngineError("unsupported_format", f"not a readable .docx file: {error}") from error


def build_html(source: str) -> str:
    return HtmlBuilder(load(source)).build()


def write_pdf(markup: str, output) -> None:
    use_bundled_libraries()
    # Imported here because it needs Pango, which HTML generation doesn't.
    from weasyprint import HTML
    from weasyprint.urls import URLFetcher

    fetcher = URLFetcher(allowed_protocols=["data"], allow_redirects=False)
    HTML(string=markup, url_fetcher=fetcher).write_pdf(output)


def convert(source: str, output: str) -> None:
    write_pdf(build_html(source), output)

