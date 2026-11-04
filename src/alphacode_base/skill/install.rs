use super::*;
use anyhow::{Context, bail};

impl SkillRegistry {
    /// Import a user-selected file of any name into the normal skill layout.
    /// Resources keep their relative paths; installation never executes them.
    pub fn install_file(source: &Path) -> Result<Skill> {
        let root = crate::storage::alphacode_dir()?.join("skills");
        Self::install_file_into(source, &root)
    }

    fn install_file_into(source: &Path, root: &Path) -> Result<Skill> {
        let skill = Self::parse_skill(source)
            .with_context(|| format!("Invalid skill file: {}", source.display()))?;
        validate_install_name(&skill.name)?;
        if skill.description.trim().is_empty() || skill.content.trim().is_empty() {
            bail!("A skill needs a nonempty description and instruction body");
        }
        std::fs::create_dir_all(root)?;
        let destination = root.join(skill.name.split_whitespace().collect::<Vec<_>>().join("-"));
        if destination.try_exists()? {
            bail!(
                "Skill '{}' is already installed at {}",
                skill.name,
                destination.display()
            );
        }
        let staging = tempfile::tempdir_in(root)?;
        std::fs::copy(source, staging.path().join("SKILL.md"))?;
        let parent = source.parent().unwrap_or_else(|| Path::new("."));
        // Do not copy the whole working directory when a loose file is selected.
        for resource in ["references", "scripts", "assets", "agents"] {
            let path = parent.join(resource);
            if path.try_exists()? {
                copy_resources(&path, &staging.path().join(resource), 0)?;
            }
        }
        let mut installed = Self::parse_skill(&staging.path().join("SKILL.md"))?;
        // create_dir provides exclusive ownership even if two imports race.
        std::fs::create_dir(&destination)?;
        if let Err(error) = copy_resources(staging.path(), &destination, 0) {
            let _ = std::fs::remove_dir_all(&destination);
            return Err(error);
        }
        installed.path = destination.join("SKILL.md");
        Ok(installed)
    }
}

fn validate_install_name(name: &str) -> Result<()> {
    if name.trim().is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b' ')
    {
        bail!("Skill name must contain 1–64 letters, digits, spaces, hyphens or underscores");
    }
    Ok(())
}

fn copy_resources(source: &Path, destination: &Path, depth: usize) -> Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() || depth > 32 {
        bail!(
            "Unsupported linked or deeply nested resource: {}",
            source.display()
        );
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_resources(
                &entry.path(),
                &destination.join(entry.file_name()),
                depth + 1,
            )?;
        }
    } else if metadata.is_file() {
        std::fs::copy(source, destination)?;
    } else {
        bail!("Unsupported resource: {}", source.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_filename_imports_with_resources_and_reloads() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir_all(source.join("references")).unwrap();
        std::fs::write(
            source.join("my hunting notes.txt"),
            "---\nname: custom-hunt\ndescription: Test a boundary\n---\nUse the owned fixture.",
        )
        .unwrap();
        std::fs::write(source.join("references/control.md"), "Control notes").unwrap();
        std::fs::write(source.join("unrelated.txt"), "Do not import").unwrap();
        let root = temp.path().join("installed");
        let installed =
            SkillRegistry::install_file_into(&source.join("my hunting notes.txt"), &root).unwrap();
        assert_eq!(installed.name, "custom-hunt");
        assert_eq!(
            std::fs::read_to_string(root.join("custom-hunt/references/control.md")).unwrap(),
            "Control notes"
        );
        assert!(!root.join("custom-hunt/unrelated.txt").exists());
        let mut registry = SkillRegistry::default();
        registry.load_from_dir(&root).unwrap();
        assert!(registry.get("custom-hunt").is_some());
        assert!(
            SkillRegistry::install_file_into(&source.join("my hunting notes.txt"), &root).is_err()
        );
    }

    #[test]
    fn invalid_skill_does_not_create_installation() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("arbitrary.md");
        let root = temp.path().join("installed");
        for text in [
            "ordinary notes",
            "---\nname: ../escape\ndescription: test\n---\nBody",
            "---\nname: empty\ndescription: test\n---\n",
        ] {
            std::fs::write(&source, text).unwrap();
            assert!(SkillRegistry::install_file_into(&source, &root).is_err());
        }
        assert!(!root.exists());
    }
}
