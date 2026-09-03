"""Refactor update.rs to extract .zip support and share the install path.

Run from repo root. Idempotent: skip if the new `extract_zip` helper is
already present.
"""
from __future__ import annotations

import sys
from pathlib import Path

PATH = Path("src/alphacode_app_core/update.rs")
data = PATH.read_bytes()

if b"fn extract_zip(" in data:
    print("already refactored; nothing to do")
    sys.exit(0)

needle = (
    b"    let mut installed_version_dir: Option<PathBuf> = None;\r\n"
    b"    if asset.name.ends_with(\".tar.gz\") {\r\n"
)

if needle not in data:
    print("ERROR: could not find the if/else install block")
    sys.exit(1)

start = data.find(needle)
# Find the matching `} else {` that switches to the write-to-file path,
# then the closing `}` of the if/else, then the next blank line.
# Easiest: scan forward from start for the literal "fs::write(&temp_path".
write_marker = b"        fs::write(&temp_path, &bytes).context(\"Failed to write temp file\")?;"
write_at = data.find(write_marker, start)
if write_at < 0:
    print("ERROR: could not find fs::write fallback")
    sys.exit(1)

# End of the else block: the line after `}` that closes the if/else.
# We want everything up to (but not including) the next blank line and the
# subsequent `let version = release.tag_name...`.
end_marker = b"\r\n\r\n    let version = release.tag_name.trim_start_matches('v');"
end_at = data.find(end_marker, write_at)
if end_at < 0:
    print("ERROR: could not find end of if/else")
    sys.exit(1)

old_block = data[start:end_at]
print("found old block of", len(old_block), "bytes")

replacement = (
    b"    let mut installed_version_dir: Option<PathBuf> = None;\r\n"
    b"    let extract_dir = temp_path.with_extension(\"extract\");\r\n"
    b"    if asset.name.ends_with(\".tar.gz\") {\r\n"
    b"        extract_tar_gz(&bytes, &extract_dir)\r\n"
    b"            .context(\"Failed to extract tar.gz update archive\")?;\r\n"
    b"        installed_version_dir = Some(install_extracted_archive(&extract_dir, release)?);\r\n"
    b"    } else if asset.name.ends_with(\".zip\") {\r\n"
    b"        // Windows release assets are published as .zip (see release.yml and\r\n"
    b"        // scripts/install.ps1). The old updater only knew how to unpack\r\n"
    b"        // tarballs, so on Windows it would fall through to the\r\n"
    b"        // write-bytes-to-file path below and then try to launch the raw\r\n"
    b"        // .zip as if it were the binary. Extract it like the tarball\r\n"
    b"        // path so the install logic after the if/else branch works for\r\n"
    b"        // both archive formats.\r\n"
    b"        extract_zip(&bytes, &extract_dir)\r\n"
    b"            .context(\"Failed to extract zip update archive\")?;\r\n"
    b"        installed_version_dir = Some(install_extracted_archive(&extract_dir, release)?);\r\n"
    b"    } else {\r\n"
    b"        fs::write(&temp_path, &bytes).context(\"Failed to write temp file\")?;\r\n"
    b"    }"
)

data = data[:start] + replacement + data[end_at:]
PATH.write_bytes(data)
print("rewrote archive branch (", len(replacement), "bytes)")
