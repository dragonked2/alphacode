//! Task Decomposition — estimates complexity and recursively splits tasks.
//!
//! Before executing anything, the system estimates complexity.  If
//! complexity exceeds a configurable threshold, the task is automatically
//! divided into smaller phases.  If a phase is still too large, additional
//! child agents are spawned.  Recursive decomposition is allowed until
//! each task is manageable.

use super::{AgentLimits, AgentSpec, TaskComplexity};
use chrono::Utc;

/// A decomposed task tree.
#[derive(Debug, Clone)]
pub struct Decomposition {
    pub original: String,
    pub complexity: TaskComplexity,
    /// Child tasks if the original was decomposed.
    pub children: Vec<DecompositionNode>,
}

/// A single node in the decomposition tree.
#[derive(Debug, Clone)]
pub struct DecompositionNode {
    pub spec: AgentSpec,
    /// None if leaf, Some if further decomposed.
    pub children: Option<Vec<DecompositionNode>>,
}

/// Decompose a task into phases and agent specs.
///
/// Returns a tree of `DecompositionNode`s.  Each leaf node is an `AgentSpec`
/// ready to be spawned.  Each internal node has children that must be
/// completed before the parent.
pub fn decompose(
    objective: &str,
    complexity: TaskComplexity,
    limits: &AgentLimits,
) -> Decomposition {
    let children = if complexity.needs_decomposition(limits.decomposition_threshold) {
        let phases = suggest_phases(objective, complexity);
        phases
            .into_iter()
            .enumerate()
            .map(|(i, phase)| DecompositionNode {
                spec: AgentSpec {
                    id: super::new_id(),
                    label: format!("phase-{i}-{}", phase.name),
                    objective: phase.objective,
                    required_files: phase.required_files,
                    relevant_summaries: Vec::new(),
                    constraints: phase.constraints,
                    coding_standards: Vec::new(),
                    acceptance_criteria: phase.acceptance_criteria,
                    model: None,
                    max_depth: limits.max_depth.saturating_sub(1),
                    created_at: Utc::now(),
                },
                children: None,
            })
            .collect()
    } else {
        let spec = AgentSpec {
            id: super::new_id(),
            label: "worker".to_string(),
            objective: objective.to_string(),
            required_files: Vec::new(),
            relevant_summaries: Vec::new(),
            constraints: Vec::new(),
            coding_standards: Vec::new(),
            acceptance_criteria: vec!["Complete the objective".to_string()],
            model: None,
            max_depth: 0,
            created_at: Utc::now(),
        };
        vec![DecompositionNode {
            spec,
            children: None,
        }]
    };

    Decomposition {
        original: objective.to_string(),
        complexity,
        children,
    }
}

/// Signals that make a task *harder* than its raw word/file counts suggest:
/// architectural moves, cross-cutting concerns, or system-wide changes.
/// Each hit raises the estimated complexity.
const COMPLEXITY_RAISERS: &[&str] = &[
    "migrate",
    "migration",
    "integrate",
    "restructure",
    "convert",
    "overhaul",
    "rearchitect",
    "redesign",
    "rewrite",
    "refactor entire",
    "parallel",
    "distributed",
    "end-to-end",
    "pipeline",
    "concurrency",
    "thread-safe",
    "multi-thread",
    "system-wide",
    "replace the",
    "database",
    "schema",
    "month-long",
    "long-running",
    "continuous",
    "autonomous",
    "self-healing",
    "watchdog",
    "recovery",
    "multi-agent",
    "orchestrate",
    "coordinate",
];

/// Signals that make a task *easier*: mechanical edits with no design work.
/// These push the estimate down unless a raiser is also present.
const COMPLEXITY_LOWERERS: &[&str] = &[
    "fix typo",
    "rename",
    "reformat",
    "lint",
    "bump version",
    "update comment",
    "add comment",
    "chore",
    "update readme",
    "add a test",
    "minor fix",
    "small fix",
    "tweak",
    "reorder",
    "update the version",
];

