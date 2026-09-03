"""Inject the extract/install helpers into update.rs.

Run from repo root. Idempotent: skip if `fn extract_zip` is already present.
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
    b"fn checksum_asset(release: &GitHubRelease) -> Option<&GitHubAsset> {\r\n"
    b'    release.assets.iter().find(|a| a.name == "SHA256SUMS")\r\n'
    b"}\r\n"
)

if needle not in data:
    print("ERROR: could not find checksum_asset to anchor against")
    sys.exit(1)

new_helpers = (
    needle
    + b"\r\n"
    + b"/// Extract every entry of a `.tar.gz` archive into `extract_dir`.\r\n"
    + b"/// Pre-existing content in `extract_dir` is wiped first so a failed\r\n"
    + b"/// retry does not see stale entries. Directories and any path that\r\n"
    + b"/// tries to escape via ../ are skipped.\r\n"
    + b"fn extract_tar_gz(bytes: &[u8], extract_dir: &Path) -> Result<()> {\r\n"
    + b"    if extract_dir.exists() {\r\n"
    + b"        let _ = fs::remove_dir_all(extract_dir);\r\n"
    + b"    }\r\n"
    + b'    fs::create_dir_all(extract_dir).context("Failed to create archive extraction dir")?;\r\n'
    + b"\r\n"
    + b"    let cursor = std::io::Cursor::new(bytes);\r\n"
    + b"    let gz = flate2::read::GzDecoder::new(cursor);\r\n"
    + b"    let mut archive = tar::Archive::new(gz);\r\n"
    + b"    for entry in archive.entries()? {\r\n"
    + b"        let mut entry = entry?;\r\n"
    + b"        let entry_path = entry.path()?.into_owned();\r\n"
    + b"        if entry_path.components().count() != 1 {\r\n"
    + b"            continue;\r\n"
    + b"        }\r\n"
    + b"        let file_name = entry_path\r\n"
    + b"            .file_name()\r\n"
    + b"            .map(|n| n.to_string_lossy().to_string())\r\n"
    + b"            .unwrap_or_default();\r\n"
    + b'        if file_name.is_empty() || file_name.ends_with(".tar.gz") {\r\n'
    + b"            continue;\r\n"
    + b"        }\r\n"
    + b"        let dest = extract_dir.join(&file_name);\r\n"
    + b"        entry.unpack(&dest)?;\r\n"
    + b"    }\r\n"
    + b"    Ok(())\r\n"
    + b"}\r\n"
    + b"\r\n"
    + b"/// Extract every entry of a `.zip` archive into `extract_dir`. Windows\r\n"
    + b"/// release assets are published as .zip (see release.yml and\r\n"
    + b"/// scripts/install.ps1); before this helper existed, the in-app\r\n"
    + b"/// updater had no zip path at all and would fall through to a path that\r\n"
    + b"/// tried to launch the raw archive.\r\n"
    + b"///\r\n"
    + b"/// Only top-level entries (no nested directories) are unpacked. The\r\n"
    + b"/// release pipeline ships the launcher and a `*.bin` payload at the\r\n"
    + b"/// root of the archive, so this matches the layout we expect.\r\n"
    + b"fn extract_zip(bytes: &[u8], extract_dir: &Path) -> Result<()> {\r\n"
    + b"    if extract_dir.exists() {\r\n"
    + b"        let _ = fs::remove_dir_all(extract_dir);\r\n"
    + b"    }\r\n"
    + b'    fs::create_dir_all(extract_dir).context("Failed to create archive extraction dir")?;\r\n'
    + b"\r\n"
    + b"    let cursor = std::io::Cursor::new(bytes);\r\n"
    + b'    let mut archive = zip::ZipArchive::new(cursor).context("Failed to open zip archive")?;\r\n'
    + b"    for i in 0..archive.len() {\r\n"
    + b"        let mut entry = archive.by_index(i)?;\r\n"
    + b"        if entry.is_dir() {\r\n"
    + b"            continue;\r\n"
    + b"        }\r\n"
    + b"        // Some zip writers use backslashes (Windows-native) and some\r\n"
    + b"        // use forward slashes. Normalise, then take only the final\r\n"
    + b"        // path component so a malicious archive cannot write outside\r\n"
    + b"        // extract_dir via ../ components.\r\n"
    + b"        let raw_name = entry.name().replace('\\\\', \"/\");\r\n"
    + b'        let file_name = match raw_name.rsplit(\'/\').next() {\r\n'
    + b'            Some(name) if !name.is_empty() => name.to_string(),\r\n'
    + b"            _ => continue,\r\n"
    + b"        };\r\n"
    + b"        let dest = extract_dir.join(&file_name);\r\n"
    + b"        let mut out = fs::File::create(&dest)\r\n"
    + b'            .with_context(|| format!("Failed to create {}", dest.display()))?;\r\n'
    + b"        std::io::copy(&mut entry, &mut out)\r\n"
    + b'            .with_context(|| format!("Failed to extract {}", file_name))?;\r\n'
    + b"    }\r\n"
    + b"    Ok(())\r\n"
    + b"}\r\n"
    + b"\r\n"
    + b"/// Walk an already-extracted archive directory and copy every file into\r\n"
    + b"/// the per-version install dir. The launcher is renamed to\r\n"
    + b"/// [`build::binary_name()`] so the rest of the build/symlink machinery\r\n"
    + b"/// does not have to care whether the archive named it\r\n"
    + b"/// `alphacode-windows-x86_64` or `alphacode-windows-x86_64.exe` (or\r\n"
    + b"/// `alphacode` on Linux/macOS).\r\n"
    + b"///\r\n"
    + b"/// This is the shared tail of the `.tar.gz` and `.zip` install paths.\r\n"
    + b"fn install_extracted_archive(extract_dir: &Path, release: &GitHubRelease) -> Result<PathBuf> {\r\n"
    + b"    // Mark every file executable up-front. On Windows the .exe bit is\r\n"
    + b"    // meaningless, but the `.bin` payload the Linux/macOS archives ship\r\n"
    + b"    // alongside the launcher is a real executable and loses its bit\r\n"
    + b"    // through some copy paths.\r\n"
    + b"    if let Ok(read_dir) = fs::read_dir(extract_dir) {\r\n"
    + b"        for entry in read_dir.flatten() {\r\n"
    + b"            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {\r\n"
    + b"                let _ = crate::platform::set_permissions_executable(&entry.path());\r\n"
    + b"            }\r\n"
    + b"        }\r\n"
    + b"    }\r\n"
    + b"\r\n"
    + b"    // Identify the launcher. Inside a release archive the launcher is\r\n"
    + b"    // the file whose stem matches the asset stem -- possibly with `.exe`\r\n"
    + b"    // on Windows, possibly with `.bin` for the sidecar payload. Anything\r\n"
    + b"    // else is installed alongside it under its own name.\r\n"
    + b"    let asset_stem = get_asset_stem();\r\n"
    + b"    let launcher_names = [\r\n"
    + b"        asset_stem.to_string(),\r\n"
    + b'        format!("{}.exe", asset_stem),\r\n'
    + b'        format!("{}.bin", asset_stem),\r\n'
    + b"    ];\r\n"
    + b"\r\n"
    + b"    let version = release.tag_name.trim_start_matches('v');\r\n"
    + b'    let dest_dir = build::builds_dir()?.join("versions").join(version);\r\n'
    + b'    fs::create_dir_all(&dest_dir).context("Failed to create version install dir")?;\r\n'
    + b"    let mut installed_files = Vec::new();\r\n"
    + b'    for entry in fs::read_dir(extract_dir).context("Failed to read extracted archive")? {\r\n'
    + b"        let entry = entry?;\r\n"
    + b"        if !entry.file_type()?.is_file() {\r\n"
    + b"            continue;\r\n"
    + b"        }\r\n"
    + b"        let name = entry.file_name();\r\n"
    + b"        let name_string = name.to_string_lossy();\r\n"
    + b"        let dest_name = if launcher_names.iter().any(|n| n == &name_string) {\r\n"
    + b"            build::binary_name().to_string()\r\n"
    + b"        } else {\r\n"
    + b"            name_string.to_string()\r\n"
    + b"        };\r\n"
    + b"        let dest = dest_dir.join(&dest_name);\r\n"
    + b"        if dest.exists() {\r\n"
    + b"            fs::remove_file(&dest)?;\r\n"
    + b"        }\r\n"
    + b"        fs::copy(entry.path(), &dest)\r\n"
    + b'            .with_context(|| format!("Failed to install {}", dest.display()))?;\r\n'
    + b"        installed_files.push(dest);\r\n"
    + b"    }\r\n"
    + b"    // Give every installed file the same mtime. The wrapper script and\r\n"
    + b"    // the `.bin` payload otherwise land with whatever sub-second skew\r\n"
    + b"    // the copy loop produced, and any code comparing binary freshness\r\n"
    + b"    // by mtime then sees two \"different age\" files for one logical\r\n"
    + b"    // install.\r\n"
    + b"    let install_stamp = SystemTime::now();\r\n"
    + b"    for path in &installed_files {\r\n"
    + b"        if let Ok(file) = fs::File::options().write(true).open(path) {\r\n"
    + b"            let _ = file.set_modified(install_stamp);\r\n"
    + b"        }\r\n"
    + b"    }\r\n"
    + b"    let _ = fs::remove_dir_all(extract_dir);\r\n"
    + b"    Ok(dest_dir.join(build::binary_name()))\r\n"
    + b"}\r\n"
)

print("found anchor:", needle in data)
data = data.replace(needle, new_helpers, 1)
PATH.write_bytes(data)
print("inserted helpers (", len(new_helpers), "bytes)")
