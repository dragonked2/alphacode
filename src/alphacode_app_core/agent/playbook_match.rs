//! Playbook matching: pattern→action pairs that make the agent an expert
//! by default. Experts pattern-match; novices derive.
//!
//! When a playbook matches the current environment, the agent's first action
//! is the playbook's validated action, not a fresh derivation.

#![allow(dead_code)]

use std::path::Path;

/// A playbook entry: observed signature → validated action.
#[derive(Debug, Clone)]
pub struct Playbook {
    /// Unique identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// What triggers this playbook.
    pub trigger: PlaybookTrigger,
    /// The validated first action to take.
    pub action: PlaybookAction,
    /// Success rate of this playbook (0.0 - 1.0).
    pub success_rate: f64,
    /// Where this playbook came from.
    pub source: PlaybookSource,
}

/// What environment pattern triggers this playbook.
#[derive(Debug, Clone)]
pub enum PlaybookTrigger {
    /// Tech stack detected + vulnerable pattern.
    TechStack {
        detected: Vec<String>,
        pattern: String,
    },
    /// A specific tool sequence was observed.
    ToolSequence {
        tools: Vec<String>,
        min_occurrences: u32,
    },
    /// A file matching a glob pattern exists (and optionally contains text).
    FilePattern {
        glob: String,
        contains: Option<String>,
    },
    /// Objective matches a keyword pattern.
    ObjectivePattern { keywords: Vec<String> },
}

/// What action to take when the playbook matches.
#[derive(Debug, Clone)]
pub enum PlaybookAction {
    /// Execute a specific tool with templated input.
    ToolCall {
        name: String,
        input_template: String,
    },
    /// Load a specific skill.
    SkillLoad { name: String },
    /// Inject an advisory system-reminder (no tool call).
    Advisory(String),
}

/// Where the playbook came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybookSource {
    /// Learned from self_improve task records.
    Learned,
    /// Shipped with alphacode.
    Bundled,
    /// User-defined in .alphacode/playbooks/.
    UserDefined,
}

/// Match environment signals against playbooks and return the best match.
pub fn match_playbook(
    objective: &str,
    detected_tech: &[String],
    recent_tools: &[String],
    working_dir: &Path,
) -> Option<Playbook> {
    let playbooks = load_playbooks(working_dir);

    let mut best: Option<(Playbook, f64)> = None;

    for playbook in playbooks {
        let score = score_playbook(
            &playbook,
            objective,
            detected_tech,
            recent_tools,
            working_dir,
        );
        if score > 0.0 {
            match &best {
                Some((_, best_score)) if score > *best_score => {
                    best = Some((playbook, score));
                }
                None => {
                    best = Some((playbook, score));
                }
                _ => {}
            }
        }
    }

    best.map(|(playbook, _)| playbook)
}

/// Score a playbook against the current environment (0.0 - 1.0).
fn score_playbook(
    playbook: &Playbook,
    objective: &str,
    detected_tech: &[String],
    recent_tools: &[String],
    working_dir: &Path,
) -> f64 {
    match &playbook.trigger {
        PlaybookTrigger::TechStack { detected, pattern } => {
            let tech_match = detected
                .iter()
                .filter(|t| detected_tech.iter().any(|dt| dt.eq_ignore_ascii_case(t)))
                .count() as f64
                / detected.len().max(1) as f64;

            let pattern_match = if objective.to_lowercase().contains(&pattern.to_lowercase()) {
                1.0
            } else {
                0.0
            };

            (tech_match * 0.6 + pattern_match * 0.4) * playbook.success_rate
        }
        PlaybookTrigger::ToolSequence {
            tools,
            min_occurrences: _,
        } => {
            if tools.len() > recent_tools.len() {
                return 0.0;
            }
            // Check if the recent tool sequence contains the trigger sequence
            let window = recent_tools.windows(tools.len());
            let matches = window
                .filter(|w| w.iter().zip(tools.iter()).all(|(a, b)| a == b))
                .count();
            if matches > 0 {
                playbook.success_rate
            } else {
                0.0
            }
        }
        PlaybookTrigger::FilePattern { glob, contains } => {
            // Simple glob matching (check if any file matches)
            let pattern = glob.trim_start_matches("./");
            let matches = match glob_match(pattern, working_dir) {
                Ok(m) => m,
                Err(_) => return 0.0,
            };

            if !matches {
                return 0.0;
            }

            if let Some(contains_text) = contains {
                // Check if any matching file contains the text
                if let Ok(entries) = std::fs::read_dir(working_dir) {
                    for entry in entries.flatten() {
                        if let Ok(content) = std::fs::read_to_string(entry.path())
                            && content.contains(contains_text.as_str())
                        {
                            return playbook.success_rate;
                        }
                    }
                }
                0.0
            } else {
                playbook.success_rate
            }
        }
        PlaybookTrigger::ObjectivePattern { keywords } => {
            let lower = objective.to_lowercase();
            let matches = keywords
                .iter()
                .filter(|k| lower.contains(&k.to_lowercase()))
                .count() as f64
                / keywords.len().max(1) as f64;
            matches * playbook.success_rate
        }
    }
}

/// Simple glob matching (checks if pattern exists in directory tree).
fn glob_match(pattern: &str, dir: &Path) -> Result<bool, std::io::Error> {
    // Simple prefix matching for common patterns
    if pattern.contains('*') {
        let prefix = pattern.split('*').next().unwrap_or("");
        for entry in walkdir::WalkDir::new(dir).max_depth(3) {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(prefix) {
                return Ok(true);
            }
        }
        Ok(false)
    } else {
        Ok(dir.join(pattern).exists())
    }
}

