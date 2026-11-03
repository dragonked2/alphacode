#!/usr/bin/env python3
"""Fail if a released binary contains more than one PE image.

Why this exists
---------------
The 1.0.92 Windows release shipped as a single unsigned `alphacode.exe` that
also carried the .NET/WPF GUI host as a *nested* PE: `build.rs` compiled
`AlphaCode.Desktop.exe` and `src/cli/gui.rs` pulled it in with
`include_bytes!`, then wrote it to disk and launched it on first run.

Two complete PE images in one file, where the inner one is dropped and executed
at runtime, is the classic packer/dropper shape that generic AV heuristics
score. BitDefender reported it as `Gen:Variant.Barys`, which the same engine
resells to ALYac, Arcibit, eScan, Emsisoft, GData and VIPRE — so one heuristic
verdict appeared as seven vendor detections.

The host is now a sibling file. This check makes that structural rather than a
convention: a future change that re-embeds a binary fails the release instead of
quietly reintroducing the detection.

Usage:
    python scripts/verify-pe-single-image.py path/to/alphacode.exe

Exit codes:
    0  at most one PE image
    1  more than one PE image found
    2  file missing / not a PE
"""

from __future__ import annotations

import struct
import sys

DOS_MAGIC = b"MZ"
PE_SIGNATURE = b"PE\x00\x00"
MAX_EMBED_OFFSET = 0x1000  # e_lfanew is a small offset from the DOS header


def pe_image_offsets(data: bytes) -> list[int]:
    """Offsets of plausible PE image starts.

    A candidate needs the DOS stub, a sane ``e_lfanew``, and a matching PE
    signature — the same test a loader uses, so incidental ``MZ`` bytes inside
    strings or resources do not count.
    """
    offsets: list[int] = []
    index = 0
    while True:
        start = data.find(DOS_MAGIC, index)
        if start < 0:
            return offsets
        header = start + 0x3C
        if header + 4 <= len(data):
            lfanew = struct.unpack_from("<I", data, header)[0]
            signature = start + lfanew
            if (
                0 < lfanew < MAX_EMBED_OFFSET
                and signature + 4 <= len(data)
                and data[signature : signature + 4] == PE_SIGNATURE
            ):
                offsets.append(start)
        index = start + 2


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(f"usage: {argv[0]} <pe-file>", file=sys.stderr)
        return 2
    path = argv[1]
    try:
        with open(path, "rb") as handle:
            data = handle.read()
    except OSError as error:
        print(f"error: cannot read {path}: {error}", file=sys.stderr)
        return 2
    if not data.startswith(DOS_MAGIC):
        print(f"error: {path} is not a PE binary", file=sys.stderr)
        return 2

    offsets = pe_image_offsets(data)
    if len(offsets) <= 1:
        print(f"OK: {path} contains {len(offsets)} PE image ({len(data)} bytes)")
        return 0

    print(
        f"error: {path} embeds {len(offsets)} PE images at offsets {offsets}. "
        "A binary that carries another executable and drops it at runtime is "
        "scored as a packer/dropper (BitDefender Gen:Variant.Barys). Ship the "
        "secondary binary as a sibling file instead of embedding it.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))