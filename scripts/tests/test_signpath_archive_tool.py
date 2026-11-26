"""Tests for scripts/signpath/archive-tool.py.

The signing pipeline depends on this helper for three things: refusing an
archive whose layout the SignPath artifact configuration does not describe,
extracting the signed bytes for verification, and writing the SHA-256 sidecar
in the exact format install.ps1 parses. The last one is the one that matters
most: install.ps1 hard-fails on a checksum mismatch, so a sidecar that does not
round-trip through the release's SHA256SUMS merge-and-lookup would make a signed
release uninstallable.
"""

import hashlib
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

_HERE = Path(__file__).resolve().parent
_TOOL = _HERE.parent / "signpath" / "archive-tool.py"


def _load_tool():
    spec = importlib.util.spec_from_file_location("signpath_archive_tool", _TOOL)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


tool = _load_tool()

BINARIES = ("alphacode.exe", "AlphaCode.Desktop.exe")


def _make_zip(path, names):
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        for name in names:
            zf.writestr(name, b"MZ" + name.encode())


class ArchiveToolTests(unittest.TestCase):
    def test_accepts_the_shipped_two_binary_layout(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "alphacode-windows-x86_64.zip"
            _make_zip(archive, BINARIES)
            self.assertEqual(tool.main(["archive-tool.py", str(archive)]), 0)

    def test_rejects_an_unexpected_layout(self):
        # The configuration names both files, so a missing one must fail before a
        # signing request is created rather than signing the wrong artifact.
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "a.zip"
            _make_zip(archive, ("alphacode.exe",))
            self.assertEqual(tool.main(["archive-tool.py", str(archive)]), 1)

    def test_rejects_a_non_zip(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "a.zip"
            archive.write_bytes(b"not a zip")
            self.assertEqual(tool.main(["archive-tool.py", str(archive)]), 1)

    def test_extract_writes_the_binaries(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "a.zip"
            _make_zip(archive, BINARIES)
            out = Path(temporary) / "out"
            self.assertEqual(
                tool.main(["archive-tool.py", str(archive), "--extract", str(out)]), 0
            )
            for name in BINARIES:
                self.assertTrue((out / name).is_file())

    def test_sidecar_round_trips_through_sha256sums(self):
        """Mirror the pipeline: sign (mutate bytes) -> recompute sidecar ->
        merge into SHA256SUMS -> install.ps1 lookup and compare."""
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "alphacode-windows-x86_64.zip"
            _make_zip(archive, BINARIES)

            # Signing appends the certificate table past the last section, so the
            # archive hash necessarily changes. Any byte change models that.
            with open(archive, "ab") as handle:
                handle.write(b"\x00" * 512)

            sidecar = Path(temporary) / "alphacode-windows-x86_64.sha256"
            self.assertEqual(
                tool.main(["archive-tool.py", str(archive), "--sha256", str(sidecar)]), 0
            )
            line = sidecar.read_text().strip()
            digest, _, name = line.partition("  ")
            self.assertEqual(name, archive.name)
            self.assertEqual(
                digest, hashlib.sha256(archive.read_bytes()).hexdigest()
            )

            # The publish job concatenates every *.sha256 into SHA256SUMS.
            sums = Path(temporary) / "SHA256SUMS"
            sums.write_text(line + "\n", encoding="ascii", newline="\n")

            # install.ps1's parse: "<digest>  <name>", name optionally "*"-prefixed.
            expected = None
            for candidate in sums.read_text().splitlines():
                parts = [part for part in candidate.split() if part]
                if len(parts) < 2:
                    continue
                parsed_name = parts[1].lstrip("*").lstrip("./")
                if parsed_name == archive.name:
                    expected = parts[0].lower()
                    break
            self.assertIsNotNone(expected, "install.ps1 lookup found no entry")
            self.assertEqual(
                expected, hashlib.sha256(archive.read_bytes()).hexdigest()
            )


if __name__ == "__main__":
    unittest.main()
