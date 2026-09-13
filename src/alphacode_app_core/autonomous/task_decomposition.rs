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
    // Modern infrastructure keywords
    "microservices",
    "kubernetes",
    "docker",
    "ci/cd",
    "terraform",
    "ansible",
    "aws",
    "gcp",
    "azure",
    // Cloud & orchestration
    "serverless",
    "lambda",
    "cloudformation",
    "helm",
    "istio",
    "service mesh",
    "load balancer",
    "cdn",
    "dns",
    // Frontend frameworks & libraries
    "react",
    "vue",
    "angular",
    "next.js",
    "nuxt",
    "svelte",
    "remix",
    "solidjs",
    // Backend & API layers
    "graphql",
    "grpc",
    "protobuf",
    "webhook",
    "rest api",
    // Data & messaging
    "kafka",
    "rabbitmq",
    "redis",
    "elasticsearch",
    "postgresql",
    "mongodb",
    "cassandra",
    "dynamodb",
    "kinesis",
    "pubsub",
    "event-driven",
    "event sourcing",
    "cqrs",
    "data pipeline",
    "etl",
    // Security & auth
    "security",
    "auth",
    "authentication",
    "authorization",
    "oauth",
    "jwt",
    "rbac",
    "encrypt",
    "decrypt",
    "certificate",
    "tls",
    "ssl",
    "vulnerability",
    "penetration",
    "owasp",
    "xss",
    "csrf",
    "injection",
    "sql injection",
    // Systems & infra
    "kernel",
    "driver",
    "firmware",
    "bootloader",
    "memory management",
    "syscall",
    "interrupt",
    "heap",
    "allocator",
    "garbage collector",
    // Performance & scale
    "latency",
    "throughput",
    "benchmark",
    "profiling",
    "optimize",
    "performance",
    "scale",
    "horizontal scaling",
    "vertical scaling",
    // Data migration & transformation
    "data migration",
    "backfill",
    "backfill data",
    "schema change",
    "data transformation",
    "normalize",
    "denormalize",
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
    // Dependency & build management
    "update dependency",
    "update dependencies",
    "bump dependency",
    "add dependency",
    "remove dependency",
    "add dev dependency",
    // Import & code hygiene
    "add import",
    "remove import",
    "remove unused",
    "clean up import",
    "sort import",
    "add to import",
    // Formatting & style
    "format code",
    "run formatter",
    "fix whitespace",
    "fix indentation",
    "remove trailing whitespace",
    "fix line ending",
    "convert tabs to spaces",
    "convert spaces to tabs",
    // Config & metadata
    "update config",
    "change config",
    "edit config",
    "add config",
    "update metadata",
    "update package.json",
    "update Cargo.toml",
    "update go.mod",
    "update requirements.txt",
    // Documentation
    "update docs",
    "fix docs",
    "add example",
    "update example",
    "fix example",
    "add section",
    // CI/CD & tooling
    "update workflow",
    "add workflow",
    "fix workflow",
    "update script",
    "fix script",
    // Test maintenance
    "fix test",
    "update test",
    "add test case",
    "update snapshot",
    "update fixture",
    "update mock",
    // Refactoring (pure mechanical, no design)
    "extract method",
    "extract function",
    "inline function",
    "move function",
    "move file",
    "extract module",
];

/// Detect if the objective involves security-sensitive work.
fn has_security_signal(objective: &str) -> bool {
    const SECURITY_KEYWORDS: &[&str] = &[
        "auth",
        "oauth",
        "jwt",
        "rbac",
        "encrypt",
        "decrypt",
        "certificate",
        "tls",
        "ssl",
        "vulnerability",
        "penetration",
        "owasp",
        "xss",
        "csrf",
        "injection",
        "sql injection",
        "security",
        "permission",
        "access control",
        "secret",
        "token",
        "credential",
        "password",
        "hashing",
        "salt",
        "sandbox",
        "privilege",
        "firewall",
    ];
    SECURITY_KEYWORDS
        .iter()
        .any(|kw| objective.to_lowercase().contains(kw))
}

/// Detect if the objective involves external service integrations.
fn has_external_integration(objective: &str) -> bool {
    const INTEGRATION_KEYWORDS: &[&str] = &[
        "api",
        "webhook",
        "third-party",
        "external",
        "third party",
        "partner",
        "integration",
        "connect to",
        "talk to",
        "communicate with",
        "send to",
        "receive from",
        "subscribe to",
        "publish to",
        "kafka",
        "rabbitmq",
        "redis",
        "elasticsearch",
        "database",
        "microservice",
        "service",
    ];
    INTEGRATION_KEYWORDS
        .iter()
        .any(|kw| objective.to_lowercase().contains(kw))
}

