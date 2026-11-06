"""Command line entry point: runs one conversion and reports the result as JSON.

The last line of stdout is always `{"ok": true}` or
`{"ok": false, "code": "...", "message": "..."}`; libraries may log above it.
"""

import argparse
import json
import sys

from minos_pdf import EngineError


def run(command: str, source: str, output: str) -> None:
    # Imported on demand: each direction has heavy, unrelated native dependencies.
    if command == "pdf-to-docx":
        from minos_pdf.pdf_to_docx import convert
    else:
        from minos_pdf.docx_to_pdf import convert
    convert(source, output)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(prog="minos-pdf")
    parser.add_argument("command", choices=["pdf-to-docx", "docx-to-pdf"])
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    try:
        run(args.command, args.input, args.output)
    except EngineError as error:
        result = {"ok": False, "code": error.code, "message": error.message}
    except Exception as error:  # Anything else still has to reach the app as JSON.
        result = {"ok": False, "code": "conversion_failed", "message": f"{type(error).__name__}: {error}"}
    else:
        result = {"ok": True}

    print(json.dumps(result), flush=True)
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
