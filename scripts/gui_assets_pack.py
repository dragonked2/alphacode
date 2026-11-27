#!/usr/bin/env python3
"""Compress built GUI frontend assets (`src/cli/gui/ui/dist`) deterministically
with gzip and generate `OUT_DIR/gui_embedded_assets.rs` for `cli::gui::web`.

Why this exists
---------------
Embedding `ui/dist` uncompressed adds ~780 KiB of plaintext JS/CSS to the binary.
Compressing each asset at build time with deterministic gzip (`mtime=0`) shrinks
the embedded footprint to ~200 KiB (~74% smaller) and lets the loopback HTTP
server serve `Content-Encoding: gzip` directly without per-request compression.

When `--require` is passed (release builds and CI release-guard builds), a
missing or incomplete `dist/` directory fails the build instead of silently
shipping a binary with an empty GUI.

Usage:
    python scripts/gui_assets_pack.py <dist_dir> <out_dir> [--require]
"""

from __future__ import annotations

import gzip
import hashlib
import sys
from pathlib import Path

BUNDLE_MARKER = "ALPHACODE_EMBEDDED_GUI_UI_V1"


def escape_rust_string(value: str) -> str:
    out: list[str] = []
    for ch in value:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        else:
            out.append(ch)
    return "".join(out)


def collect_dist_files(dist_dir: Path) -> list[tuple[str, Path]]:
    if not (dist_dir / "index.html").is_file():
        return []
    entries: list[tuple[str, Path]] = []
    for path in dist_dir.rglob("*"):
        if path.is_file():
            rel = path.relative_to(dist_dir).as_posix()
            entries.append((rel, path))
    entries.sort(key=lambda item: item[0])
    return entries


def main(argv: list[str]) -> int:
    require = "--require" in argv
    positional = [arg for arg in argv if arg != "--require"]
    if len(positional) != 2:
        print(
            "usage: gui_assets_pack.py <dist_dir> <out_dir> [--require]",
            file=sys.stderr,
        )
        return 2

    dist_dir = Path(positional[0])
    out_dir = Path(positional[1])
    assets_out = out_dir / "gui_assets"
    generated_rs = out_dir / "gui_embedded_assets.rs"

    entries = collect_dist_files(dist_dir)
    has_js = any(rel.endswith(".js") or rel.endswith(".mjs") for rel, _ in entries)

    if require and (not entries or not has_js):
        print(
            "error: release build requires compiled GUI frontend assets in "
            f"'{dist_dir}' (index.html + at least one JS bundle).\n"
            "Run `npm ci && npm run build` inside `src/cli/gui/ui` before building.",
            file=sys.stderr,
        )
        return 1

    assets_out.mkdir(parents=True, exist_ok=True)
    expected_gz: set[str] = set()
    table_lines: list[str] = []

    for rel, src_path in entries:
        raw = src_path.read_bytes()
        # mtime=0 guarantees byte-for-byte reproducible builds across machines.
        gz = gzip.compress(raw, compresslevel=9, mtime=0)
        digest = hashlib.sha256(rel.encode("utf-8")).hexdigest()[:16]
        gz_name = f"{digest}.gz"
        expected_gz.add(gz_name)
        gz_path = assets_out / gz_name
        if not gz_path.is_file() or gz_path.read_bytes() != gz:
            gz_path.write_bytes(gz)

        rel_escaped = escape_rust_string(rel)
        abs_escaped = escape_rust_string(str(gz_path))
        table_lines.append(
            f'    ("{rel_escaped}", include_bytes!("{abs_escaped}")),\n'
        )

    # Remove stale .gz files from previous Vite hashed builds.
    for existing in assets_out.glob("*.gz"):
        if existing.name not in expected_gz:
            existing.unlink(missing_ok=True)

    marker = BUNDLE_MARKER if (entries and has_js) else ""
    code = (
        f'pub(super) const EMBEDDED_UI_BUNDLE_MARKER: &str = "{marker}";\n'
        "pub(super) const EMBEDDED_UI_ASSETS_GZIP: &[(&str, &[u8])] = &[\n"
        + "".join(table_lines)
        + "];\n"
    )

    if not generated_rs.is_file() or generated_rs.read_text(encoding="utf-8") != code:
        generated_rs.write_text(code, encoding="utf-8", newline="\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
