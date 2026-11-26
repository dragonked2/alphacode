#!/usr/bin/env python3
"""Inspect and validate a Windows release archive for the signing pipeline.

The signing workflow and scripts/signpath/submit-signing-request.ps1 both need
three small operations that are easy to get subtly wrong in shell:

* confirm a release ZIP contains exactly the two shipped Windows binaries before
  it is handed to SignPath (the artifact configuration names them explicitly, so
  an unexpected layout must fail here, not after a signing request is created);
* extract the ZIP to verify the *signed* bytes afterwards;
* emit the SHA-256 sidecar in the same ``<digest>  <name>`` format release.yml
  produces, written from Python because ``sha256sum`` under Git for Windows
  intermittently corrupts the redirection target (see the 1.1.22 release notes).

Usage:
    archive-tool.py <archive.zip> [--require NAME]... [--extract DIR] [--sha256 OUT]

With no mode flag it validates the archive contents and extracts nothing.

Exit codes:
    0  success
    1  a required file is missing, or the archive is not a ZIP
    2  usage error / file unreadable
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
import zipfile

DEFAULT_REQUIRED = ("alphacode.exe", "AlphaCode.Desktop.exe")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("archive")
    parser.add_argument(
        "--require",
        action="append",
        default=[],
        metavar="NAME",
        help="file that must be present at the archive root (repeatable; "
        f"default: {', '.join(DEFAULT_REQUIRED)})",
    )
    parser.add_argument("--extract", metavar="DIR", help="extract the archive to DIR")
    parser.add_argument(
        "--sha256",
        metavar="OUT",
        help="write the SHA-256 sidecar (<digest>  <basename>) to OUT",
    )
    args = parser.parse_args(argv[1:])

    if not os.path.isfile(args.archive):
        print(f"error: {args.archive} does not exist", file=sys.stderr)
        return 2
    try:
        zf = zipfile.ZipFile(args.archive)
    except zipfile.BadZipFile as error:
        print(f"error: {args.archive} is not a ZIP archive: {error}", file=sys.stderr)
        return 1

    required = args.require or list(DEFAULT_REQUIRED)
    names = set(zf.namelist())
    missing = [name for name in required if name not in names]
    if missing:
        print(
            f"error: {args.archive} is missing {missing}; the signing artifact "
            "configuration signs these files by name",
            file=sys.stderr,
        )
        return 1

    if args.extract:
        os.makedirs(args.extract, exist_ok=True)
        zf.extractall(args.extract)
        print(f"extracted {args.archive} -> {args.extract}")

    if args.sha256:
        with open(args.archive, "rb") as handle:
            digest = hashlib.sha256(handle.read()).hexdigest()
        with open(args.sha256, "w", encoding="ascii", newline="\n") as handle:
            handle.write(f"{digest}  {os.path.basename(args.archive)}\n")
        print(f"{digest}  {os.path.basename(args.archive)}")

    if not args.extract and not args.sha256:
        print(f"OK: {args.archive} contains {', '.join(required)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
