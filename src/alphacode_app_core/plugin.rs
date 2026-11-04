//! Plugin management for alphacode.
//!
//! Plugins are directories under `~/.alphacode/plugins/` that follow a
//! standard manifest format (`plugin.json`) and can provide skills,
//! tools, or other extensions. The plugin system reuses the existing
//! skill loading infrastructure so plugin skills are automatically
//! discovered and available via `/skillname` slash commands.
//!
//! ## Directory Layout
//!
//! ```text
//! ~/.alphacode/plugins/
//!   my-plugin/
//!     plugin.json          # manifest
//!     skills/
//!       my-skill/
//!         SKILL.md         # skill definition (same format as bundled skills)
//!       another-skill/
//!         SKILL.md
//!     tools/               # (future) custom tool definitions
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Plugin manifest format. Stored as `plugin.json` inside each plugin
/// directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique local plugin identifier (e.g. "my-company-plugin").
    pub name: String,

    /// Human-readable version string (semver recommended).
    pub version: String,

    /// Short description shown in plugin listings.
    #[serde(default)]
    pub description: String,

    /// Plugin author or organization.
    #[serde(default)]
    pub author: String,

    /// SPDX license identifier or path to LICENSE file.
    #[serde(default)]
    pub license: String,

    /// Homepage or repository URL.
    #[serde(default)]
    pub homepage: String,

    /// Minimum alphacode version required (semver range).
    #[serde(default)]
    pub min_version: String,

    /// Tags for discoverability (e.g. ["security", "devops"]).
    #[serde(default)]
    pub tags: Vec<String>,

    /// Skills provided by this plugin. Each entry is a relative path
    /// (from the plugin root) to a directory containing a SKILL.md.
    /// If omitted, all directories under `skills/` are auto-discovered.
    #[serde(default)]
    pub skills: Vec<String>,

    /// Tools provided by this plugin (future use).
    #[serde(default)]
    pub tools: Vec<PluginTool>,
}

/// A tool provided by a plugin (future extensibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTool {
    pub name: String,
    pub description: String,
    pub command: String,
}

/// Information about an installed plugin.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub path: PathBuf,
    pub skill_count: usize,
}

/// Result of a plugin install operation.
#[derive(Debug)]
pub struct InstallResult {
    pub plugin: PluginInfo,
    pub skills_added: Vec<String>,
}

/// Get the plugins directory (`~/.alphacode/plugins/`).
pub fn plugins_dir() -> Result<PathBuf> {
    let alphacode_dir = crate::storage::alphacode_dir()?;
    let plugins = alphacode_dir.join("plugins");
    fs::create_dir_all(&plugins)
        .with_context(|| format!("Failed to create plugins directory: {}", plugins.display()))?;
    Ok(plugins)
}

