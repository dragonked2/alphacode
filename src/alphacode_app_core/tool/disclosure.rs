//! Progressive tool disclosure: decide which tool *definitions* are advertised
//! to the model on a given request.
//!
//! # Why this exists
//!
//! Measured on the real registry (81 tools), the static prompt prefix sent on
//! every request was ~61.5k tokens: 26.6k of that was tool input schemas, 9.5k
//! tool descriptions, 22.8k the always-on system prompt. Paying that on every
//! turn is waste — the model cannot read 81 schemas at once, and most of them
//! are irrelevant to the turn at hand (a Rust build needs neither `sqlmap` nor
//! `volatility`).
//!
//! Crucially, [`Registry`](crate::tool::Registry) already separates
//! *execution* from *advertisement*: `self.tools` holds every tool and stays
//! callable, while `definitions()` returns the prompt-visible subset. So we can
//! advertise a small tier and still execute anything the session has enabled.
//!
//! # Tiers
//!
//! * [`ToolTier::Core`] — everything needed to read, edit, run, search, fetch,
//!   plan, and ask the user. Always advertised.
//! * [`ToolTier::Standard`] — core plus the general-purpose surface a coding or
//!   research turn plausibly needs (browser, httpflow, memory, skills, swarm,
//!   scheduling, self-dev). Advertised by default.
//! * [`ToolTier::Extended`] — specialized families (recon/offensive binaries,
//!   CTF/forensics, media, cloud). Promoted on demand.
//!
//! Promotion is sticky per session: once a tool has been advertised it stays
//! advertised, so the model's learned call shape never rots mid-task.

use std::collections::HashSet;

use crate::alphacode_message_types::ToolDefinition;

/// Advertisement tier for a tool definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolTier {
    /// Always advertised: the minimal surface needed to do any work.
    Core,
    /// Advertised by default: general-purpose productivity surface.
    Standard,
    /// Promoted on demand: specialized families.
    Extended,
}

impl ToolTier {
    /// Whether this tier is advertised without an explicit promotion.
    pub fn is_default_visible(self, policy: DisclosurePolicy) -> bool {
        match policy {
            // `Legacy` reproduces pre-disclosure behaviour: advertise everything.
            DisclosurePolicy::Legacy => true,
            DisclosurePolicy::Progressive => match self {
                ToolTier::Core | ToolTier::Standard => true,
                ToolTier::Extended => false,
            },
        }
    }
}

/// How aggressively to hide tool definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisclosurePolicy {
    /// Advertise every enabled tool on every request (old behaviour, opt-in).
    Legacy,
    /// Advertise `Core` + `Standard`; promote `Extended` on demand.
    #[default]
    Progressive,
}

impl DisclosurePolicy {
    /// Parse a config/env value. Unknown values fall back to `Progressive`.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "legacy" | "off" | "all" | "full" | "0" | "false" | "no" => Self::Legacy,
            _ => Self::Progressive,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Progressive => "progressive",
        }
    }
}

