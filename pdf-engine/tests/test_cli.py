import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(*args):
    done = subprocess.run(
        [sys.executable, "-m", "minos_pdf", *map(str, args)], cwd=ROOT, capture_output=True, encoding="utf-8"
    )
    return done.returncode, json.loads(done.stdout.strip().splitlines()[-1])


def test_success_reports_ok_as_last_line(rich_pdf, tmp_path):
    # A path with spaces and non-ASCII letters, as the app will pass.
    folder = tmp_path / "carpeta con ñ"
    folder.mkdir()
    output = folder / "salida ñ.docx"
    code, result = run("pdf-to-docx", "--input", rich_pdf, "--output", output)
    assert (code, result) == (0, {"ok": True})
    assert output.stat().st_size > 0


def test_failure_reports_code_and_message(tmp_path):
    path = tmp_path / "bad.pdf"
    path.write_bytes(b"nope")
    code, result = run("pdf-to-docx", "--input", path, "--output", tmp_path / "out.docx")
    assert code == 1
    assert result["ok"] is False
    assert result["code"] == "invalid_pdf"
    assert result["message"]


def test_missing_input_still_reports_json(tmp_path):
    code, result = run("docx-to-pdf", "--input", tmp_path / "missing.docx", "--output", tmp_path / "o.pdf")
    assert code == 1
    assert result["code"] == "unsupported_format"