/// Estimate the complexity of a task based on heuristics.
///
/// This is a heuristic estimate; in a full system the LLM would provide it.
/// Here we combine keyword signals (raising/lowering), sentence count, word
/// count, and file count so small mechanical edits are not over-decomposed
/// and cross-cutting changes are not under-decomposed.
pub fn estimate_complexity(objective: &str, file_count: usize) -> TaskComplexity {
    let lower = objective.to_lowercase();
    let words = objective.split_whitespace().count();
    let sentences = count_sentences(&lower);

    let raises = COMPLEXITY_RAISERS
        .iter()
        .filter(|sig| lower.contains(**sig))
        .count();
    let lowers = COMPLEXITY_LOWERERS
        .iter()
        .filter(|sig| lower.contains(**sig))
        .count();

    // A purely mechanical edit is cheap regardless of wording; across many
    // files it is still just bulk application, so it tops out at Low.
    if lowers > 0 && raises == 0 {
        return if file_count > 5 {
            TaskComplexity::Low
        } else {
            TaskComplexity::Trivial
        };
    }

    let mentions_multiple_files = file_count > 5;
    let mentions_system = raises >= 2
        || lower.contains("redesign")
        || lower.contains("rewrite")
        || lower.contains("refactor entire");

    if mentions_multiple_files && mentions_system {
        return TaskComplexity::Extreme;
    }
    if mentions_multiple_files
        || mentions_system
        || (words > 30 && (lower.contains("architecture") || raises > 0))
    {
        return TaskComplexity::High;
    }
    if sentences >= 2 || file_count > 2 || words > 20 || raises > 0 {
        return TaskComplexity::Medium;
    }
    if file_count > 0 || words > 10 {
        return TaskComplexity::Low;
    }
    TaskComplexity::Trivial
}

/// Count sentence boundaries, treating a terminator as a boundary only when it
/// is followed by whitespace (or the end of the string). Splitting on every
/// `.` would count dots inside file names (`README.md`) and versions
/// (`v1.2.3`) as extra sentences and over-classify trivial tasks.
fn count_sentences(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0usize;
    for (i, ch) in chars.iter().enumerate() {
        if !matches!(ch, '.' | ';' | '!' | '?') {
            continue;
        }
        let next = chars.get(i + 1);
        if next.is_none() || next.is_some_and(|c| c.is_whitespace()) {
            count += 1;
        }
    }
    count.max(1)
}

/// A suggested phase for decomposition.
#[derive(Debug, Clone)]
pub struct SuggestedPhase {
    pub name: String,
    pub objective: String,
    pub required_files: Vec<String>,
    pub constraints: Vec<String>,
    pub acceptance_criteria: Vec<String>,
}