/// List all installed plugins.
pub fn list_plugins() -> Result<Vec<PluginInfo>> {
    let plugins_dir = plugins_dir()?;
    let mut plugins = Vec::new();

    if !plugins_dir.exists() {
        return Ok(plugins);
    }

    let entries = fs::read_dir(&plugins_dir).with_context(|| {
        format!(
            "Failed to read plugins directory: {}",
            plugins_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.json");
        if !manifest_path.exists() {
            continue;
        }

        match load_manifest(&manifest_path) {
            Ok(manifest) => {
                let skill_count = count_skills_in_plugin(&path, &manifest);
                plugins.push(PluginInfo {
                    manifest,
                    path,
                    skill_count,
                });
            }
            Err(e) => {
                // Skip plugins with invalid manifests but log a warning
                eprintln!(
                    "Warning: skipping plugin at {} (invalid manifest: {})",
                    path.display(),
                    e
                );
            }
        }
    }

    plugins.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    Ok(plugins)
}

/// Get info about a specific plugin by name.
pub fn get_plugin(name: &str) -> Result<PluginInfo> {
    let plugins_dir = plugins_dir()?;
    let plugin_dir = plugins_dir.join(name);

    if !plugin_dir.exists() {
        anyhow::bail!("Plugin '{}' is not installed", name);
    }

    let manifest_path = plugin_dir.join("plugin.json");
    if !manifest_path.exists() {
        anyhow::bail!("Plugin '{}' has no manifest (plugin.json)", name);
    }

    let manifest = load_manifest(&manifest_path)?;
    let skill_count = count_skills_in_plugin(&plugin_dir, &manifest);

    Ok(PluginInfo {
        manifest,
        path: plugin_dir,
        skill_count,
    })
}

/// Install a plugin from a local directory.
/// Unlike upgrades, this entry point never replaces an installed package.
pub fn install_new_plugin(source_dir: &Path) -> Result<InstallResult> {
    install_new_plugin_into(source_dir, &plugins_dir()?)
}

fn install_new_plugin_into(source_dir: &Path, root: &Path) -> Result<InstallResult> {
    let manifest = load_manifest(&source_dir.join("plugin.json"))?;
    fs::create_dir_all(root)?;
    let target_dir = root.join(&manifest.name);
    if target_dir.try_exists()? {
        anyhow::bail!("Plugin '{}' is already installed", manifest.name);
    }
    let staging = tempfile::tempdir_in(root)?;
    copy_dir_recursive(source_dir, staging.path())?;
    // Claim the destination exclusively; a racing import cannot trigger upgrade.
    fs::create_dir(&target_dir)?;
    if let Err(error) = copy_dir_recursive(staging.path(), &target_dir) {
        let _ = fs::remove_dir_all(&target_dir);
        return Err(error);
    }
    let skills_added = discover_skill_names(&target_dir, &manifest);
    Ok(InstallResult {
        plugin: PluginInfo {
            manifest,
            path: target_dir,
            skill_count: skills_added.len(),
        },
        skills_added,
    })
}

/// Install or upgrade a plugin from a local directory.
pub fn install_plugin(source_dir: &Path) -> Result<InstallResult> {
    let manifest_path = source_dir.join("plugin.json");
    if !manifest_path.exists() {
        anyhow::bail!("No plugin.json found in {}", source_dir.display());
    }

    let manifest = load_manifest(&manifest_path)?;
    let plugins_dir = plugins_dir()?;
    let target_dir = plugins_dir.join(&manifest.name);

    // Check if already installed
    if target_dir.exists() {
        let existing = load_manifest(&target_dir.join("plugin.json"))?;
        if existing.version == manifest.version {
            anyhow::bail!(
                "Plugin '{}' v{} is already installed",
                manifest.name,
                manifest.version
            );
        }
        // Upgrade: remove old version first
        fs::remove_dir_all(&target_dir)
            .with_context(|| format!("Failed to remove old plugin: {}", target_dir.display()))?;
    }

    // Copy plugin directory
    copy_dir_recursive(source_dir, &target_dir)
        .with_context(|| format!("Failed to copy plugin to {}", target_dir.display()))?;

    // Discover skills
    let skills_added = discover_skill_names(&target_dir, &manifest);

    let plugin = PluginInfo {
        manifest,
        path: target_dir,
        skill_count: skills_added.len(),
    };

    Ok(InstallResult {
        plugin,
        skills_added,
    })
}

/// Install a plugin from a Git repository URL.
///
/// Clones the repo into a temporary directory, validates the manifest,
/// then copies it into the plugins directory.
pub async fn install_plugin_from_git(url: &str) -> Result<InstallResult> {
    let temp_dir =
        tempfile::tempdir().context("Failed to create temporary directory for plugin clone")?;

    // Clone the repository
    let status = tokio::process::Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            url,
            &temp_dir.path().to_string_lossy(),
        ])
        .status()
        .await
        .context("Failed to run git clone")?;

    if !status.success() {
        anyhow::bail!("Failed to clone plugin repository: {}", url);
    }

    install_plugin(temp_dir.path())
}

/// Remove an installed plugin.
pub fn remove_plugin(name: &str) -> Result<PluginInfo> {
    let plugins_dir = plugins_dir()?;
    let plugin_dir = plugins_dir.join(name);

    if !plugin_dir.exists() {
        anyhow::bail!("Plugin '{}' is not installed", name);
    }

    let manifest = load_manifest(&plugin_dir.join("plugin.json"))?;
    let skill_count = count_skills_in_plugin(&plugin_dir, &manifest);

    let info = PluginInfo {
        manifest: manifest.clone(),
        path: plugin_dir.clone(),
        skill_count,
    };

    fs::remove_dir_all(&plugin_dir).with_context(|| {
        format!(
            "Failed to remove plugin directory: {}",
            plugin_dir.display()
        )
    })?;

    Ok(info)
}

