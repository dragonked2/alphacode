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
    python scripts/verify-pe-metadata.py --self-test

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


def certificate_table(data: bytes) -> tuple[int, int]:
    """Return ``(file_offset, size)`` of the PKCS#7 Authenticode certificate table.

    ``(0, 0)`` when the image carries no signature. Unlike the other data
    directories, ``IMAGE_DIRECTORY_ENTRY_SECURITY`` (index 4) stores a *file
    offset* rather than an RVA, because the certificate table is appended after
    the last section instead of being mapped as a section.

    This matters here: a signed binary therefore has bytes past its last
    section by design, so the overlay check below cannot treat "anything after
    the sections" as a packer artifact.
    """
    lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    optional = lfanew + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    if magic == 0x20B:  # PE32+
        directory_offset = optional + 112
        directory_count_offset = optional + 108
    elif magic == 0x10B:  # PE32
        directory_offset = optional + 96
        directory_count_offset = optional + 92
    else:
        return 0, 0
    if directory_count_offset + 4 > len(data):
        return 0, 0
    directory_count = struct.unpack_from("<I", data, directory_count_offset)[0]
    if directory_count <= 4:
        return 0, 0
    entry = directory_offset + 4 * 8
    if entry + 8 > len(data):
        return 0, 0
    offset, size = struct.unpack_from("<II", data, entry)
    return offset, size


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

    # Bytes after the last section are the packer/dropper shape *unless* they
    # are the Authenticode certificate table, which is a legitimate reason for a
    # signed binary to extend past the mapped image. Subtract that directory and
    # only what remains is genuine overlay. Without this the overlay check would
    # reject every correctly signed release and force SignPath's output to be
    # reverted -- the exact opposite of the goal.
    cert_offset, cert_size = certificate_table(data)
    attributed_end = end
    if cert_size and cert_offset >= end and cert_offset + cert_size <= len(data):
        attributed_end = cert_offset + cert_size
    overlay = len(data) - attributed_end
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


def _synthetic_pe(*, version: bool, signed: bool, trailing: int) -> bytes:
    """Build a minimal PE32+ image for the self-test.

    One section spanning file offsets ``0x200..0x400``; VERSIONINFO text (or
    zeros) at ``0x200``; optionally an 8-byte certificate table at ``0x400``
    registered in data directory 4; optionally ``trailing`` bytes of genuine
    junk after everything else.
    """
    optional_size = 240
    lfanew = 0x40
    header = bytearray(0x200)
    header[0:2] = DOS_MAGIC
    struct.pack_into("<I", header, 0x3C, lfanew)
    header[lfanew : lfanew + 4] = PE_SIGNATURE
    coff = lfanew + 4
    struct.pack_into("<H", header, coff + 0, 0x8664)  # Machine
    struct.pack_into("<H", header, coff + 2, 1)  # NumberOfSections
    struct.pack_into("<H", header, coff + 16, optional_size)
    optional = lfanew + 24
    struct.pack_into("<H", header, optional, 0x20B)  # PE32+
    struct.pack_into("<I", header, optional + 108, 16)  # NumberOfRvaAndSizes
    section = optional + optional_size
    struct.pack_into("<I", header, section + 16, 0x200)  # SizeOfRawData
    struct.pack_into("<I", header, section + 20, 0x200)  # PointerToRawData

    data = bytearray(header) + bytearray(0x200)  # 0x400 total
    if version:
        blob = ("VS_VERSION_INFO\0" + "\0".join(REQUIRED_STRINGS)).encode("utf-16-le")
        data[0x200 : 0x200 + len(blob)] = blob
    if signed:
        cert_offset = len(data)  # 0x400, right after the sections
        struct.pack_into("<II", data, optional + 112 + 4 * 8, cert_offset, 8)
        data += b"\x00" * 8
    if trailing:
        data += b"\x00" * trailing
    return bytes(data)


def self_test() -> int:
    """Prove the overlay check treats the certificate table as legitimate.

    A guard that flagged every signed binary would silently make code signing
    impossible; these cases pin the behaviour in both directions.
    """
    import tempfile

    cases = (
        # (label, version, signed, trailing, expect_overlay_problem, expect_version_problem)
        ("signed, nothing else", True, True, 0, False, False),
        ("unsigned, 8-byte overlay", True, False, 8, True, False),
        ("signed, plus 8-byte overlay", True, True, 8, True, False),
        ("no VERSIONINFO, unsigned", False, False, 0, False, True),
    )
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        for label, version, signed, trailing, want_overlay, want_version in cases:
            path = os.path.join(tmp, "case.exe")
            with open(path, "wb") as handle:
                handle.write(
                    _synthetic_pe(version=version, signed=signed, trailing=trailing)
                )
            problems, _ = inspect(path)
            got_overlay = any("overlay" in problem for problem in problems)
            got_version = any("VERSIONINFO" in problem for problem in problems)
            if got_overlay == want_overlay and got_version == want_version:
                print(f"ok: self-test '{label}'")
                continue
            failures += 1
            print(
                f"FAIL: self-test '{label}': overlay={got_overlay} "
                f"(want {want_overlay}), version={got_version} (want {want_version})",
                file=sys.stderr,
            )
    if failures:
        return 1
    print("OK: verify-pe-metadata.py self-test passed")
    return 0


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: verify-pe-metadata.py <binary> [more...]", file=sys.stderr)
        return 2
    if argv[1] == "--self-test":
        return self_test()

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
