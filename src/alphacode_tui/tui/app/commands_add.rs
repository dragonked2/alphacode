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

pub(super) struct PendingAdd {
    request: u64,
    session_id: String,
    result: std::sync::mpsc::Receiver<Result<AddOutcome, String>>,
}

enum AddOutcome {
    Browse(PathBuf, Vec<AccountPickerItem>),
    Imported(String, Box<crate::skill::SkillRegistry>),
}

pub(super) fn handle_add_command(app: &mut App, input: &str) -> bool {
    let Some(rest) = command_path(input) else {
        return false;
    };
    if app.pending_add.is_some() {
        // A directory scan is usually complete before the next key. Poll it
        // first, but never launch overlapping imports or lose their result.
        poll_add(app);
        if app.pending_add.is_some() {
            app.set_status_notice("Still opening the selection; Esc closes the browser");
            return true;
        }
    }
    let (browse_only, rest) = rest
        .strip_prefix("--browse ")
        .map_or((false, rest), |path| (true, path));
    let rest = rest.to_owned();
    let working = app
        .session
        .working_dir
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    static NEXT_REQUEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let request = NEXT_REQUEST.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let session_id = app
        .active_client_session_id()
        .unwrap_or(&app.session.id)
        .to_owned();
    let (tx, result) = std::sync::mpsc::channel();
    app.account_picker_overlay = Some(std::cell::RefCell::new(AccountPicker::file_browser(
        request,
        if rest.is_empty() {
            working.display().to_string()
        } else {
            rest.clone()
        },
    )));
    app.set_status_notice("Opening selection…");
    let wake_session = session_id.clone();
    match std::thread::Builder::new()
        .name("skill-file-picker".into())
        .spawn(move || {
            let outcome = (|| -> Result<AddOutcome> {
                let path = resolve_path(&rest, &working)?;
                if path.is_dir() && (browse_only || !path.join("plugin.json").is_file()) {
                    let items = browse_items(&path)?;
                    Ok(AddOutcome::Browse(path, items))
                } else {
                    let message = import_path(&path)?;
                    let skills = crate::skill::SkillRegistry::load_global()?;
                    Ok(AddOutcome::Imported(message, Box::new(skills)))
                }
            })()
            .map_err(|error| format!("{error:#}"));
            if tx.send(outcome).is_ok() {
                // Wake either idle loop without adding a transcript entry.
                crate::bus::Bus::global().publish(crate::bus::BusEvent::UiActivity(
                    crate::bus::UiActivity::catalog(Some(wake_session), "", None::<String>),
                ));
            }
        }) {
        Ok(_) => {
            app.pending_add = Some(PendingAdd {
                request,
                session_id,
                result,
            })
        }
        Err(error) => {
            app.account_picker_overlay = None;
            app.push_display_message(DisplayMessage::error(format!(
                "Could not open selection: {error}"
            )));
        }
    }
    true
}

pub(super) fn poll_add(app: &mut App) -> bool {
    use std::sync::mpsc::TryRecvError;
    let Some(pending) = app.pending_add.as_ref() else {
        return false;
    };
    let result = match pending.result.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return false,
        Err(TryRecvError::Disconnected) => Err("File selection worker stopped unexpectedly".into()),
    };
    let pending = app.pending_add.take().expect("pending selection");
    let same_session =
        app.active_client_session_id().unwrap_or(&app.session.id) == pending.session_id;
    let visible = same_session
        && app
            .account_picker_overlay
            .as_ref()
            .is_some_and(|picker| picker.borrow().is_file_browser_request(pending.request));
    match result {
        Ok(AddOutcome::Browse(path, items)) if visible => {
            if let Some(picker) = &app.account_picker_overlay {
                picker
                    .borrow_mut()
                    .set_file_browser_items(path.display().to_string(), items);
            }
            app.set_status_notice("Choose a .md / .txt skill file or open a folder");
        }
        Ok(AddOutcome::Imported(message, skills)) => {
            app.skills = std::sync::Arc::new((*skills).clone());
            if let Ok(mut shared) = app.registry.skills().try_write() {
                *shared = *skills;
            }
            app.invalidate_command_candidates_cache();
            crate::alphacode_tui::tui::ui::prepare::invalidate_header_prep_cache();
            if visible {
                app.account_picker_overlay = None;
            }
            if same_session {
                app.push_display_message(DisplayMessage::system(message).with_title("Added"));
                app.set_status_notice("Imported; use /skills to view available skills");
            }
        }
        Err(error) if visible => {
            app.account_picker_overlay = None;
            app.push_display_message(DisplayMessage::error(format!(
                "Could not add: {error}\nUse /add [path] to select a .md / .txt skill file or plugin folder."
            )));
            app.set_status_notice("Import failed");
        }
        _ => {} // Closing/replacing the browser must never reopen it.
    }
    true
}

fn browse_items(path: &Path) -> Result<Vec<AccountPickerItem>> {
    // Cache classification and case-folded sort keys once per entry. Repeated
    // Path::is_dir in a sort comparator is especially slow on cloud folders.
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let folder = kind.is_dir();
        let child = entry.path();
        if !folder
            && !(kind.is_file()
                && child.extension().is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("txt")
                }))
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        entries.push((!folder, name.to_lowercase(), name, child));
    }
    entries.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    let mut items = Vec::with_capacity(entries.len() + 1);
    if let Some(parent) = path.parent() {
        items.push(browser_item(parent, "..".into(), true));
    }
    items.extend(
        entries
            .into_iter()
            .map(|(file, _, name, child)| browser_item(&child, name, !file)),
    );
    Ok(items)
}

fn browser_item(path: &Path, title: String, folder: bool) -> AccountPickerItem {
    let kind = if folder { "Folders" } else { "Files" };
    AccountPickerItem::action(
        kind,
        kind,
        title,
        if folder {
            "Open folder"
        } else {
            "Import skill"
        },
        AccountPickerCommand::SubmitInput(format!(
            "/add {}\"{}\"",
            if folder { "--browse " } else { "" },
            path.display()
        )),
    )
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
    fn browser_only_lists_immediate_markdown_text_files_and_folders() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("instructions.custom"), "text").unwrap();
        std::fs::create_dir(temp.path().join("nested")).unwrap();
        std::fs::write(temp.path().join("skill.MD"), "text").unwrap();
        std::fs::write(temp.path().join("notes.txt"), "text").unwrap();
        std::fs::write(temp.path().join("nested/hidden.md"), "text").unwrap();
        let items = browse_items(temp.path()).unwrap();
        assert!(items.iter().any(|i| i.title == "skill.MD"));
        assert!(items.iter().any(|i| i.title == "notes.txt"));
        assert!(!items.iter().any(|i| i.title == "hidden.md"));
        assert!(!items.iter().any(|i| i.title == "instructions.custom"));
        assert!(items.iter().any(|i| i.title == "nested"));
        assert!(items.iter().any(|i| i.title == ".."));
    }
}
