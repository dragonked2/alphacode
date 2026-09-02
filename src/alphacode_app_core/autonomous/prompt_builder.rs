//! Prompt Builder — constructs fresh prompts from state and relevant context.
//!
//! Every inference should build a fresh prompt.  The prompt consists of:
//! system rules, current goal, current phase, relevant files, relevant
//! memory, relevant summaries, coding standards, tool results, and
//! acceptance criteria.  Never include irrelevant history.
//!
//! Token discipline: project state (objective / active phase / completed
//! phases) is injected once by the orchestrator's own system prompt, not
//! copied into every spawned worker's system prompt — a swarm of N workers
//! otherwise pays for N duplicate copies.  Long state fields are capped so a
//! months-long project cannot balloon a fresh prompt.

use super::AgentSpec;
use crate::alphacode_app_core::memory_manager::MemoryManager;
use crate::alphacode_app_core::memory_manager::ProjectIndex;
use anyhow::Result;

/// Cap for the persisted project objective when injected into a prompt.
/// Long objectives (some users keep a running journal in there) are truncated
/// with an ellipsis; the full text still lives in project state.
const MAX_OBJECTIVE_CHARS: usize = 800;
/// Only the most recent completed phases are worth repeating; older ones are
/// progress history, not actionable context.
const MAX_COMPLETED_PHASES: usize = 5;
/// Recovery prompts embed the last checkpoint summary; beyond this it is more
/// recap than signal and costs tokens on every retry.
const MAX_CHECKPOINT_SUMMARY_CHARS: usize = 2_000;

/// Truncate `text` to `max_chars` characters, appending an ellipsis when cut.
/// Cuts on `char` boundaries so wide/CJK text is never split mid-glyph.
fn cap_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A prompt built by the PromptBuilder.
#[derive(Debug, Clone)]
pub struct AgentPrompt {
    /// System rules (always present).
    pub system: String,
    /// The main prompt body (goal + context).
    pub body: String,
}

/// PromptBuilder constructs fresh prompts for each inference.
pub struct PromptBuilder {
    /// Shared memory manager for state retrieval.
    memory: MemoryManager,
}

impl PromptBuilder {
    pub fn new(memory: MemoryManager) -> Self {
        Self { memory }
    }

    /// Build a system prompt with the project's coding standards and rules,
    /// including current project state (objective / phase / progress).
    ///
    /// Used for the orchestrator and recovery prompts, where project state is
    /// the agent's job context. Worker agents get the leaner
    /// [`Self::worker_system_prompt`] instead so state is not duplicated
    /// once per spawned agent.
    pub fn system_prompt(&self, rules: &[String]) -> String {
        self.system_prompt_with_state(rules, true)
    }

    /// Worker system prompt: rules + coding standards only.
    ///
    /// The objective and any project state live in the worker's prompt body
    /// (built from its `AgentSpec`), so repeating them in the system section
    /// would multiply token cost across parallel sub-agents for zero new
    /// information.
    pub fn worker_system_prompt(&self, rules: &[String]) -> String {
        self.system_prompt_with_state(rules, false)
    }

    fn system_prompt_with_state(&self, rules: &[String], include_state: bool) -> String {
        let mut parts = Vec::new();

        parts.push("# Agent Directives\n".to_string());
        parts.push(
            "You are a persistent autonomous software engineering agent. \
             The system owns memory, planning, coordination, state, \
             recovery, parallelism, and execution.  You are only the \
             reasoning engine for the current task."
                .to_string(),
        );

        if !rules.is_empty() {
            parts.push("\n## Project Rules".to_string());
            for r in rules {
                parts.push(format!("- {r}"));
            }
        }

        if include_state {
            // Append current project context for awareness, capped so a long
            // project cannot balloon every fresh prompt.
            if let Ok(state) = self.memory.load_state() {
                if !state.objective.is_empty() {
                    parts.push(format!(
                        "\n## Current Objective\n{}",
                        cap_text(&state.objective, MAX_OBJECTIVE_CHARS)
                    ));
                }
                if let Some(phase) = &state.active_phase {
                    parts.push(format!("\n## Current Phase\n{phase}"));
                }
                let len = state.completed_phases.len();
                let start = len.saturating_sub(MAX_COMPLETED_PHASES);
                if start < len {
                    parts.push("\n## Recent Completed Phases".to_string());
                    for p in &state.completed_phases[start..] {
                        parts.push(format!("- {p}"));
                    }
                }
            }
        }

        parts.join("\n")
    }

