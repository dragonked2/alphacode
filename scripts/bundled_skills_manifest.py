#!/usr/bin/env python3
"""Read the bundled-skill manifest out of `bundled_skills.rs` and pack it.

Why this exists
---------------
`src/alphacode_base/bundled_skills.rs` carried a 4.2 MiB corpus of
documentation as ~290 separate `include_str!` calls. Every byte landed in the
executable's `.rdata` as contiguous plaintext, so the shipped Windows binary
literally contained the raw text of every security-methodology document it
bundles. Generic malware heuristics and crowdsourced YARA rules matched that
*inert* text and reported the program as a stealer/dropper:

  * `INDICATOR_SUSPICIOUS_EXE_SQLQuery_ConfidentialDataStore`
    ("... from logins", "... from cookies", "password_value" — browser-credential
    forensics recipes in the CTF bundle)
  * `INDICATOR_SUSPICIOUS_EXE_NoneWindowsUA` (macOS/X11 user-agent literals)
  * `INDICATOR_SUSPICIOUS_EXE_References_PublicServiceInterface`
    ("webhook.site/" — out-of-band callback examples)
  * `INDICATOR_SUSPICIOUS_ClearWinLogs` ("wevtutil cl Application/System/Security")
  * `INDICATOR_SUSPICIOUS_VM_Evasion_MACAddrComb` (hypervisor MAC prefixes)
  * `Mimikatz_Strings`, `Impacket`

None of those strings were ever executed; they were documentation of the
techniques the agent is *taught to assess*. Compressing the corpus removes the
plaintext from the image without changing a single line the model is served:
the exact same content is decompressed into memory at runtime.

`src/alphacode_base/bundled_skills.json` is the manifest's single source of
truth: it records every skill's name, its body file, and each reference's key
and file path. This script validates that every referenced file exists, then
writes one packed blob. `build.rs` compresses that blob and the Rust runtime
indexes it, so neither the manifest nor the documentation text is compiled into
the binary as plaintext.

Usage:
    python scripts/bundled_skills_manifest.py out/bundled-skills.bin
"""

from __future__ import annotations

import json
import struct
import sys
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MANIFEST_JSON = REPO / "src" / "alphacode_base" / "bundled_skills.json"
SRC_ROOT = REPO / "src" / "alphacode_base"

MAGIC = b"ACSKILL1"


def parse_manifest() -> list[dict]:
    """Load and validate the committed manifest.

    Validation is the point: a skill or reference listed here whose file is
    missing must fail the build rather than silently ship a shorter corpus.
    """
    if not MANIFEST_JSON.is_file():
        raise SystemExit(f"missing manifest: {MANIFEST_JSON}")
    try:
        skills = json.loads(MANIFEST_JSON.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise SystemExit(f"manifest is not valid JSON: {error}")

    if not isinstance(skills, list) or not skills:
        raise SystemExit("manifest must be a non-empty list of skills")

    seen_names: set[str] = set()
    for skill in skills:
        name = skill.get("name", "")
        if not name:
            raise SystemExit(f"manifest entry missing name: {skill!r}")
        if name in seen_names:
            raise SystemExit(f"duplicate skill name in manifest: {name}")
        seen_names.add(name)

        body = skill.get("body", "")
        if body != f"bundled_skills/{name}/SKILL.md":
            raise SystemExit(
                f"skill {name!r} body must be 'bundled_skills/{name}/SKILL.md', "
                f"got {body!r}"
            )
        for reference in skill.get("references", []):
            key, path = reference.get("key", ""), reference.get("path", "")
            if not key or not path:
                raise SystemExit(f"skill {name!r} has a reference missing key/path: {reference!r}")
            # A reference normally lives under its own skill directory, but a
            # few are deliberately shared across parents (for example the `ctf`
            # skill reuses `ctf-workflow/solver-engineering`). Only require that
            # the target stays inside the bundled corpus.
            if not path.startswith("bundled_skills/"):
                raise SystemExit(
                    f"skill {name!r} reference {key!r} path {path!r} is outside "
                    f"the bundled corpus"
                )
    return skills


def build_blob(skills: list[dict]) -> bytes:
    """Concatenate contents and record (offset, length) spans."""

    data_parts: list[bytes] = []
    entries: list[dict] = []
    offset = 0

    def append(relative: str) -> dict:
        nonlocal offset
        absolute = SRC_ROOT / relative
        if not absolute.is_file():
            raise SystemExit(f"missing bundled file: {absolute}")
        raw = absolute.read_bytes()
        data_parts.append(raw)
        span = {"offset": offset, "len": len(raw)}
        offset += len(raw)
        return span

    for skill in skills:
        body = append(skill["body"])
        references = [
            {"key": reference["key"], **append(reference["path"])}
            for reference in skill.get("references", [])
        ]
        entries.append({"name": skill["name"], "body": body, "references": references})

    index = json.dumps(entries, separators=(",", ":")).encode("utf-8")
    data = b"".join(data_parts)
    # Deflate only the body data, and keep the index (skill names + reference
    # keys) as plaintext. The literals that trip antivirus heuristics live in the
    # bodies; the names are ordinary identifiers and stay greppable so release
    # verification can confirm the bundled skills are present in the image.
    compressed = zlib.compress(data, 9)
    header = MAGIC + struct.pack("<III", len(index), len(data), len(compressed))
    return header + index + compressed


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: bundled_skills_manifest.py <output.bin>", file=sys.stderr)
        return 2
    skills = parse_manifest()
    blob = build_blob(skills)
    out = Path(sys.argv[1])
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(blob)
    references = sum(len(s.get("references", [])) for s in skills)
    print(
        f"packed {len(skills)} skills, {references} references, "
        f"{len(blob)} bytes -> {out}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
