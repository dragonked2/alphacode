//! Bundled skills that ship inside the compiled alphacode binary.
//!
//! Each bundled skill lives as a directory tree of `SKILL.md` files
//! committed under `bundled_skills/<skill>/...`. The top-level
//! `<skill>/SKILL.md` defines the skill itself; nested subskill
//! `SKILL.md` files are merged into the parent as named references
//! (e.g. `hunt-sqli.md`, `recon.md`) so they are available contextually
//! when the skill is invoked, but they are not exposed as separate
//! slash commands. This keeps the slash-command surface focused on the
//! parent skill while preserving the detailed methodology content.
//!
//! Skills are embedded at compile time via [`include_str!`] so they are
//! always available regardless of working directory, `$HOME`, or any
//! on-disk install. Users can still override or shadow these with
//! skills placed in `~/.alphacode/skills/` or `./.alphacode/skills/` —
//! the project-local overlay always wins, mirroring the existing load
//! order.
//!
//! To add a new bundled skill:
//!  1. Create the directory tree under `bundled_skills/<name>/...`.
//!  2. Add an entry below in [`bundled_skills`] with the top-level
//!     `SKILL.md` body and a list of `(name, body)` reference pairs.

/// Top-level bundled skills: `(skill_name, top_level_body, references)`.
///
/// `references` are merged into the skill's `reference_files` map keyed
/// by the subskill directory name (e.g. `hunt-sqli`).
pub(super) const BUNDLED_SKILLS: &[BundledSkill] = &[
    BundledSkill {
        name: "bugbounty",
        body: include_str!("bundled_skills/bugbounty/SKILL.md"),
        references: &[
            (
                "advanced-techniques",
                include_str!("bundled_skills/bugbounty/advanced-techniques/SKILL.md"),
            ),
            (
                "client-reverse",
                include_str!("bundled_skills/bugbounty/client-reverse/SKILL.md"),
            ),
            (
                "credential-attack",
                include_str!("bundled_skills/bugbounty/credential-attack/SKILL.md"),
            ),
            (
                "hunt-api",
                include_str!("bundled_skills/bugbounty/hunt-api/SKILL.md"),
            ),
            (
                "hunt-graphql",
                include_str!("bundled_skills/bugbounty/hunt-graphql/SKILL.md"),
            ),
            (
                "hunt-idor",
                include_str!("bundled_skills/bugbounty/hunt-idor/SKILL.md"),
            ),
            (
                "hunt-memory",
                include_str!("bundled_skills/bugbounty/hunt-memory/SKILL.md"),
            ),
            (
                "hunt-oauth",
                include_str!("bundled_skills/bugbounty/hunt-oauth/SKILL.md"),
            ),
            (
                "hunt-sqli",
                include_str!("bundled_skills/bugbounty/hunt-sqli/SKILL.md"),
            ),
            (
                "hunt-ssrf",
                include_str!("bundled_skills/bugbounty/hunt-ssrf/SKILL.md"),
            ),
            (
                "hunt-xss",
                include_str!("bundled_skills/bugbounty/hunt-xss/SKILL.md"),
            ),
            (
                "llm-redteam",
                include_str!("bundled_skills/bugbounty/llm-redteam/SKILL.md"),
            ),
            (
                "recon",
                include_str!("bundled_skills/bugbounty/recon/SKILL.md"),
            ),
            (
                "report",
                include_str!("bundled_skills/bugbounty/report/SKILL.md"),
            ),
            (
                "security-arsenal",
                include_str!("bundled_skills/bugbounty/security-arsenal/SKILL.md"),
            ),
            (
                "web3-audit",
                include_str!("bundled_skills/bugbounty/web3-audit/SKILL.md"),
            ),
            (
                "pentest-ops",
                include_str!("bundled_skills/bugbounty/pentest-ops/SKILL.md"),
            ),
            (
                "knowledge-broker",
                include_str!("bundled_skills/bugbounty/knowledge-broker/SKILL.md"),
            ),
            (
                "findings-lifecycle",
                include_str!("bundled_skills/bugbounty/findings-lifecycle/SKILL.md"),
            ),
            (
                "evidence-locker",
                include_str!("bundled_skills/bugbounty/evidence-locker/SKILL.md"),
            ),
            (
                "tool-doctor",
                include_str!("bundled_skills/bugbounty/tool-doctor/SKILL.md"),
            ),
            (
                "runbook",
                include_str!("bundled_skills/bugbounty/runbook/SKILL.md"),
            ),
            (
                "network-cloud-triage",
                include_str!("bundled_skills/bugbounty/network-cloud-triage/SKILL.md"),
            ),
            (
                "redteam-ops",
                include_str!("bundled_skills/bugbounty/redteam-ops/SKILL.md"),
            ),
            (
                "hunt-cors",
                include_str!("bundled_skills/bugbounty/hunt-cors/SKILL.md"),
            ),
            (
                "hunt-ssrf-advanced",
                include_str!("bundled_skills/bugbounty/hunt-ssrf-advanced/SKILL.md"),
            ),
            (
                "hunt-race",
                include_str!("bundled_skills/bugbounty/hunt-race/SKILL.md"),
            ),
            (
                "hunt-desync",
                include_str!("bundled_skills/bugbounty/hunt-desync/SKILL.md"),
            ),
            (
                "llm-injection",
                include_str!("bundled_skills/bugbounty/llm-injection/SKILL.md"),
            ),
        ],
    },
    BundledSkill {
        name: "optimization",
        body: include_str!("bundled_skills/optimization/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "security-audit",
        body: include_str!("bundled_skills/security-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "refactor",
        body: include_str!("bundled_skills/refactor/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "test-writer",
        body: include_str!("bundled_skills/test-writer/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "api-design",
        body: include_str!("bundled_skills/api-design/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "dependency-audit",
        body: include_str!("bundled_skills/dependency-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "architecture-review",
        body: include_str!("bundled_skills/architecture-review/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "performance-profiling",
        body: include_str!("bundled_skills/performance-profiling/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "incident-response",
        body: include_str!("bundled_skills/incident-response/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "data-pipeline",
        body: include_str!("bundled_skills/data-pipeline/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "migration-planner",
        body: include_str!("bundled_skills/migration-planner/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "observability",
        body: include_str!("bundled_skills/observability/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "vuln-hunter",
        body: include_str!("bundled_skills/vuln-hunter/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "bug-bounty-methodology",
        body: include_str!("bundled_skills/bug-bounty-methodology/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "api-security",
        body: include_str!("bundled_skills/api-security/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "crypto-audit",
        body: include_str!("bundled_skills/crypto-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "cloud-security",
        body: include_str!("bundled_skills/cloud-security/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "code-audit",
        body: include_str!("bundled_skills/code-audit/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "exploit-dev",
        body: include_str!("bundled_skills/exploit-dev/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "frontend-design",
        body: include_str!("bundled_skills/frontend-design/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "git-workflow",
        body: include_str!("bundled_skills/git-workflow/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "docker",
        body: include_str!("bundled_skills/docker/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "database",
        body: include_str!("bundled_skills/database/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "code-review",
        body: include_str!("bundled_skills/code-review/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "debug",
        body: include_str!("bundled_skills/debug/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "documentation",
        body: include_str!("bundled_skills/documentation/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "devops",
        body: include_str!("bundled_skills/devops/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "regex",
        body: include_str!("bundled_skills/regex/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "testing",
        body: include_str!("bundled_skills/testing/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "api-builder",
        body: include_str!("bundled_skills/api-builder/SKILL.md"),
        references: &[],
    },
    BundledSkill {
        name: "ctf",
        body: include_str!("bundled_skills/ctf/SKILL.md"),
        references: &[
            ("web", include_str!("bundled_skills/ctf/ctf/web/SKILL.md")),
            (
                "crypto",
                include_str!("bundled_skills/ctf/ctf/crypto/SKILL.md"),
            ),
            ("pwn", include_str!("bundled_skills/ctf/ctf/pwn/SKILL.md")),
            ("rev", include_str!("bundled_skills/ctf/ctf/rev/SKILL.md")),
            (
                "forensics",
                include_str!("bundled_skills/ctf/ctf/forensics/SKILL.md"),
            ),
            ("misc", include_str!("bundled_skills/ctf/ctf/misc/SKILL.md")),
            (
                "methodology",
                include_str!("bundled_skills/ctf/ctf/methodology/SKILL.md"),
            ),
            (
                "toolkit",
                include_str!("bundled_skills/ctf/ctf/toolkit/SKILL.md"),
            ),
            ("dfir", include_str!("bundled_skills/ctf/ctf/dfir/SKILL.md")),
            (
                "malware-analysis",
                include_str!("bundled_skills/ctf/ctf/malware-analysis/SKILL.md"),
            ),
            ("siem", include_str!("bundled_skills/ctf/ctf/siem/SKILL.md")),
            (
                "advanced-reasoning",
                include_str!("bundled_skills/ctf/ctf/advanced-reasoning/SKILL.md"),
            ),
            (
                "adversarial-thinking",
                include_str!("bundled_skills/ctf/ctf/adversarial-thinking/SKILL.md"),
            ),
            (
                "solution-verifier",
                include_str!("bundled_skills/ctf/ctf/solution-verifier/SKILL.md"),
            ),
        ],
    },
    BundledSkill {
        name: "frontend-dev",
        body: include_str!("bundled_skills/frontend-dev/SKILL.md"),
        references: &[
            (
                "react",
                include_str!("bundled_skills/frontend-dev/react/SKILL.md"),
            ),
            (
                "nextjs",
                include_str!("bundled_skills/frontend-dev/nextjs/SKILL.md"),
            ),
            (
                "tailwind",
                include_str!("bundled_skills/frontend-dev/tailwind/SKILL.md"),
            ),
            (
                "components",
                include_str!("bundled_skills/frontend-dev/components/SKILL.md"),
            ),
            (
                "performance",
                include_str!("bundled_skills/frontend-dev/performance/SKILL.md"),
            ),
            (
                "testing",
                include_str!("bundled_skills/frontend-dev/testing/SKILL.md"),
            ),
        ],
    },
    BundledSkill {
        name: "backend-dev",
        body: include_str!("bundled_skills/backend-dev/SKILL.md"),
        references: &[
            (
                "nodejs",
                include_str!("bundled_skills/backend-dev/nodejs/SKILL.md"),
            ),
            (
                "python",
                include_str!("bundled_skills/backend-dev/python/SKILL.md"),
            ),
            (
                "rust",
                include_str!("bundled_skills/backend-dev/rust/SKILL.md"),
            ),
            (
                "database",
                include_str!("bundled_skills/backend-dev/database/SKILL.md"),
            ),
            (
                "auth",
                include_str!("bundled_skills/backend-dev/auth/SKILL.md"),
            ),
            (
                "caching",
                include_str!("bundled_skills/backend-dev/caching/SKILL.md"),
            ),
        ],
    },
];

/// One bundled skill: a name, the top-level SKILL.md body, and any
/// nested subskill references that should be merged into the skill's
/// `reference_files`.
pub(super) struct BundledSkill {
    pub name: &'static str,
    pub body: &'static str,
    pub references: &'static [(&'static str, &'static str)],
}

/// Number of bundled top-level skills (parent skills only — subskill
/// references are part of the parent).
pub fn bundled_skill_count() -> usize {
    BUNDLED_SKILLS.len()
}

/// Public list of bundled skill names. Excludes subskill references so
/// callers (e.g. `/skills` introspection) only see parent skills.
pub fn bundled_skill_names() -> Vec<&'static str> {
    BUNDLED_SKILLS.iter().map(|s| s.name).collect()
}