    /// Build a prompt for a specific agent spec.
    ///
    /// The agent prompt includes: objective, constraints, required files,
    /// relevant summaries, coding standards, and acceptance criteria.
    /// It never includes irrelevant history.
    pub fn build_agent_prompt(&self, spec: &AgentSpec, index: &ProjectIndex) -> AgentPrompt {
        let system = self.worker_system_prompt(&spec.coding_standards);

        let mut body = Vec::new();

        body.push(format!("# Objective\n{}", spec.objective));

        if !spec.constraints.is_empty() {
            body.push("\n## Constraints".to_string());
            for c in &spec.constraints {
                body.push(format!("- {c}"));
            }
        }

        if !spec.required_files.is_empty() {
            body.push("\n## Required Files".to_string());
            for f in &spec.required_files {
                if let Some(meta) = index.files.get(f) {
                    body.push(format!(
                        "- {} ({} lines, {})",
                        f,
                        meta.lines,
                        meta.language.as_deref().unwrap_or("unknown")
                    ));
                } else {
                    body.push(format!("- {f}"));
                }
            }
        }

        if !spec.relevant_summaries.is_empty() {
            body.push("\n## Relevant Summaries".to_string());
            for s in &spec.relevant_summaries {
                body.push(format!("- {}", cap_text(s, 600)));
            }
        }

        if !spec.acceptance_criteria.is_empty() {
            body.push("\n## Acceptance Criteria".to_string());
            for c in &spec.acceptance_criteria {
                body.push(format!("- {c}"));
            }
        }

        AgentPrompt {
            system,
            body: body.join("\n"),
        }
    }