/// Suggest phases for an objective, scaled to its estimated complexity.
///
/// Splitting a task spawns extra agents, so the phase plan is complexity-
/// aware: small tasks run as a single phase (no architecture or documentation
/// overhead), while genuinely large tasks get the full pipeline.
pub fn suggest_phases(objective: &str, complexity: TaskComplexity) -> Vec<SuggestedPhase> {
    let lower = objective.to_lowercase();
    let build_task =
        lower.contains("build") || lower.contains("implement") || lower.contains("create");
    let is_long_running = lower.contains("month")
        || lower.contains("long-running")
        || lower.contains("autonomous")
        || lower.contains("continuous")
        || lower.contains("self-healing")
        || lower.contains("watchdog");

    // Small tasks execute directly: splitting into architecture + documentation
    // phases would spawn extra agents for zero benefit (tokens + latency).
    if complexity <= TaskComplexity::Low {
        return vec![focused_implementation_phase(objective)];
    }

    // Long-running autonomous tasks get a comprehensive pipeline.
    if is_long_running && complexity >= TaskComplexity::High {
        return vec![
            SuggestedPhase {
                name: "analysis".into(),
                objective: format!(
                    "Analyze requirements and design resilience strategy for: {objective}"
                ),
                required_files: Vec::new(),
                constraints: vec![
                    "Design for months of continuous operation.".into(),
                    "Identify failure modes and recovery strategies.".into(),
                ],
                acceptance_criteria: vec![
                    "Failure modes documented".into(),
                    "Recovery strategy defined".into(),
                ],
            },
            SuggestedPhase {
                name: "architecture".into(),
                objective: format!("Design the architecture for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Design for resilience and self-healing.".into(),
                    "Include health monitoring and adaptive throttling.".into(),
                ],
                acceptance_criteria: vec!["Architecture document exists".into()],
            },
            SuggestedPhase {
                name: "implementation".into(),
                objective: format!("Implement core functionality for: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Follow the architecture from the previous phase.".into()],
                acceptance_criteria: vec![
                    "Core functionality implemented".into(),
                    "No obvious bugs".into(),
                ],
            },
            SuggestedPhase {
                name: "resilience".into(),
                objective: format!(
                    "Add self-healing, watchdog, and crash recovery for: {objective}"
                ),
                required_files: Vec::new(),
                constraints: vec![
                    "Must handle months of continuous operation.".into(),
                    "Include memory leak detection and resource monitoring.".into(),
                ],
                acceptance_criteria: vec![
                    "Self-healing implemented".into(),
                    "Watchdog configured".into(),
                    "Crash recovery tested".into(),
                ],
            },
            SuggestedPhase {
                name: "testing".into(),
                objective: format!("Write tests for: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Cover edge cases and failure modes.".into()],
                acceptance_criteria: vec!["All tests pass".into()],
            },
            SuggestedPhase {
                name: "documentation".into(),
                objective: format!("Document: {objective}"),
                required_files: Vec::new(),
                constraints: Vec::new(),
                acceptance_criteria: vec!["Documentation is complete and clear".into()],
            },
        ];
    }

    if build_task {
        if complexity == TaskComplexity::Medium {
            // Medium build: implementation plus verification, no architecture.
            return vec![
                SuggestedPhase {
                    name: "implementation".into(),
                    objective: format!("Implement: {objective}"),
                    required_files: Vec::new(),
                    constraints: vec!["Keep the design minimal and modular.".into()],
                    acceptance_criteria: vec![
                        "Core functionality implemented".into(),
                        "No obvious bugs".into(),
                    ],
                },
                SuggestedPhase {
                    name: "testing".into(),
                    objective: format!("Write tests for: {objective}"),
                    required_files: Vec::new(),
                    constraints: vec!["Cover edge cases.".into()],
                    acceptance_criteria: vec!["All tests pass".into()],
                },
            ];
        }
        // High / Extreme: the full pipeline.
        return vec![
            SuggestedPhase {
                name: "architecture".into(),
                objective: format!("Design the architecture for: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Keep the design minimal and modular.".into()],
                acceptance_criteria: vec!["Architecture document exists".into()],
            },
            SuggestedPhase {
                name: "implementation".into(),
                objective: format!("Implement: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Follow the architecture from the previous phase.".into()],
                acceptance_criteria: vec![
                    "Core functionality implemented".into(),
                    "No obvious bugs".into(),
                ],
            },
            SuggestedPhase {
                name: "testing".into(),
                objective: format!("Write tests for: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Cover edge cases.".into()],
                acceptance_criteria: vec!["All tests pass".into()],
            },
            SuggestedPhase {
                name: "documentation".into(),
                objective: format!("Document: {objective}"),
                required_files: Vec::new(),
                constraints: Vec::new(),
                acceptance_criteria: vec!["Documentation is complete and clear".into()],
            },
        ];
    }

    // Non-build work: analysis + implementation, plus verification at high.
    match complexity {
        TaskComplexity::Medium => vec![analysis_phase(objective), implementation_phase(objective)],
        TaskComplexity::High | TaskComplexity::Extreme => vec![
            analysis_phase(objective),
            implementation_phase(objective),
            SuggestedPhase {
                name: "verification".into(),
                objective: format!("Verify and harden: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Exercise failure and edge-case paths.".into()],
                acceptance_criteria: vec!["Behavior verified under realistic conditions".into()],
            },
        ],
        // Trivial/Low never reach here: the small-task early return above runs
        // first. Kept explicit so the invariant is documented rather than a
        // silent catch-all.
        TaskComplexity::Trivial | TaskComplexity::Low => {
            unreachable!("small tasks are handled by the single-phase early return")
        }
    }
}

/// Single-phase plan for small tasks: focused implementation with no
/// architecture or documentation overhead.
fn focused_implementation_phase(objective: &str) -> SuggestedPhase {
    SuggestedPhase {
        name: "implementation".into(),
        objective: format!("Implement: {objective}"),
        required_files: Vec::new(),
        constraints: vec!["Keep the change minimal and focused.".into()],
        acceptance_criteria: vec!["Objective complete".into(), "No obvious bugs".into()],
    }
}

fn analysis_phase(objective: &str) -> SuggestedPhase {
    SuggestedPhase {
        name: "analysis".into(),
        objective: format!("Analyze requirements for: {objective}"),
        required_files: Vec::new(),
        constraints: Vec::new(),
        acceptance_criteria: vec!["Requirements are understood".into()],
    }
}

fn implementation_phase(objective: &str) -> SuggestedPhase {
    SuggestedPhase {
        name: "implementation".into(),
        objective: format!("Implement: {objective}"),
        required_files: Vec::new(),
        constraints: Vec::new(),
        acceptance_criteria: vec!["Objective complete".into()],
    }
}

