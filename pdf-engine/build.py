"""Builds the engine as two folder bundles under src-tauri/pdf-engine-dist/minos-pdf.

There's one per direction because their libraries conflict: OpenCV, which PDF -> DOCX needs,
ships older copies of libraries that Pango, which DOCX -> PDF needs, also uses. On macOS,
PyInstaller would make Pango load those copies, which lack functions it requires.

Run with the Python environment that has requirements.in and requirements-dev.txt installed.
DOCX -> PDF bundles Pango, so the build machine needs it too and the linker has to find it.
On Windows, that means having its DLLs on PATH (e.g. C:\\msys64\\ucrt64\\bin).

The bundles are for the machine that builds them: PyInstaller can't cross-compile.
"""

import argparse
import ctypes
import json
import os
import subprocess
import tempfile
from pathlib import Path

import PyInstaller.__main__
import pymupdf
from docx import Document

ROOT = Path(__file__).resolve().parent
DEFAULT_DIST = ROOT.parent / "src-tauri" / "pdf-engine-dist"
# Pulled in by dependencies but never reached by the conversions.
EXCLUDES = ["tkinter", "matplotlib", "pytest", "IPython", "scipy", "pandas", "fire"]
TO_DOCX, TO_PDF = "minos-to-docx", "minos-to-pdf"
BUNDLE_EXCLUDES = {
    TO_DOCX: ["weasyprint"],
    TO_PDF: ["cv2", "numpy", "pymupdf", "fitz", "pdf2docx"],
}
# What WeasyPrint loads at run time, which has to be inside its bundle on every system.
REQUIRED_LIBRARIES = ["gobject-2.0", "pango-1.0", "pangoft2-1.0", "harfbuzz", "fontconfig"]


def library_report(bundle: Path) -> str:
    """Why a bundled library won't load, which WeasyPrint's own error doesn't say: it only
    reports the first name it tried."""
    lines = []
    for library in REQUIRED_LIBRARIES:
        for path in sorted((bundle / "_internal").glob(f"*{library}*")):
            try:
                ctypes.CDLL(str(path))
                status = "loads"
            except OSError as error:
                status = f"FAILS: {error}"
            lines.append(f"{path.name} ({'link' if path.is_symlink() else 'file'}): {status}")
    return "\n".join(lines)


def run_bundle(bundle: Path, command: str, source: Path, output: Path) -> None:
    """Runs a conversion without this machine's Pango, as it would on a user's machine."""
    environment = {
        name: value
        for name, value in os.environ.items()
        if name not in ("WEASYPRINT_DLL_DIRECTORIES", "FONTCONFIG_PATH", "FONTCONFIG_FILE", "DYLD_FALLBACK_LIBRARY_PATH")
    }
    if os.name == "nt":
        environment["PATH"] = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32")
    executable = bundle / (f"{bundle.name}.exe" if os.name == "nt" else bundle.name)
    result = subprocess.run(
        [str(executable), command, "--input", str(source), "--output", str(output)],
        capture_output=True,
        encoding="utf-8",
        errors="replace",
        timeout=300,
        env=environment,
    )
    lines = result.stdout.strip().splitlines()
    try:
        report = json.loads(lines[-1]) if lines else {}
    except ValueError:
        report = {}
    if not report.get("ok") or not output.is_file():
        raise SystemExit(
            f"{bundle.name} can't do {command}: {result.stdout}\n{result.stderr}\n{library_report(bundle)}"
        )


def smoke_test(dist: Path) -> None:
    """Fails the build if a bundle can't convert: missing files only show up when it runs."""
    with tempfile.TemporaryDirectory() as work:
        folder = Path(work)
        pdf = pymupdf.open()
        pdf.new_page().insert_text((72, 90), "Prueba del motor")
        pdf.save(folder / "señor.pdf")
        pdf.close()
        run_bundle(dist / TO_DOCX, "pdf-to-docx", folder / "señor.pdf", folder / "señor.docx")

        document = Document()
        document.add_paragraph("Señor, ¿cómo está? áéíóú")
        document.save(folder / "informe ñ.docx")
        run_bundle(dist / TO_PDF, "docx-to-pdf", folder / "informe ñ.docx", folder / "informe ñ.pdf")
        with pymupdf.open(folder / "informe ñ.pdf") as rendered:
            text = rendered[0].get_text()
        if "Señor, ¿cómo está? áéíóú" not in text:
            raise SystemExit(f"{TO_PDF} lost text rendering a PDF: {ascii(text)}")


def build_bundle(name: str, dist: Path) -> None:
    data = ("fonts-windows.conf", "fonts-posix.conf") if name == TO_PDF else ()
    with tempfile.TemporaryDirectory() as work:
        PyInstaller.__main__.run(
            [
                str(ROOT / "entry.py"),
                f"--name={name}",
                "--onedir",
                "--console",  # The app reads stdout and hides the window itself.
                "--noconfirm",
                "--clean",
                f"--paths={ROOT}",
                f"--distpath={dist}",
                f"--workpath={work}",
                f"--specpath={work}",
                *(f"--exclude-module={module}" for module in EXCLUDES + BUNDLE_EXCLUDES[name]),
                *(f"--add-data={ROOT / 'minos_pdf' / file}{os.pathsep}minos_pdf" for file in data),
            ]
        )

    bundle = dist / name
    # python-docx opens its templates through docx/parts/../templates. The code of that folder
    # is in PyInstaller's archive, so it isn't on disk, and Linux and macOS won't resolve a path
    # through a missing folder. The file keeps installers from dropping it for being empty.
    parts = bundle / "_internal" / "docx" / "parts"
    parts.mkdir(parents=True, exist_ok=True)
    (parts / ".keep").touch()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist", type=Path, default=DEFAULT_DIST)
    args = parser.parse_args()

    dist = args.dist / "minos-pdf"
    for name in (TO_DOCX, TO_PDF):
        build_bundle(name, dist)

    # OpenCV's video codecs: about 30 MB that pdf2docx's image handling never loads.
    for codec in (dist / TO_DOCX).rglob("opencv_videoio_ffmpeg*"):
        codec.unlink()
    # The conversion test alone wouldn't notice: it could be finding this machine's copies.
    for library in REQUIRED_LIBRARIES:
        if not any(library in path.name for path in (dist / TO_PDF / "_internal").iterdir()):
            raise SystemExit(f"{TO_PDF} is missing {library}. Is Pango where the linker can find it?")
    # Tauri needs the resource folder to exist, so this placeholder is tracked by git.
    (dist / ".gitkeep").touch()
    smoke_test(dist)


if __name__ == "__main__":
    main()