    /// Build a recovery prompt after a crash.
    ///
    /// The recovery prompt instructs the agent to continue exactly where
    /// it stopped.  It includes the objective, active phase, and last
    /// checkpoint summary.
    pub fn build_recovery_prompt(&self, checkpoint_summary: &str) -> Result<AgentPrompt> {
        let state = self.memory.load_state().unwrap_or_default();

        let system = self.system_prompt(&[]);

        let mut body = Vec::new();
        body.push("# Recovery — Continue Where You Stopped".to_string());
        body.push(format!("Objective: {}", state.objective));
        if let Some(phase) = &state.active_phase {
            body.push(format!("Active Phase: {phase}"));
        }
        if !checkpoint_summary.is_empty() {
            body.push(format!(
                "\nLast Checkpoint Summary:\n{}",
                cap_text(checkpoint_summary, MAX_CHECKPOINT_SUMMARY_CHARS)
            ));
        }
        body.push(
            "\nReview what's been done so far, then continue the work. \
             Do not restart from scratch."
                .to_string(),
        );

        Ok(AgentPrompt {
            system,
            body: body.join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_builder() -> (TempDir, PromptBuilder) {
        let dir = TempDir::new().unwrap();
        let memory = MemoryManager::new(dir.path()).unwrap();
        let builder = PromptBuilder::new(memory);
        (dir, builder)
    }

    #[test]
    fn test_system_prompt_includes_rules() {
        let (_dir, builder) = make_builder();
        let system = builder.system_prompt(&["No FIXME comments".to_string()]);
        assert!(system.contains("No FIXME comments"));
        assert!(system.contains("autonomous"));
    }

    #[test]
    fn test_build_agent_prompt() {
        let (_dir, builder) = make_builder();
        let spec = AgentSpec {
            id: "a1".into(),
            label: "worker".into(),
            objective: "Write tests for module X".into(),
            required_files: vec!["src/x.rs".into()],
            relevant_summaries: vec!["Module X handles HTTP".into()],
            constraints: vec!["Edge cases must be covered".into()],
            coding_standards: vec!["Use anyhow::Result".into()],
            acceptance_criteria: vec!["All tests pass".into()],
            model: None,
            max_depth: 0,
            created_at: chrono::Utc::now(),
        };
        let index = ProjectIndex::default();
        let prompt = builder.build_agent_prompt(&spec, &index);
        assert!(prompt.body.contains("Write tests for module X"));
        assert!(prompt.body.contains("Edge cases must be covered"));
        assert!(prompt.body.contains("src/x.rs"));
        // Coding standards live in the system prompt (project rules), not the body.
        assert!(prompt.system.contains("Use anyhow::Result"));
        assert!(prompt.body.contains("All tests pass"));
    }

    #[test]
    fn worker_system_prompt_omits_project_state() {
        let (_dir, builder) = make_builder();
        builder
            .memory
            .update_state(|s| {
                s.objective = "Build a browser".into();
                s.active_phase = Some("CSS Engine".into());
            })
            .unwrap();

        // The orchestrator prompt carries state…
        let with_state = builder.system_prompt(&[]);
        assert!(with_state.contains("Build a browser"));
        assert!(with_state.contains("CSS Engine"));

        // …but a spawned worker's system prompt must not duplicate it.
        let worker = builder.worker_system_prompt(&[]);
        assert!(!worker.contains("Build a browser"));
        assert!(!worker.contains("CSS Engine"));
        assert!(!worker.contains("Current Objective"));
    }

    #[test]
    fn agent_prompt_uses_lean_worker_system_prompt() {
        let (_dir, builder) = make_builder();
        builder
            .memory
            .update_state(|s| {
                s.objective = "Build a browser".into();
            })
            .unwrap();
        let spec = AgentSpec {
            id: "a1".into(),
            label: "worker".into(),
            objective: "Write tests for module X".into(),
            required_files: Vec::new(),
            relevant_summaries: Vec::new(),
            constraints: Vec::new(),
            coding_standards: Vec::new(),
            acceptance_criteria: Vec::new(),
            model: None,
            max_depth: 0,
            created_at: chrono::Utc::now(),
        };
        let prompt = builder.build_agent_prompt(&spec, &ProjectIndex::default());
        // Objective appears exactly once (in the body), not duplicated in system.
        assert_eq!(prompt.body.matches("Write tests for module X").count(), 1);
        assert!(!prompt.system.contains("Build a browser"));
    }

    #[test]
    fn long_objective_is_capped_in_system_prompt() {
        let (_dir, builder) = make_builder();
        let long = "x".repeat(2_000);
        builder
            .memory
            .update_state(|s| s.objective = long.clone())
            .unwrap();

        let system = builder.system_prompt(&[]);
        assert!(system.contains(&cap_text(&long, MAX_OBJECTIVE_CHARS)));
        assert!(!system.contains(&"x".repeat(1_500)));
    }

    #[test]
    fn only_recent_completed_phases_are_injected() {
        let (_dir, builder) = make_builder();
        let phases: Vec<String> = (0..12).map(|i| format!("phase-{i}")).collect();
        builder
            .memory
            .update_state(|s| s.completed_phases = phases.clone())
            .unwrap();

        let system = builder.system_prompt(&[]);
        // The oldest phases are dropped; the newest are kept.
        assert!(!system.contains("phase-0"));
        assert!(!system.contains("phase-6"));
        assert!(system.contains("phase-7"));
        assert!(system.contains("phase-11"));
    }

    #[test]
    fn test_recovery_prompt() {
        let (_dir, builder) = make_builder();
        builder
            .memory
            .update_state(|s| {
                s.objective = "Build a browser".into();
                s.active_phase = Some("CSS Engine".into());
            })
            .unwrap();

        let prompt = builder
            .build_recovery_prompt("Architecture complete, networking done")
            .unwrap();
        assert!(prompt.body.contains("Build a browser"));
        assert!(prompt.body.contains("CSS Engine"));
        assert!(prompt.body.contains("Architecture complete"));
        assert!(prompt.body.contains("Continue"));
    }
}
