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