/// Load playbooks from bundled + user-defined sources.
fn load_playbooks(working_dir: &Path) -> Vec<Playbook> {
    let mut playbooks = Vec::new();

    // Bundled playbooks (common patterns)
    playbooks.extend(bundled_playbooks());

    // User-defined playbooks from .alphacode/playbooks/
    let playbook_dir = working_dir.join(".alphacode").join("playbooks");
    if let Ok(entries) = std::fs::read_dir(&playbook_dir) {
        for entry in entries.flatten() {
            if let Some(playbook) = load_playbook_file(&entry.path()) {
                playbooks.push(playbook);
            }
        }
    }

    playbooks
}

/// Load a single playbook from a JSON file.
fn load_playbook_file(path: &Path) -> Option<Playbook> {
    let content = std::fs::read_to_string(path).ok()?;
    let _value: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Parse the playbook JSON (simplified for now)
    let id = path.file_stem()?.to_string_lossy().to_string();
    Some(Playbook {
        id: id.clone(),
        name: id,
        trigger: PlaybookTrigger::ObjectivePattern {
            keywords: vec!["placeholder".into()],
        },
        action: PlaybookAction::Advisory("Playbook loaded".into()),
        success_rate: 0.5,
        source: PlaybookSource::UserDefined,
    })
}

/// Bundled playbooks for common patterns.
fn bundled_playbooks() -> Vec<Playbook> {
    vec![
        Playbook {
            id: "angular-xss".into(),
            name: "Angular XSS".into(),
            trigger: PlaybookTrigger::TechStack {
                detected: vec!["angular".into(), "javascript".into()],
                pattern: "xss".into(),
            },
            action: PlaybookAction::Advisory(
                "Angular detected with XSS objective. Known payload: {{7*7}} for template injection, ng-app for auto-initialization.".into()
            ),
            success_rate: 0.9,
            source: PlaybookSource::Bundled,
        },
        Playbook {
            id: "rust-build-test".into(),
            name: "Rust Build & Test".into(),
            trigger: PlaybookTrigger::FilePattern {
                glob: "Cargo.toml".into(),
                contains: None,
            },
            action: PlaybookAction::ToolCall {
                name: "bash".into(),
                input_template: "cargo build 2>&1 && cargo test 2>&1".into(),
            },
            success_rate: 0.95,
            source: PlaybookSource::Bundled,
        },
        Playbook {
            id: "python-pip-install".into(),
            name: "Python Dependencies".into(),
            trigger: PlaybookTrigger::FilePattern {
                glob: "requirements.txt".into(),
                contains: None,
            },
            action: PlaybookAction::ToolCall {
                name: "bash".into(),
                input_template: "pip install -r requirements.txt".into(),
            },
            success_rate: 0.85,
            source: PlaybookSource::Bundled,
        },
    ]
}

/// Generate advisory text for a matched playbook.
pub fn playbook_advisory(playbook: &Playbook) -> String {
    format!(
        "<system-reminder>\nPlaybook '{}' matched (confidence: {:.0}%). Recommended first action: {}\n</system-reminder>",
        playbook.name,
        playbook.success_rate * 100.0,
        match &playbook.action {
            PlaybookAction::ToolCall { name, .. } => format!("execute {}", name),
            PlaybookAction::SkillLoad { name } => format!("load skill '{}'", name),
            PlaybookAction::Advisory(text) => text.clone(),
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_playbooks_exist() {
        let playbooks = bundled_playbooks();
        assert!(!playbooks.is_empty());
    }

    #[test]
    fn score_tech_stack_match() {
        let playbook = Playbook {
            id: "test".into(),
            name: "Test".into(),
            trigger: PlaybookTrigger::TechStack {
                detected: vec!["angular".into()],
                pattern: "xss".into(),
            },
            action: PlaybookAction::Advisory("test".into()),
            success_rate: 0.9,
            source: PlaybookSource::Bundled,
        };

        let score = score_playbook(
            &playbook,
            "find xss vulnerability",
            &["angular".into(), "javascript".into()],
            &[],
            Path::new("."),
        );
        assert!(score > 0.5);
    }

    #[test]
    fn score_no_match() {
        let playbook = Playbook {
            id: "test".into(),
            name: "Test".into(),
            trigger: PlaybookTrigger::TechStack {
                detected: vec!["angular".into()],
                pattern: "xss".into(),
            },
            action: PlaybookAction::Advisory("test".into()),
            success_rate: 0.9,
            source: PlaybookSource::Bundled,
        };

        let score = score_playbook(
            &playbook,
            "fix the bug",
            &["python".into()],
            &[],
            Path::new("."),
        );
        assert_eq!(score, 0.0);
    }

    #[test]
    fn advisory_format() {
        let playbook = Playbook {
            id: "test".into(),
            name: "Test Playbook".into(),
            trigger: PlaybookTrigger::ObjectivePattern {
                keywords: vec!["test".into()],
            },
            action: PlaybookAction::Advisory("Do X".into()),
            success_rate: 0.8,
            source: PlaybookSource::Bundled,
        };

        let advisory = playbook_advisory(&playbook);
        assert!(advisory.contains("Test Playbook"));
        assert!(advisory.contains("80%"));
    }
}
