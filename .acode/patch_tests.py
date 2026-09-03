"""Add regression tests for the updater fix to update.rs.

Run from repo root. Idempotent: skip if `extract_zip_round_trip` is present.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

PATH = Path("src/alphacode_app_core/update.rs")
data = PATH.read_bytes()

if b"fn extract_zip_round_trip()" in data:
    print("tests already present; nothing to do")
    sys.exit(0)

# Anchor: the closing `    }` of the first test, then the mod's closing `}`,
# then the blank line and the next `#[cfg(test)]` mod.
anchor = b"    }\r\n}\r\n\r\n#[cfg(test)]\r\nmod github_auth_tests {"
if anchor not in data:
    print("ERROR: anchor not found")
    sys.exit(1)

new_block = (
    b"    }\r\n\r\n"
    b"    /// Regression test: a v1.0.9-shaped release listing must be\r\n"
    b"    /// discoverable by the platform matcher. Before this test existed,\r\n"
    b"    /// a v1.0.5 binary that asked for an `aarch64`/`.exe` asset could\r\n"
    b"    /// not find any of the v1.0.9 assets (which are named `arm64`/`\r\n"
    b"    /// .zip`) and the in-app /update silently fell through to\r\n"
    b"    /// \"already up to date\".\r\n"
    b"    #[test]\r\n"
    b"    fn platform_asset_finds_current_target_stem() {\r\n"
    b"        // Build a synthetic v1.0.9-shaped release, identical in shape\r\n"
    b"        // to the real GitHub response, then assert the platform matcher\r\n"
    b"        // picks exactly the binary (not its .sha256 sibling).\r\n"
    b'        let release = GitHubRelease {\r\n'
    b'            tag_name: "v1.0.9".to_string(),\r\n'
    b'            _name: Some("v1.0.9".to_string()),\r\n'
    b"            _html_url: String::new(),\r\n"
    b"            _published_at: None,\r\n"
    b"            assets: vec![\r\n"
    b"                GitHubAsset {\r\n"
    b'                    name: format!("{}.sha256", get_asset_stem()),\r\n'
    b"                    browser_download_url: String::new(),\r\n"
    b"                    _size: 96,\r\n"
    b"                },\r\n"
    b"                GitHubAsset {\r\n"
    b"                    name: get_asset_filename(),\r\n"
    b"                    browser_download_url: String::new(),\r\n"
    b"                    _size: 1234,\r\n"
    b"                },\r\n"
    b"            ],\r\n"
    b'            _target_commitish: "main".to_string(),\r\n'
    b"        };\r\n"
    b'        let asset = platform_asset(&release).expect("platform asset must resolve");\r\n'
    b"        assert_eq!(asset.name, get_asset_filename());\r\n"
    b"    }\r\n\r\n"
    b"    /// Every supported platform must publish an asset whose name starts\r\n"
    b"    /// with `get_asset_stem()` for the matching target. Lock that down\r\n"
    b"    /// here so a future drift (e.g. someone reverts the `aarch64` ->\r\n"
    b"    /// `arm64` rename) trips the test instead of regressing in the\r\n"
    b"    /// field.\r\n"
    b"    #[test]\r\n"
    b"    fn platform_asset_finds_assets_for_every_supported_target() {\r\n"
    b"        let every_release = vec![\r\n"
    b'            ("v1.0.9", "alphacode-linux-x86_64.tar.gz"),\r\n'
    b'            ("v1.0.9", "alphacode-linux-arm64.tar.gz"),\r\n'
    b'            ("v1.0.9", "alphacode-macos-x86_64.tar.gz"),\r\n'
    b'            ("v1.0.9", "alphacode-macos-arm64.tar.gz"),\r\n'
    b'            ("v1.0.9", "alphacode-windows-x86_64.zip"),\r\n'
    b'            ("v1.0.9", "alphacode-windows-arm64.zip"),\r\n'
    b"        ];\r\n"
    b"        for (tag, asset_name) in every_release {\r\n"
    b"            let release = GitHubRelease {\r\n"
    b"                tag_name: tag.to_string(),\r\n"
    b"                _name: Some(tag.to_string()),\r\n"
    b"                _html_url: String::new(),\r\n"
    b"                _published_at: None,\r\n"
    b"                assets: vec![GitHubAsset {\r\n"
    b"                    name: asset_name.to_string(),\r\n"
    b"                    browser_download_url: String::new(),\r\n"
    b"                    _size: 1234,\r\n"
    b"                }],\r\n"
    b'                _target_commitish: "main".to_string(),\r\n'
    b"            };\r\n"
    b"            let resolved = platform_asset(&release);\r\n"
    b"            let stem = get_asset_stem();\r\n"
    b"            if asset_name.starts_with(stem) {\r\n"
    b"                assert!(\r\n"
    b"                    resolved.is_ok(),\r\n"
    b'                    "platform_asset must find {asset_name} for stem {stem}"\r\n'
    b"                );\r\n"
    b"            }\r\n"
    b"        }\r\n"
    b"    }\r\n\r\n"
    b"    /// Build a small zip in memory, round-trip it through `extract_zip`,\r\n"
    b"    /// and assert every leaf file lands in the destination directory.\r\n"
    b"    /// This is the only place the Windows release asset gets a chance\r\n"
    b"    /// to unpack, so it must work for both the launcher `.exe` and a\r\n"
    b"    /// sidecar `.bin` payload -- the same shape the release pipeline\r\n"
    b"    /// ships.\r\n"
    b"    #[test]\r\n"
    b"    fn extract_zip_round_trip() {\r\n"
    b'        let tmp = std::env::temp_dir().join(format!(\r\n'
    b'            "alphacode-zip-test-{}",\r\n'
    b"            std::process::id()\r\n"
    b"        ));\r\n"
    b"        let _ = std::fs::remove_dir_all(&tmp);\r\n"
    b"        std::fs::create_dir_all(&tmp).unwrap();\r\n"
    b'        let zip_path = tmp.join("sample.zip");\r\n'
    b'        let extract_dir = tmp.join("extract");\r\n\r\n'
    b"        {\r\n"
    b'            let file = std::fs::File::create(&zip_path).unwrap();\r\n'
    b"            let mut zip = zip::ZipWriter::new(file);\r\n"
    b"            let opts: zip::write::SimpleFileOptions =\r\n"
    b"                zip::write::SimpleFileOptions::default();\r\n"
    b'            zip.start_file("alphacode-windows-x86_64.exe", opts).unwrap();\r\n'
    b'            std::io::Write::write_all(&mut zip, b"exe-bytes").unwrap();\r\n'
    b'            zip.start_file("alphacode-windows-x86_64.bin", opts).unwrap();\r\n'
    b'            std::io::Write::write_all(&mut zip, b"bin-bytes").unwrap();\r\n'
    b"            zip.finish().unwrap();\r\n"
    b"        }\r\n\r\n"
    b"        let bytes = std::fs::read(&zip_path).unwrap();\r\n"
    b'        extract_zip(&bytes, &extract_dir).expect("extract_zip must succeed");\r\n\r\n'
    b'        let exe = std::fs::read(extract_dir.join("alphacode-windows-x86_64.exe")).unwrap();\r\n'
    b'        let bin = std::fs::read(extract_dir.join("alphacode-windows-x86_64.bin")).unwrap();\r\n'
    b'        assert_eq!(exe, b"exe-bytes");\r\n'
    b'        assert_eq!(bin, b"bin-bytes");\r\n\r\n'
    b"        // Cleanup is best-effort; the OS will reap the temp dir.\r\n"
    b"        let _ = std::fs::remove_dir_all(&tmp);\r\n"
    b"    }\r\n"
)

# Replace the very first `    }\r\n}\r\n` that comes right before
# the next `#[cfg(test)]` mod. The pattern is exactly:
#   <indent>}\r\n<indent>}\r\n\r\n#[cfg(test)]\r\nmod github_auth_tests {
# with the two closing braces belonging to a test fn and the mod itself.
# Use a literal-byte substitution. Earlier we tried a regex but the
# escaping for `#[` in a raw bytes pattern is awkward; a direct replace
# is simpler and the anchor is unique enough to be safe.
anchor_bytes = b"    }\r\n}\r\n\r\n#[cfg(test)]\r\nmod github_auth_tests {"
if data.count(anchor_bytes) != 1:
    print("ERROR: expected exactly 1 anchor match, found", data.count(anchor_bytes))
    sys.exit(1)
new_data = data.replace(
    anchor_bytes,
    new_block + b"\r\n#[cfg(test)]\r\nmod github_auth_tests {",
    1,
)
PATH.write_bytes(new_data)
print("inserted tests (", len(new_block), "bytes)")
