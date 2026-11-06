import sys

import cffi
import pytest

from minos_pdf.bundle import use_bundled_libraries


@pytest.fixture
def frozen(tmp_path, monkeypatch):
    """Pretends to run from a PyInstaller bundle, restoring cffi afterwards."""
    monkeypatch.setattr(sys, "_MEIPASS", str(tmp_path), raising=False)
    monkeypatch.delenv("FONTCONFIG_FILE", raising=False)
    calls = []
    monkeypatch.setattr(cffi.FFI, "dlopen", lambda self, library, flags=0: calls.append(library))
    return tmp_path, calls


def test_does_nothing_outside_a_bundle(monkeypatch):
    monkeypatch.delattr(sys, "_MEIPASS", raising=False)
    monkeypatch.delenv("FONTCONFIG_FILE", raising=False)
    original = cffi.FFI.dlopen
    use_bundled_libraries()
    assert cffi.FFI.dlopen is original
    assert "FONTCONFIG_FILE" not in __import__("os").environ


def test_uses_the_font_configuration_in_the_bundle(frozen):
    root, _ = frozen
    use_bundled_libraries()
    import os

    path = os.environ["FONTCONFIG_FILE"]
    assert path.startswith(str(root))
    assert path.endswith(".conf")


def test_loads_libraries_from_the_bundle_first(frozen):
    root, calls = frozen
    (root / "libgobject-2.0.dylib").write_bytes(b"")
    use_bundled_libraries()
    cffi.FFI().dlopen("libgobject-2.0.dylib")
    cffi.FFI().dlopen("not-in-the-bundle")
    assert calls == [str(root / "libgobject-2.0.dylib"), "not-in-the-bundle"]


def test_finds_a_bundled_library_with_its_version_in_the_name(frozen):
    root, calls = frozen
    (root / "libpango-1.0.0.dylib").write_bytes(b"")
    use_bundled_libraries()
    cffi.FFI().dlopen("libpango-1.0.dylib")
    assert calls == [str(root / "libpango-1.0.0.dylib")]


def test_configurations_exist_for_every_system():
    from pathlib import Path

    import minos_pdf

    folder = Path(minos_pdf.__file__).parent
    assert (folder / "fonts-windows.conf").is_file()
    assert (folder / "fonts-posix.conf").is_file()