/// Tier for a tool by name.
///
/// Unknown tools (MCP servers, plugins, future additions) default to
/// [`ToolTier::Standard`]: a tool we do not recognise should never be hidden by
/// accident, because hiding it would silently remove a capability the user
/// installed.
pub fn tier_for(name: &str) -> ToolTier {
    // External namespaces have their own semantics and must not be hidden just
    // because a leaf happens to match a specialized built-in tool.
    if name.contains("__") {
        return ToolTier::Standard;
    }
    let leaf = name;

    // --- Core: the day-to-day surface. Read, write, run, search, fetch, plan,
    // remember, and ask. This is what a coding or research turn actually uses;
    // everything else is one `tool_help` call away and costs nothing until then.
    const CORE: &[&str] = &[
        "read",
        "write",
        "edit",
        "apply_patch",
        "bash",
        "alphagrep",
        "grep",
        "file_grep",
        "ls",
        "glob",
        "webfetch",
        "websearch",
        "todo",
        "skill_manage",
        "memory",
        "tool_help",
    ];

    // --- Extended: everything else. Specialized families, plus the tools that
    // are useful but not needed on a typical turn (editing variants, background
    // work, scheduling, coordination, discovery marketplace).
    const EXTENDED: &[&str] = &[
        "ctf_web_recon",
        // Redundant editing variants — `edit`/`apply_patch` cover the same work.
        "multiedit",
        "patch",
        // Orchestration and background surfaces.
        "plan",
        "invalid",
        "swarm",
        "batch",
        "bg",
        "cron",
        "schedule",
        "initiative",
        "selfdev",
        "self_improve",
        "conversation_search",
        "session_search",
        "side_panel",
        "doctor",
        // Desktop / OS integration.
        "open",
        "clipboard",
        "gmail",
        // Recon / offensive binaries.
        "subfinder",
        "dnsx",
        "assetfinder",
        "amass",
        "httpx",
        "httprobe",
        "gau",
        "waybackurls",
        "katana",
        "hakrawler",
        "gospider",
        "cariddi",
        "meg",
        "ffuf",
        "feroxbuster",
        "gobuster",
        "gf",
        "qsreplace",
        "anew",
        "kxss",
        "dalfox",
        "corsy",
        "crlfuzz",
        "sqlmap",
        "nikto",
        "nuclei",
        "nmap",
        "naabu",
        "httpflow",
        "scrapling",
        "unfurl",
        "jwt",
        // CTF / forensics / reverse engineering.
        "pwntools",
        "binwalk",
        "steghide",
        "radare2",
        "ghidra",
        "angr",
        "z3",
        "exiftool",
        "volatility",
        "tshark",
        "hashcat",
        "john",
        "flag_scanner",
        "challenge_classifier",
        "ctf_auto_solver",
        "browser",
        "desktop",
        "macos_computer_use",
        "discover_tools",
    ];

    if CORE.iter().any(|t| t.eq_ignore_ascii_case(leaf)) {
        return ToolTier::Core;
    }
    if EXTENDED.iter().any(|t| t.eq_ignore_ascii_case(leaf)) {
        return ToolTier::Extended;
    }
    // Unrecognised (MCP, plugins, new built-ins): keep visible.
    ToolTier::Standard
}

/// Which tool definitions to advertise for one request.
#[derive(Debug, Clone, Default)]
pub struct DisclosureSelection {
    /// Tool names explicitly promoted in this session.
    promoted: HashSet<String>,
}