/// Flatten a decomposition tree into a flat list of leaf agent specs.
pub fn flatten(decomposition: &Decomposition) -> Vec<AgentSpec> {
    fn recurse(nodes: &[DecompositionNode], out: &mut Vec<AgentSpec>) {
        for node in nodes {
            if let Some(children) = &node.children {
                recurse(children, out);
            } else {
                out.push(node.spec.clone());
            }
        }
    }
    let mut out = Vec::new();
    recurse(&decomposition.children, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_trivial() {
        assert_eq!(estimate_complexity("Fix typo", 0), TaskComplexity::Trivial);
    }

    #[test]
    fn test_estimate_mechanical_edit_is_trivial() {
        assert_eq!(
            estimate_complexity("Fix typo in the README and add a small comment", 3),
            TaskComplexity::Trivial
        );
    }

    #[test]
    fn test_estimate_mechanical_across_many_files_is_low() {
        // Bulk mechanical change: no design work, so never above Low.
        assert_eq!(
            estimate_complexity("Rename the public API across the whole codebase", 12),
            TaskComplexity::Low
        );
    }

    #[test]
    fn test_estimate_migration_is_extreme() {
        assert_eq!(
            estimate_complexity(
                "Migrate the database schema and rework the query layer across 30 modules",
                30
            ),
            TaskComplexity::Extreme
        );
    }

    #[test]
    fn test_estimate_version_dots_do_not_inflate_sentence_count() {
        // "v1.2.3" contains dots but is a single sentence; treating each dot
        // as a boundary would over-classify this trivial update as Medium.
        assert_eq!(
            estimate_complexity("Update the login screen to support v1.2.3 of the API", 0),
            TaskComplexity::Trivial
        );
        assert_eq!(count_sentences("update readme.md and bump to v1.2.3"), 1);
        assert_eq!(
            count_sentences("add a retry loop. wire it in. expose a flag."),
            3
        );
    }

    #[test]
    fn test_estimate_multi_sentence_is_medium() {
        assert_eq!(
            estimate_complexity(
                "Add a retry loop. Wire it into the CLI. Then expose a flag.",
                0
            ),
            TaskComplexity::Medium
        );
    }

    #[test]
    fn test_estimate_extreme() {
        assert_eq!(
            estimate_complexity("Rewrite the entire rendering engine and all 10 modules", 10),
            TaskComplexity::Extreme
        );
    }

    #[test]
    fn test_decompose_below_threshold() {
        let limits = AgentLimits::default();
        let dec = decompose("Fix typo", TaskComplexity::Trivial, &limits);
        assert_eq!(dec.children.len(), 1);
        assert!(dec.children[0].children.is_none());
    }

    #[test]
    fn test_decompose_above_threshold() {
        let limits = AgentLimits {
            decomposition_threshold: TaskComplexity::Medium,
            ..Default::default()
        };
        let dec = decompose("Build a web server", TaskComplexity::Extreme, &limits);
        assert!(dec.children.len() > 1);
        // All children should have been given agent specs
        for child in &dec.children {
            assert!(!child.spec.label.is_empty());
        }
    }

    #[test]
    fn test_suggest_phases_small_task_is_single_phase() {
        // Small tasks must not spawn architecture/documentation agents.
        let phases = suggest_phases("Add a settings toggle", TaskComplexity::Low);
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].name, "implementation");
        assert!(!phases.iter().any(|p| p.name == "documentation"));
    }

    #[test]
    fn test_suggest_phases_medium_build_is_two_phases() {
        let phases = suggest_phases("Implement a webhook retry queue", TaskComplexity::Medium);
        assert_eq!(phases.len(), 2);
        assert!(phases.iter().any(|p| p.name == "testing"));
        assert!(!phases.iter().any(|p| p.name == "architecture"));
    }

    #[test]
    fn test_suggest_phases_high_non_build_gets_verification() {
        let phases = suggest_phases(
            "Optimize the render pipeline for large scenes",
            TaskComplexity::High,
        );
        assert!(phases.iter().any(|p| p.name == "analysis"));
        assert!(phases.iter().any(|p| p.name == "verification"));
    }

    #[test]
    fn test_suggest_phases_build() {
        let phases = suggest_phases("Build a browser", TaskComplexity::Extreme);
        assert_eq!(phases.len(), 4);
        assert!(phases.iter().any(|p| p.name == "architecture"));
        assert!(phases.iter().any(|p| p.name == "implementation"));
    }

    #[test]
    fn test_flatten() {
        let limits = AgentLimits::default();
        let dec = decompose("Build application", TaskComplexity::High, &limits);
        let flat = flatten(&dec);
        assert!(!flat.is_empty());
        assert!(flat.iter().all(|s| !s.objective.is_empty()));
    }
}
