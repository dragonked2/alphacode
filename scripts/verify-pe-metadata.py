#!/usr/bin/env python3
"""Fail if a shipped PE is unattributed, leaks build paths, or carries an overlay.

Why this exists
---------------
Two independent, *structural* defects put the 1.1.21 `AlphaCode.Desktop.exe`
(GUI host, sha256 `f61b747a...e272a054`) in front of VirusTotal's heuristics:

1. **PE overlay.** `build.rs::append_version_resource` compiled a VERSIONINFO
   `.res` with `rc.exe` and then *appended* the raw bytes after the last
   section. Nothing references that block, and the extra bytes past the mapped
   image are exactly what a packer leaves behind -- VirusTotal tagged the file
   `overlay`. Verified on the shipped 1.1.21 host: 684 bytes of overlay.

2. **Empty product metadata.** Because the appended block is not registered in
   the resource directory, Windows reported `ProductName`, `CompanyName` and
   `LegalCopyright` as *empty* even though the bytes were present. An unsigned,
   unnamed .NET launcher that embeds and runs a PowerShell script reads as a
   dropped payload.

A third check covers build-machine paths. A release built without
`--remap-path-prefix` carries the builder's profile directory thousands of
times, which is both a privacy leak and a reputation signal.

The fix for (1) and (2) is to let `csc.exe` synthesise VERSIONINFO from
assembly attributes, so the resource is linked into `.rsrc` (referenced, no
overlay) and every product field is populated. This script makes that
structural rather than a convention.

Scope note: this checks *shape, attribution and hygiene*, not behaviour. It
cannot prove a binary is free of false positives, and it does not replace code
signing.

Usage:
    python scripts/verify-pe-metadata.py path/to/binary.exe [more...]

Exit codes:
    0  every file passed
    1  a file has an overlay, missing metadata, or leaked build paths
    2  no input / file missing / not a PE
"""

from __future__ import annotations

import os
import struct
import sys

DOS_MAGIC = b"MZ"
PE_SIGNATURE = b"PE\x00\x00"

# Every field Explorer and reputation scanners read. `OriginalFilename` is
# included because an empty one is itself an "unattributed binary" signal.
REQUIRED_STRINGS = (
    "FileDescription",
    "ProductName",
    "CompanyName",
    "LegalCopyright",
    "FileVersion",
    "ProductVersion",
    "OriginalFilename",
)


def build_path_marker() -> bytes | None:
    r"""The build machine's own profile path as raw bytes, or ``None``.

    Only *this* machine's exact profile directory is meaningful: a bare
    `C:\Users\` can legitimately appear in ordinary text, whereas
    `C:\Users\<user>` can only be present because a `file!()` literal was
    embedded without `--remap-path-prefix`.

    Order: explicit override, then `USERPROFILE` on Windows. Returns ``None``
    when neither is available, in which case the caller reports the check as
    skipped rather than silently passing.
    """
    override = os.environ.get("ALPHACODE_BUILD_PATH_MARKER")
    if override:
        return override.encode("utf-8", "ignore")
    profile = os.environ.get("USERPROFILE")
    if profile and len(profile.replace("\\", "")) > 3:
        return profile.encode("utf-8", "ignore")
    return None


def section_table(data: bytes) -> tuple[int, int]:
    """Return ``(section_count, end_of_last_section)`` for a PE image."""
    if not data.startswith(DOS_MAGIC):
        raise ValueError("not a PE binary (missing MZ header)")
    lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    if data[lfanew : lfanew + 4] != PE_SIGNATURE:
        raise ValueError("not a PE binary (missing PE signature)")
    section_count = struct.unpack_from("<H", data, lfanew + 6)[0]
    size_of_optional = struct.unpack_from("<H", data, lfanew + 20)[0]
    table = lfanew + 24 + size_of_optional
    end = 0
    for index in range(section_count):
        entry = table + index * 40
        raw_size = struct.unpack_from("<I", data, entry + 16)[0]
        raw_pointer = struct.unpack_from("<I", data, entry + 20)[0]
        end = max(end, raw_pointer + raw_size)
    return section_count, end


def version_info_missing(data: bytes, mapped_end: int) -> list[str]:
    """VERSIONINFO fields that Windows cannot read from the mapped image.

    Only bytes up to ``mapped_end`` (the end of the last section) are mapped by
    the loader, so a VERSIONINFO block that lives in an overlay is invisible to
    Explorer even though its strings are present in the file. That is exactly
    what the 1.1.21 host did. Decoding the mapped region only keeps this check
    honest: it fails both a file with no version resource and one whose
    resource sits past the last section.
    """
    mapped = data[:mapped_end]
    if "VS_VERSION_INFO".encode("utf-16-le") not in mapped:
        return list(REQUIRED_STRINGS)
    wide = mapped.decode("utf-16-le", "ignore")
    return [name for name in REQUIRED_STRINGS if name not in wide]


def inspect(path: str) -> tuple[list[str], bool]:
    """Return ``(problems, path_check_ran)`` for one file."""
    with open(path, "rb") as handle:
        data = handle.read()
    _, end = section_table(data)
    problems: list[str] = []

    overlay = len(data) - end
    if overlay:
        problems.append(
            f"{overlay} bytes of PE overlay past the last section "
            f"(file {len(data)}, sections end {end}); a packer/dropper shape"
        )

    missing = version_info_missing(data, end)
    if missing:
        problems.append(
            "VERSIONINFO not readable from the mapped image; missing "
            + ", ".join(missing)
        )

    marker = build_path_marker()
    if marker is None:
        return problems, False
    leaked = data.count(marker)
    if leaked:
        problems.append(
            f"{leaked} occurrences of the build machine's profile path "
            f"({marker.decode(errors='replace')}); the release build must "
            "pass --remap-path-prefix"
        )
    return problems, True


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: verify-pe-metadata.py <binary> [more...]", file=sys.stderr)
        return 2

    failed = False
    skipped = False
    for path in argv[1:]:
        try:
            problems, path_check_ran = inspect(path)
        except OSError as error:
            print(f"error: cannot read {path}: {error}", file=sys.stderr)
            return 2
        except ValueError as error:
            print(f"error: {path}: {error}", file=sys.stderr)
            return 2
        skipped = skipped or not path_check_ran
        if not problems:
            print(f"OK: {path} has no overlay and complete VERSIONINFO")
            continue
        failed = True
        print(f"FAIL: {path}", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "  Overlay/VERSIONINFO: let csc.exe emit VERSIONINFO from assembly "
            "attributes instead of appending a compiled .res (see build.rs) so "
            "the resource is linked into .rsrc and every product field is "
            "populated. Build paths: build with --remap-path-prefix as "
            "release.yml and scripts/release-build.sh do.",
            file=sys.stderr,
        )

    if skipped:
        print(
            "note: build-path check skipped (no ALPHACODE_BUILD_PATH_MARKER or "
            "USERPROFILE in the environment)",
            file=sys.stderr,
        )
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
