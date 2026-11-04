//! Import user-selected skill files and local plugin packages.
use super::{App, DisplayMessage};
use crate::alphacode_tui::tui::account_picker::{
    AccountPicker, AccountPickerCommand, AccountPickerItem,
};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

fn command_path(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/add")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

fn resolve_path(rest: &str, working_dir: &Path) -> Result<PathBuf> {
    let rest = if rest.starts_with(['\'', '"']) {
        let quote = rest.chars().next().unwrap();
        rest.strip_prefix(quote)
            .and_then(|s| s.strip_suffix(quote))
            .context("Close the quote around the path")?
    } else {
        rest
    };
    let path = if rest == "~" || rest.starts_with("~/") || rest.starts_with("~\\") {
        crate::storage::user_home_path(rest.get(2..).unwrap_or(""))?
    } else if rest.is_empty() {
        working_dir.to_path_buf()
    } else {
        working_dir.join(rest)
    };
    path.canonicalize()
        .with_context(|| format!("Cannot open {}", path.display()))
}

pub(super) fn handle_add_command(app: &mut App, input: &str) -> bool {
    let Some(rest) = command_path(input) else {
        return false;
    };
    let result = (|| -> Result<()> {
        let cwd = std::env::current_dir()?;
        let working = app
            .session
            .working_dir
            .as_deref()
            .map(Path::new)
            .filter(|p| p.is_dir())
            .unwrap_or(&cwd);
        let path = resolve_path(rest, working)?;
        if path.is_dir() && !path.join("plugin.json").is_file() {
            let items = browse_items(&path)?;
            app.account_picker_overlay = Some(std::cell::RefCell::new(AccountPicker::action_list(
                format!("Add skill / plugin — {}", path.display()),
                items,
            )));
            app.set_status_notice("Select a skill file or browse a folder; Esc cancels");
        } else {
            let message = import_path(&path)?;
            app.refresh_skills_snapshot();
            app.push_display_message(DisplayMessage::system(message).with_title("Added"));
            app.set_status_notice("Imported; use /skills to view available skills");
        }
        Ok(())
    })();
    if let Err(error) = result {
        app.push_display_message(DisplayMessage::error(format!(
            "Could not add: {error:#}\nUse /add [path] to select a skill file or plugin folder."
        )));
        app.set_status_notice("Import failed");
    }
    true
}

fn browse_items(path: &Path) -> Result<Vec<AccountPickerItem>> {
    let mut paths = std::fs::read_dir(path)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort_by(|a, b| {
        b.is_dir()
            .cmp(&a.is_dir())
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });
    if let Some(parent) = path.parent() {
        paths.insert(0, parent.to_path_buf());
    }
    Ok(paths
        .into_iter()
        .filter(|p| p.is_dir() || p.is_file())
        .map(|p| {
            let title = if Some(p.as_path()) == path.parent() {
                "..".to_string()
            } else {
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            };
            let kind = if p.is_dir() { "Folders" } else { "Files" };
            AccountPickerItem::action(
                kind,
                kind,
                title,
                p.display().to_string(),
                AccountPickerCommand::SubmitInput(format!("/add {}", p.display())),
            )
        })
        .collect())
}

fn import_path(path: &Path) -> Result<String> {
    let manifest_file = path.is_file()
        && path.file_name().is_some_and(|name| name == "plugin.json")
        && !std::fs::read_to_string(path)?
            .trim_start()
            .starts_with("---");
    let path = if manifest_file {
        path.parent()
            .context("Plugin manifest needs a parent directory")?
    } else {
        path
    };
    if path.is_dir() {
        let manifest: crate::alphacode_app_core::plugin::PluginManifest =
            serde_json::from_str(&std::fs::read_to_string(path.join("plugin.json"))?)?;
        // /add installs a new local package; replacement stays an explicit CLI operation.
        if manifest.name.is_empty()
            || !manifest
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("Plugin name must use letters, digits, hyphens or underscores");
        }
        if crate::alphacode_app_core::plugin::plugins_dir()?
            .join(&manifest.name)
            .try_exists()?
        {
            bail!("Plugin '{}' is already installed", manifest.name);
        }
        // Validate the entire copy before the installer creates its destination.
        for entry in walkdir::WalkDir::new(path).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_symlink() || entry.depth() > 32 {
                bail!("Linked or deeply nested plugin resources are unsupported");
            }
        }
        let result = crate::alphacode_app_core::plugin::install_new_plugin(path)?;
        Ok(format!(
            "Installed plugin {} with {} skill(s).\n{}\nCustom executable plugin tools are not supported.",
            result.plugin.manifest.name,
            result.skills_added.len(),
            result.plugin.path.display()
        ))
    } else {
        let skill = crate::skill::SkillRegistry::install_file(path)?;
        Ok(format!(
            "Installed /{}\n{}\n{}",
            skill.name,
            skill.description,
            skill.path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_add_and_preserves_paths_with_spaces() {
        assert_eq!(command_path("/additional"), None);
        assert_eq!(command_path("/add\tmy skill.txt"), Some("my skill.txt"));
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("any name.txt"), "text").unwrap();
        assert_eq!(
            resolve_path("\"any name.txt\"", temp.path()).unwrap(),
            temp.path().join("any name.txt").canonicalize().unwrap()
        );
        assert!(resolve_path("\"unclosed", temp.path()).is_err());
        assert!(resolve_path("missing", temp.path()).is_err());
    }

    #[test]
    fn browser_includes_arbitrary_filenames_and_folders() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("instructions.custom"), "text").unwrap();
        std::fs::create_dir(temp.path().join("nested")).unwrap();
        let items = browse_items(temp.path()).unwrap();
        assert!(items.iter().any(|i| i.title == "instructions.custom"));
        assert!(items.iter().any(|i| i.title == "nested"));
        assert!(items.iter().any(|i| i.title == ".."));
    }
}
