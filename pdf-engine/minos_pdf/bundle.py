"""Makes WeasyPrint use what's packaged with the engine, when running from the bundle.

Without this, it would look for Pango and its font configuration where the machine that
built the bundle had them, which only works by accident on that machine.
"""

import glob
import os
import sys
from pathlib import Path


def _in_bundle(root: Path, library):
    """The bundled copy of a library. On macOS it may carry its version, as in
    libgobject-2.0.0.dylib, while WeasyPrint asks for libgobject-2.0.dylib."""
    if not isinstance(library, str):
        return None
    if (root / library).is_file():
        return str(root / library)
    versions = sorted(root.glob(f"{glob.escape(Path(library).stem)}.*{Path(library).suffix}"))
    return str(versions[0]) if versions and Path(library).suffix else None


def use_bundled_libraries() -> None:
    root = getattr(sys, "_MEIPASS", None)
    if root is None:
        return
    root = Path(root)
    name = "fonts-windows.conf" if os.name == "nt" else "fonts-posix.conf"
    os.environ["FONTCONFIG_FILE"] = str(root / "minos_pdf" / name)

    import cffi

    original = cffi.FFI.dlopen
    if getattr(original, "uses_bundle", False):
        return

    def dlopen(self, library, flags=0):
        # PyInstaller makes ctypes look in the bundle, but not cffi, which WeasyPrint uses.
        return original(self, _in_bundle(root, library) or library, flags)

    dlopen.uses_bundle = True
    cffi.FFI.dlopen = dlopen