/// Format plugin list for display.
pub fn format_plugin_list(plugins: &[PluginInfo]) -> String {
    if plugins.is_empty() {
        return "No plugins installed.\n\nInstall plugins with:\n  alphacode plugin install <path>\n  alphacode plugin install --git <url>".to_string();
    }

    let mut output = format!("Installed plugins ({}):\n\n", plugins.len());

    for plugin in plugins {
        output.push_str(&format!(
            "  {} v{}\n    {} | {} skills\n",
            plugin.manifest.name,
            plugin.manifest.version,
            if plugin.manifest.description.is_empty() {
                "No description"
            } else {
                &plugin.manifest.description
            },
            plugin.skill_count,
        ));

        if !plugin.manifest.tags.is_empty() {
            output.push_str(&format!("    Tags: {}\n", plugin.manifest.tags.join(", ")));
        }

        if !plugin.manifest.homepage.is_empty() {
            output.push_str(&format!("    URL: {}\n", plugin.manifest.homepage));
        }

        output.push('\n');
    }

    output
}

/// Format detailed info about a single plugin.
pub fn format_plugin_info(plugin: &PluginInfo) -> String {
    let mut output = format!(
        "Plugin: {} v{}\n\
         Author: {}\n\
         License: {}\n\
         Description: {}\n\
         Homepage: {}\n\
         Min version: {}\n\
         Skills: {}\n\
         Path: {}",
        plugin.manifest.name,
        plugin.manifest.version,
        if plugin.manifest.author.is_empty() {
            "Unknown"
        } else {
            &plugin.manifest.author
        },
        if plugin.manifest.license.is_empty() {
            "Unknown"
        } else {
            &plugin.manifest.license
        },
        if plugin.manifest.description.is_empty() {
            "No description"
        } else {
            &plugin.manifest.description
        },
        if plugin.manifest.homepage.is_empty() {
            "None"
        } else {
            &plugin.manifest.homepage
        },
        if plugin.manifest.min_version.is_empty() {
            "None"
        } else {
            &plugin.manifest.min_version
        },
        plugin.skill_count,
        plugin.path.display(),
    );

    if !plugin.manifest.tags.is_empty() {
        output.push_str(&format!("\nTags: {}", plugin.manifest.tags.join(", ")));
    }

    // List discovered skills
    let skills_dir = plugin.path.join("skills");
    if skills_dir.exists() {
        let skills = discover_skill_names(&plugin.path, &plugin.manifest);
        if !skills.is_empty() {
            output.push_str("\n\nProvided skills:\n");
            for skill in &skills {
                output.push_str(&format!("  /{}\n", skill));
            }
        }
    }

    output
}

// --- Internal helpers ---

/// Load a plugin manifest from a JSON file.
fn load_manifest(path: &Path) -> Result<PluginManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest: {}", path.display()))?;

    let manifest: PluginManifest = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse manifest: {}", path.display()))?;

    // Validate required fields
    if manifest.name.is_empty()
        || !manifest
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        anyhow::bail!("Plugin name must use letters, digits, hyphens or underscores");
    }
    if manifest.version.is_empty() {
        anyhow::bail!("Plugin manifest has empty version");
    }

    Ok(manifest)
}

/// Count skills provided by a plugin.
fn count_skills_in_plugin(plugin_dir: &Path, manifest: &PluginManifest) -> usize {
    if !manifest.skills.is_empty() {
        // Use explicit skill list from manifest
        manifest
            .skills
            .iter()
            .filter(|skill_path| plugin_dir.join(skill_path).join("SKILL.md").exists())
            .count()
    } else {
        // Auto-discover from skills/ directory
        let skills_dir = plugin_dir.join("skills");
        if !skills_dir.exists() {
            return 0;
        }
        fs::read_dir(&skills_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().join("SKILL.md").exists())
                    .count()
            })
            .unwrap_or(0)
    }
}

