"""Builds the engine as a folder bundle under src-tauri/pdf-engine-dist/minos-pdf.

Run with the Python environment that has requirements.in and requirements-dev.txt
installed. DOCX -> PDF needs Pango, which goes inside the bundle, so the build machine
needs it too and the linker has to find it. On Windows, that means having its DLLs on
PATH (e.g. C:\\msys64\\ucrt64\\bin).

The bundle is for the machine that builds it: PyInstaller can't cross-compile.
"""

import argparse
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
# What WeasyPrint loads at run time, which has to be inside the bundle on every system.
REQUIRED_LIBRARIES = ["gobject-2.0", "pango-1.0", "pangoft2-1.0", "harfbuzz", "fontconfig"]


def smoke_test(bundle: Path) -> None:
    """Fails the build if the bundle can't convert: missing files only show up when it runs.

    It runs without this machine's Pango, as it would on a user's machine.
    """
    executable = bundle / ("minos-pdf.exe" if os.name == "nt" else "minos-pdf")
    environment = {
        name: value
        for name, value in os.environ.items()
        if name not in ("WEASYPRINT_DLL_DIRECTORIES", "FONTCONFIG_PATH", "FONTCONFIG_FILE", "DYLD_FALLBACK_LIBRARY_PATH")
    }
    if os.name == "nt":
        environment["PATH"] = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32")

    def run(command: str, source: Path, output: Path) -> None:
        result = subprocess.run(
            [str(executable), command, "--input", str(source), "--output", str(output)],
            capture_output=True,
            encoding="utf-8",
            errors="replace",
            timeout=300,
            env=environment,
        )
        lines = result.stdout.strip().splitlines()
        report = json.loads(lines[-1]) if lines else {}
        if not report.get("ok") or not output.is_file():
            raise SystemExit(f"The engine bundle can't do {command}: {result.stdout}\n{result.stderr}")

    with tempfile.TemporaryDirectory() as work:
        folder = Path(work)
        pdf = pymupdf.open()
        pdf.new_page().insert_text((72, 90), "Prueba del motor")
        pdf.save(folder / "señor.pdf")
        pdf.close()
        run("pdf-to-docx", folder / "señor.pdf", folder / "señor.docx")

        document = Document()
        document.add_paragraph("Señor, ¿cómo está? áéíóú")
        document.save(folder / "informe ñ.docx")
        run("docx-to-pdf", folder / "informe ñ.docx", folder / "informe ñ.pdf")
        with pymupdf.open(folder / "informe ñ.pdf") as rendered:
            text = rendered[0].get_text()
        if "Señor, ¿cómo está? áéíóú" not in text:
            raise SystemExit(f"The engine bundle lost text rendering a PDF: {ascii(text)}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist", type=Path, default=DEFAULT_DIST)
    args = parser.parse_args()

    with tempfile.TemporaryDirectory() as work:
        PyInstaller.__main__.run(
            [
                str(ROOT / "entry.py"),
                "--name=minos-pdf",
                "--onedir",
                "--console",  # The app reads stdout and hides the window itself.
                "--noconfirm",
                "--clean",
                f"--paths={ROOT}",
                f"--distpath={args.dist}",
                f"--workpath={work}",
                f"--specpath={work}",
                *(f"--exclude-module={name}" for name in EXCLUDES),
                *(f"--add-data={ROOT / 'minos_pdf' / name}{os.pathsep}minos_pdf"
                  for name in ("fonts-windows.conf", "fonts-posix.conf")),
            ]
        )

    bundle = args.dist / "minos-pdf"
    # The conversion test alone wouldn't notice: it could be finding this machine's copies.
    for library in REQUIRED_LIBRARIES:
        if not any(library in path.name for path in (bundle / "_internal").iterdir()):
            raise SystemExit(f"The engine bundle is missing {library}. Is Pango where the linker can find it?")
    # OpenCV's video codecs: about 30 MB that pdf2docx's image handling never loads.
    for codec in bundle.rglob("opencv_videoio_ffmpeg*"):
        codec.unlink()
    # python-docx opens its templates through docx/parts/../templates. The code of that folder
    # is in PyInstaller's archive, so it isn't on disk, and Linux and macOS won't resolve a path
    # through a missing folder. The file keeps installers from dropping it for being empty.
    parts = bundle / "_internal" / "docx" / "parts"
    parts.mkdir(parents=True, exist_ok=True)
    (parts / ".keep").touch()
    # PyInstaller replaces the folder, and this placeholder is tracked by git.
    (bundle / ".gitkeep").touch()
    smoke_test(bundle)


if __name__ == "__main__":
    main()
