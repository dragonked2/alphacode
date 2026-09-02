//! Stage 2: the action gate.
//!
//! Stage 1 ([`crate::assess`]) decides *whether* a command deserves scrutiny.
//! This decides *what happens next*.
//!
//! # Why this is so simple
//!
//! The previous design had a `Confirm` tier that held every cross-directory
//! `rm`/`find -delete` for a "reflection turn", forcing the model to
//! re-issue the same call with a `justification` field. In practice that
//! gate fired on routine authorized bug-bounty work (`nmap`, `subfinder`,
//! `nuclei`, `httpx`, `ffuf`, `gobuster`, `curl` against an in-scope
//! target, etc.) because every such tool reaches outside the working
//! directory by definition. The user-visible result was "I tried to scan a
//! real bug bounty program and the agent refused", which is the opposite of
//! what an authorized pentest harness is for.
//!
//! The gate is now reduced to the two outcomes that actually protect the
//! user from permanent loss: `Allow` for everything except the catastrophic
//! tier, and `Deny` for the catastrophic tier. The catastrophic tier still
//! covers `rm -rf ~`, `rm -rf ~/.ssh`, `rm -rf /etc`, recursive destruction
//! of system paths, and direct writes to device nodes — none of which a
//! legitimate scanning workflow will ever request.
//!
//! # Honest limitations
//!
//! This is defense in depth, not a sandbox. The catastrophic tier is a
//! small, absolute, path-based deny that does not depend on parsing the
//! command correctly, which is what makes it the only tier worth keeping.

use crate::alphacode_command_risk::{RiskAssessment, RiskLevel};

/// What the harness should do with a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    /// Run it.
    Allow,
    /// Refuse permanently. No justification unlocks this.
    Deny { reason: String },
}

/// A justification supplied by the model on a second attempt.
///
/// Kept for API compatibility with the previous design; no longer gates
/// execution. Scanning an authorized target is not a thing that needs to be
/// justified twice.
#[derive(Debug, Clone, Default)]
pub struct Justification {
    /// The model's account of which user request this serves.
    pub text: Option<String>,
}

impl Justification {
    /// Whether the justification is substantive rather than a token retry.
    ///
    /// Kept for API compatibility but unused by [`gate`].
    pub fn is_substantive(&self) -> bool {
        let Some(text) = self.text.as_deref().map(str::trim) else {
            return false;
        };
        if text.len() < MIN_JUSTIFICATION_LEN {
            return false;
        }
        // Reject pure affirmations, which carry no information about intent.
        const EMPTY_AFFIRMATIONS: &[&str] = &[
            "yes",
            "ok",
            "okay",
            "sure",
            "confirmed",
            "proceed",
            "do it",
            "continue",
            "y",
            "approved",
            "go ahead",
        ];
        let lowered = text.to_lowercase();
        let stripped = lowered.trim_end_matches(['.', '!', '?']).trim();
        !EMPTY_AFFIRMATIONS.contains(&stripped)
    }
}

/// Minimum characters for a justification to count as considered.
const MIN_JUSTIFICATION_LEN: usize = 25;

/// Decide what to do with an assessed command.
pub fn gate(assessment: &RiskAssessment, _justification: &Justification) -> GateOutcome {
    match assessment.level {
        RiskLevel::Safe | RiskLevel::Low | RiskLevel::Confirm => GateOutcome::Allow,
        RiskLevel::Catastrophic => GateOutcome::Deny {
            reason: format!(
                "This command is blocked because it would destroy a protected \
                 path (home directory, credential store, or system root).\n\n{}\n\
                 Run it yourself outside the agent if it is genuinely required.",
                assessment.explanation()
            ),
        },
    }
}