/// Detect if the objective involves data migration.
fn has_data_migration(objective: &str) -> bool {
    const MIGRATION_KEYWORDS: &[&str] = &[
        "migrate",
        "migration",
        "backfill",
        "data migration",
        "schema change",
        "schema migration",
        "data transformation",
        "normalize",
        "denormalize",
        "data model",
        "data layer",
        "etl",
        "data pipeline",
    ];
    MIGRATION_KEYWORDS
        .iter()
        .any(|kw| objective.to_lowercase().contains(kw))
}

/// Compute a penalty factor based on file count.
///
/// Even a simple task becomes non-trivial when applied across many files.
/// The penalty is sub-linear: going from 0→20 files adds a lot of weight,
/// but 20→40 adds less.
fn file_count_penalty(file_count: usize) -> f64 {
    if file_count == 0 {
        return 0.0;
    }
    // Logarithmic scaling: 1 file → 0.3, 5 → 0.7, 10 → 1.0, 20 → 1.3, 50 → 1.7
    (file_count as f64).ln() * 0.3 + 0.3
}

/// Estimate the complexity of a task based on heuristics.
///
/// This is a heuristic estimate; in a full system the LLM would provide it.
/// Here we combine keyword signals (raising/lowering), sentence count, word
/// count, file count, and domain-specific signals so small mechanical edits
/// are not over-decomposed and cross-cutting changes are not under-decomposed.
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

    // Compute domain-specific signals.
    let is_security = has_security_signal(&lower);
    let is_integration = has_external_integration(&lower);
    let is_migration = has_data_migration(&lower);

    let mentions_multiple_files = file_count > 5;
    let mentions_many_files = file_count > 15;
    let mentions_system = raises >= 2
        || lower.contains("redesign")
        || lower.contains("rewrite")
        || lower.contains("refactor entire");

    // New feature vs modification: "build", "create", "implement" suggest
    // new feature which is inherently harder than modifying existing code.
    let is_new_feature = lower.contains("build")
        || lower.contains("create")
        || lower.contains("implement")
        || lower.contains("add ");

    // Heuristic score: sum of raisers, sentence count, word weight, and
    // domain signals to determine complexity level.
    let domain_bonus = (is_security as u8 + is_integration as u8 + is_migration as u8) as usize;
    let feature_bonus = if is_new_feature { 1 } else { 0 };
    let file_penalty = file_count_penalty(file_count) as usize;

    let score = raises
        + (sentences.saturating_sub(1))
        + ((words / 15).min(4))
        + file_penalty
        + domain_bonus
        + feature_bonus;

    // Extreme: system-wide with many files, or very high signal density.
    if (mentions_many_files && mentions_system)
        || (raises >= 4 && mentions_multiple_files)
        || (score >= 10 && is_migration)
    {
        return TaskComplexity::Extreme;
    }

    // High: multiple files, or system-level work, or domain-heavy with
    // significant word/sentence density.
    if mentions_multiple_files
        || mentions_system
        || (words > 30 && (lower.contains("architecture") || raises > 0))
        || (score >= 7 && (is_security || is_integration))
        || (raises >= 2 && is_migration)
    {
        return TaskComplexity::High;
    }

    // Medium: moderate signals — a few sentences, a few files, or any
    // raiser present.
    if sentences >= 2 || file_count > 2 || words > 20 || raises > 0 {
        return TaskComplexity::Medium;
    }

    // Low: touches some files or has meaningful word count.
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
    let is_security = has_security_signal(&lower);
    let is_migration = has_data_migration(&lower);
    let is_integration = has_external_integration(&lower);

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

    // Security-sensitive tasks: require a dedicated security review phase.
    if is_security && complexity >= TaskComplexity::High {
        let mut phases = vec![
            SuggestedPhase {
                name: "architecture".into(),
                objective: format!("Design secure architecture for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Follow least-privilege and defense-in-depth principles.".into(),
                    "Identify attack surface and threat model.".into(),
                ],
                acceptance_criteria: vec![
                    "Threat model documented".into(),
                    "Security architecture approved".into(),
                ],
            },
            SuggestedPhase {
                name: "implementation".into(),
                objective: format!("Implement with security controls for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Use constant-time comparisons for secrets.".into(),
                    "Validate all inputs.".into(),
                    "Never log sensitive data.".into(),
                ],
                acceptance_criteria: vec![
                    "Security controls implemented".into(),
                    "No obvious vulnerabilities".into(),
                ],
            },
            SuggestedPhase {
                name: "security-review".into(),
                objective: format!("Security review and hardening for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Check for OWASP Top 10 vulnerabilities.".into(),
                    "Verify authentication and authorization logic.".into(),
                    "Check for injection, XSS, and CSRF vulnerabilities.".into(),
                ],
                acceptance_criteria: vec![
                    "No critical security issues found".into(),
                    "Security checklist completed".into(),
                ],
            },
            SuggestedPhase {
                name: "testing".into(),
                objective: format!("Security-focused testing for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Test authentication bypass attempts.".into(),
                    "Test authorization edge cases.".into(),
                    "Test input validation with malicious payloads.".into(),
                ],
                acceptance_criteria: vec![
                    "All security tests pass".into(),
                    "Penetration test scenarios covered".into(),
                ],
            },
        ];
        if complexity >= TaskComplexity::Extreme {
            phases.push(SuggestedPhase {
                name: "documentation".into(),
                objective: format!("Document security considerations for: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Document all security decisions and trade-offs.".into()],
                acceptance_criteria: vec!["Security documentation complete".into()],
            });
        }
        return phases;
    }

    // Data migration tasks: need careful planning and rollback strategy.
    if is_migration && complexity >= TaskComplexity::High {
        return vec![
            SuggestedPhase {
                name: "analysis".into(),
                objective: format!("Analyze data migration requirements for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Map source and target schemas.".into(),
                    "Identify data transformation rules.".into(),
                    "Plan rollback strategy.".into(),
                ],
                acceptance_criteria: vec![
                    "Migration plan documented".into(),
                    "Rollback strategy defined".into(),
                ],
            },
            SuggestedPhase {
                name: "schema-design".into(),
                objective: format!("Design target schema for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Ensure backward compatibility.".into(),
                    "Consider data integrity constraints.".into(),
                ],
                acceptance_criteria: vec!["Target schema approved".into()],
            },
            SuggestedPhase {
                name: "implementation".into(),
                objective: format!("Implement migration scripts for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Scripts must be idempotent.".into(),
                    "Include data validation checks.".into(),
                    "Support partial rollback.".into(),
                ],
                acceptance_criteria: vec![
                    "Migration scripts complete".into(),
                    "Rollback scripts complete".into(),
                ],
            },
            SuggestedPhase {
                name: "testing".into(),
                objective: format!("Test migration with production-like data for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Test with full production data volume.".into(),
                    "Verify data integrity after migration.".into(),
                    "Test rollback procedure.".into(),
                ],
                acceptance_criteria: vec![
                    "Migration verified on staging".into(),
                    "Data integrity confirmed".into(),
                ],
            },
            SuggestedPhase {
                name: "documentation".into(),
                objective: format!("Document migration procedure for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Document runbook for operations team.".into(),
                    "Include monitoring and alerting setup.".into(),
                ],
                acceptance_criteria: vec!["Migration runbook complete".into()],
            },
        ];
    }

    // Integration-heavy tasks: need design, implementation, and contract
    // testing phases.
    if is_integration && complexity >= TaskComplexity::High {
        let mut phases = vec![
            SuggestedPhase {
                name: "architecture".into(),
                objective: format!("Design integration architecture for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Define API contracts and data formats.".into(),
                    "Plan retry and circuit-breaker strategies.".into(),
                ],
                acceptance_criteria: vec![
                    "Integration contracts documented".into(),
                    "Error handling strategy defined".into(),
                ],
            },
            SuggestedPhase {
                name: "implementation".into(),
                objective: format!("Implement integrations for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Follow integration contracts.".into(),
                    "Add structured logging for debugging.".into(),
                ],
                acceptance_criteria: vec![
                    "Integrations implemented".into(),
                    "Error handling in place".into(),
                ],
            },
            SuggestedPhase {
                name: "contract-testing".into(),
                objective: format!("Contract and integration testing for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Test with mock external services.".into(),
                    "Verify contract compliance.".into(),
                ],
                acceptance_criteria: vec![
                    "All contract tests pass".into(),
                    "Integration tests pass".into(),
                ],
            },
            SuggestedPhase {
                name: "verification".into(),
                objective: format!("Verify end-to-end flows for: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Exercise failure and edge-case paths.".into()],
                acceptance_criteria: vec!["Behavior verified under realistic conditions".into()],
            },
        ];
        if complexity >= TaskComplexity::Extreme {
            phases.push(SuggestedPhase {
                name: "documentation".into(),
                objective: format!("Document integration for: {objective}"),
                required_files: Vec::new(),
                constraints: Vec::new(),
                acceptance_criteria: vec!["Integration documentation complete".into()],
            });
        }
        return phases;
    }

    // Web application build: frontend + backend + testing.
    let is_web_app =
        (lower.contains("web") || lower.contains("frontend") || lower.contains("backend"))
            && build_task;
    if is_web_app && complexity >= TaskComplexity::High {
        return vec![
            SuggestedPhase {
                name: "architecture".into(),
                objective: format!("Design web application architecture for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Define API and component boundaries.".into(),
                    "Plan state management strategy.".into(),
                ],
                acceptance_criteria: vec!["Architecture document exists".into()],
            },
            SuggestedPhase {
                name: "backend".into(),
                objective: format!("Implement backend/API for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Follow the architecture from the previous phase.".into(),
                    "Include error handling and validation.".into(),
                ],
                acceptance_criteria: vec![
                    "Backend implemented".into(),
                    "API endpoints working".into(),
                ],
            },
            SuggestedPhase {
                name: "frontend".into(),
                objective: format!("Implement frontend/UI for: {objective}"),
                required_files: Vec::new(),
                constraints: vec![
                    "Use the API contracts from the backend phase.".into(),
                    "Follow component architecture.".into(),
                ],
                acceptance_criteria: vec![
                    "Frontend implemented".into(),
                    "UI matches design".into(),
                ],
            },
            SuggestedPhase {
                name: "testing".into(),
                objective: format!("End-to-end testing for: {objective}"),
                required_files: Vec::new(),
                constraints: vec!["Cover critical user journeys.".into()],
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

    #[test]
    fn test_security_task_gets_security_review_phase() {
        let phases = suggest_phases(
            "Implement OAuth2 authentication with JWT tokens",
            TaskComplexity::High,
        );
        assert!(phases.iter().any(|p| p.name == "security-review"));
        assert!(phases.iter().any(|p| p.name == "architecture"));
        assert!(phases.iter().any(|p| p.name == "testing"));
    }

    #[test]
    fn test_migration_task_gets_rollback_phases() {
        let phases = suggest_phases(
            "Migrate user data from MongoDB to PostgreSQL across 20 tables",
            TaskComplexity::Extreme,
        );
        assert!(phases.iter().any(|p| p.name == "analysis"));
        assert!(phases.iter().any(|p| p.name == "schema-design"));
        assert!(phases.iter().any(|p| p.name == "testing"));
        assert!(phases.iter().any(|p| p.name == "documentation"));
    }

    #[test]
    fn test_integration_task_gets_contract_testing() {
        let phases = suggest_phases(
            "Integrate with Kafka for event-driven microservices communication",
            TaskComplexity::High,
        );
        assert!(phases.iter().any(|p| p.name == "architecture"));
        assert!(phases.iter().any(|p| p.name == "contract-testing"));
    }

    #[test]
    fn test_web_app_gets_frontend_backend_phases() {
        let phases = suggest_phases(
            "Build a React frontend with a Node.js backend",
            TaskComplexity::High,
        );
        assert!(phases.iter().any(|p| p.name == "architecture"));
        assert!(phases.iter().any(|p| p.name == "backend"));
        assert!(phases.iter().any(|p| p.name == "frontend"));
    }

    #[test]
    fn test_mechanical_lowerers_with_modern_keywords() {
        // Update dependency is still mechanical even with "docker" mentioned
        // if "docker" appears only in context of "update docker dependency".
        assert_eq!(
            estimate_complexity("Update docker dependency version", 2),
            TaskComplexity::Trivial
        );
    }

    #[test]
    fn test_complexity_raisers_modern_infra() {
        // Microservices + kubernetes + multi-file should be high.
        assert_eq!(
            estimate_complexity(
                "Deploy microservices to kubernetes cluster with service mesh",
                20
            ),
            TaskComplexity::Extreme
        );
    }
}