impl DisclosureSelection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a promotion. Returns `true` when this is a new promotion (i.e. the
    /// advertised set actually grows).
    pub fn promote(&mut self, name: &str) -> bool {
        self.promoted.insert(name.to_string())
    }

    /// Promote a whole family by explicit name list.
    pub fn promote_all<I, S>(&mut self, names: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        names
            .into_iter()
            .filter(|n| self.promoted.insert(n.as_ref().to_string()))
            .count()
    }

    /// Promote every tool that matches a case-insensitive substring query, plus
    /// (for a family query like "recon") every tool whose name contains the
    /// query. Returns the names newly promoted.
    pub fn promote_matching(&mut self, query: &str, candidates: &[String]) -> Vec<String> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut promoted = Vec::new();
        for name in candidates {
            if name.to_ascii_lowercase().contains(&needle) && self.promoted.insert(name.clone()) {
                promoted.push(name.clone());
            }
        }
        promoted
    }

    pub fn is_promoted(&self, name: &str) -> bool {
        self.promoted.contains(name)
    }

    pub fn promoted_count(&self) -> usize {
        self.promoted.len()
    }

    /// Decide whether one tool should be advertised.
    ///
    /// `extra_promoted` carries promotions recorded outside this value (the
    /// session-scoped `tool_help` registry). Keeping it a parameter instead of a
    /// global lookup keeps this type pure and testable.
    pub fn is_visible(
        &self,
        name: &str,
        policy: DisclosurePolicy,
        forced_visible: &HashSet<String>,
        extra_promoted: &HashSet<String>,
    ) -> bool {
        // An explicit allow-list (CLI `--tools`, config `[tools].enabled`, ACP
        // profile) is a user statement of intent: always honour it.
        if forced_visible.contains(name) {
            return true;
        }
        if self.promoted.contains(name) || extra_promoted.contains(name) {
            return true;
        }
        tier_for(name).is_default_visible(policy)
    }

    /// Filter a definition list down to the advertised subset, preserving order.
    pub fn apply(
        &self,
        definitions: &mut Vec<ToolDefinition>,
        policy: DisclosurePolicy,
        forced_visible: &HashSet<String>,
        extra_promoted: &HashSet<String>,
    ) {
        if policy == DisclosurePolicy::Legacy {
            return;
        }
        definitions
            .retain(|def| self.is_visible(&def.name, policy, forced_visible, extra_promoted));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: String::new(),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    #[test]
    fn core_tools_are_always_visible() {
        let sel = DisclosureSelection::new();
        let forced = HashSet::new();
        let none = HashSet::new();
        for name in ["read", "write", "edit", "bash", "alphagrep", "todo"] {
            assert!(
                sel.is_visible(name, DisclosurePolicy::Progressive, &forced, &none),
                "{name} must stay advertised"
            );
        }
    }

    #[test]
    fn extended_tools_are_hidden_until_promoted() {
        let mut sel = DisclosureSelection::new();
        let forced = HashSet::new();
        let none = HashSet::new();
        assert!(!sel.is_visible("sqlmap", DisclosurePolicy::Progressive, &forced, &none));
        assert!(sel.promote("sqlmap"));
        assert!(sel.is_visible("sqlmap", DisclosurePolicy::Progressive, &forced, &none));
        // Idempotent: promoting twice is not a new promotion.
        assert!(!sel.promote("sqlmap"));
    }

    #[test]
    fn legacy_policy_advertises_everything() {
        let sel = DisclosureSelection::new();
        let forced = HashSet::new();
        let none = HashSet::new();
        assert!(sel.is_visible("volatility", DisclosurePolicy::Legacy, &forced, &none));
    }

    #[test]
    fn unknown_tools_stay_visible() {
        let sel = DisclosureSelection::new();
        let forced = HashSet::new();
        let none = HashSet::new();
        // MCP and plugin tools must never be hidden by accident.
        assert!(sel.is_visible(
            "mcp__github__create_issue",
            DisclosurePolicy::Progressive,
            &forced,
            &none
        ));
        assert!(sel.is_visible(
            "some_future_tool",
            DisclosurePolicy::Progressive,
            &forced,
            &none
        ));
    }

    #[test]
    fn explicit_allow_list_wins_over_tier() {
        let sel = DisclosureSelection::new();
        let mut forced = HashSet::new();
        forced.insert("sqlmap".to_string());
        let none = HashSet::new();
        assert!(sel.is_visible("sqlmap", DisclosurePolicy::Progressive, &forced, &none));
    }

    #[test]
    fn external_namespaces_are_not_filtered_by_builtin_names() {
        assert_eq!(tier_for("mcp__filesystem__read"), ToolTier::Standard);
        assert_eq!(tier_for("mcp__x__sqlmap"), ToolTier::Standard);
    }

    #[test]
    fn apply_filters_in_place_and_preserves_order() {
        let mut sel = DisclosureSelection::new();
        sel.promote("nmap");
        let forced = HashSet::new();
        let none = HashSet::new();
        let mut defs = vec![
            def("read"),
            def("sqlmap"),
            def("nmap"),
            def("volatility"),
            def("write"),
        ];
        sel.apply(&mut defs, DisclosurePolicy::Progressive, &forced, &none);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["read", "nmap", "write"]);
    }

    /// Every tool the registry registers must be explicitly Tiered. A tool that
    /// falls through to `Standard` is advertised on every request, so an
    /// unclassified addition silently re-inflates the prompt budget.
    #[test]
    fn every_registered_builtin_tool_is_classified() {
        let unclassified: Vec<String> = super::super::known_tool_names()
            .into_iter()
            .filter(|name| {
                // MCP and plugin tools are deliberately left visible.
                !name.starts_with("mcp__") && tier_for(name) == ToolTier::Standard
            })
            .collect();
        assert!(
            unclassified.is_empty(),
            "these built-in tools are not classified in Core or Extended and would be              advertised every turn: {unclassified:?}. Add them to disclosure::tier_for."
        );
    }

    #[test]
    fn policy_parse_accepts_legacy_aliases() {
        for raw in ["legacy", "OFF", "all", "full", "0", "false", "no"] {
            assert_eq!(
                DisclosurePolicy::parse(raw),
                DisclosurePolicy::Legacy,
                "{raw}"
            );
        }
        for raw in ["progressive", "on", "", "anything-else"] {
            assert_eq!(
                DisclosurePolicy::parse(raw),
                DisclosurePolicy::Progressive,
                "{raw}"
            );
        }
    }

    #[test]
    fn promote_matching_finds_family_members() {
        let mut sel = DisclosureSelection::new();
        let candidates: Vec<String> = ["subfinder", "httpx", "read", "sqlmap"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let promoted = sel.promote_matching("sql", &candidates);
        assert_eq!(promoted, vec!["sqlmap".to_string()]);
    }
}