/// Discover skill names from a plugin.
fn discover_skill_names(plugin_dir: &Path, manifest: &PluginManifest) -> Vec<String> {
    if !manifest.skills.is_empty() {
        return manifest
            .skills
            .iter()
            .filter_map(|skill_path| {
                let dir = plugin_dir.join(skill_path);
                if dir.join("SKILL.md").exists() {
                    dir.file_name().map(|n| n.to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect();
    }

    let skills_dir = plugin_dir.join("skills");
    if !skills_dir.exists() {
        return Vec::new();
    }

    fs::read_dir(&skills_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().join("SKILL.md").exists())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    copy_dir_bounded(src, dst, 0)
}

fn copy_dir_bounded(src: &Path, dst: &Path, depth: usize) -> Result<()> {
    if depth > 32 || fs::symlink_metadata(src)?.file_type().is_symlink() {
        anyhow::bail!(
            "Unsupported linked or deeply nested plugin resource: {}",
            src.display()
        );
    }
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        let metadata = fs::symlink_metadata(&src_path)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!(
                "Linked plugin resources are unsupported: {}",
                src_path.display()
            );
        }
        if metadata.is_dir() {
            copy_dir_bounded(&src_path, &dst_path, depth + 1)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_plugin_import_preserves_existing_version() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("skills/custom")).unwrap();
        fs::write(
            source.join("plugin.json"),
            r#"{"name":"local-plugin","version":"1.0"}"#,
        )
        .unwrap();
        fs::write(source.join("skills/custom/SKILL.md"), "original").unwrap();
        let root = temp.path().join("installed");
        let result = install_new_plugin_into(&source, &root).unwrap();
        assert_eq!(result.skills_added, vec!["custom"]);
        fs::write(
            source.join("plugin.json"),
            r#"{"name":"local-plugin","version":"2.0"}"#,
        )
        .unwrap();
        fs::write(source.join("skills/custom/SKILL.md"), "replacement").unwrap();
        assert!(install_new_plugin_into(&source, &root).is_err());
        assert_eq!(
            fs::read_to_string(root.join("local-plugin/skills/custom/SKILL.md")).unwrap(),
            "original"
        );
    }

    #[test]
    fn local_manifest_cannot_escape_plugin_directory() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("plugin.json");
        for name in ["../outside", "C:\\outside", "a/b", "a\\b", "..", ""] {
            fs::write(
                &path,
                serde_json::json!({ "name": name, "version": "1.0" }).to_string(),
            )
            .unwrap();
            assert!(load_manifest(&path).is_err(), "accepted {name}");
        }
    }

    #[test]
    fn local_plugin_copy_preserves_nested_resources() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("skills/custom/references")).unwrap();
        fs::write(source.join("skills/custom/SKILL.md"), "instructions").unwrap();
        fs::write(
            source.join("skills/custom/references/control.md"),
            "control",
        )
        .unwrap();
        let destination = temp.path().join("destination");
        copy_dir_recursive(&source, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("skills/custom/references/control.md")).unwrap(),
            "control"
        );
    }

    #[test]
    fn plugin_manifest_roundtrip() {
        let manifest = PluginManifest {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            homepage: "https://example.com".to_string(),
            min_version: "1.0.0".to_string(),
            tags: vec!["test".to_string()],
            skills: vec![],
            tools: vec![],
        };

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed: PluginManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "test-plugin");
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.skills.len(), 0);
    }

    #[test]
    fn plugin_manifest_with_skills() {
        let json = r#"{
            "name": "my-plugin",
            "version": "2.0.0",
            "skills": ["skills/my-skill", "skills/another"]
        }"#;

        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.skills.len(), 2);
        assert_eq!(manifest.skills[0], "skills/my-skill");
    }

    #[test]
    fn plugin_manifest_defaults() {
        let json = r#"{"name": "minimal", "version": "0.1.0"}"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();

        assert!(manifest.description.is_empty());
        assert!(manifest.author.is_empty());
        assert!(manifest.tags.is_empty());
        assert!(manifest.skills.is_empty());
        assert!(manifest.tools.is_empty());
    }
}
