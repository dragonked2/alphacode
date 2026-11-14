//! Transferable research rules, not a database of historical targets or payloads.
//! Matching suggests questions; it never raises finding confidence or grants scope.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Shipped, reviewed abstractions are part of every bounty prompt, including
/// after compaction. External lessons stay tool-result data, never instructions.
pub fn foundation_brief() -> &'static str {
    static BRIEF: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BRIEF.get_or_init(|| {
        let Ok(curriculum) = ReasoningCurriculum::bundled() else {
            return "Bundled reasoning curriculum unavailable; reason from current evidence and explicit controls.".into();
        };
        let mut brief = String::from("\nAnalyst-distilled research questions to apply when the corresponding product structure is observed. These are reasoning prompts, not evidence or claims that a mechanism exists on this target:\n");
        for lesson in curriculum.lessons {
            brief.push_str(&format!("- {}: {} {}\n", lesson.required_cues.join(" + "), lesson.invariant, lesson.transfer_question));
        }
        brief
    })
}

pub const RESEARCH_DISCIPLINE: &str = "Model the product's protected value and trust boundaries before choosing tests. State the invariant, the strongest benign explanation, and the smallest observation that distinguishes them. Vary one meaningful dimension at a time; compare alternate consumers, representations and lifecycle states. Establish authentication, ownership, reachability and prerequisites explicitly. Verify persisted effects with controlled data. A blocked tool is a coverage gap, not a negative result. Reassess unchanged probes when they stop reducing uncertainty; continue independent authorized work. Carry only observed capabilities into chains. Learned rules generate hypotheses, never findings; fresh target evidence and independent impact review remain necessary.";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningSource {
    pub report_id: u64,
    pub content_sha256: String,
    /// `narrative` and `summary` describe exactly what was accessible.
    pub basis: String,
    pub observation: String,
    /// A short grounding anchor, verified against the captured source by the compiler.
    pub evidence_quote: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningTest {
    pub positive_control: String,
    pub negative_control: String,
    pub discriminating_test: String,
    pub success_oracle: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningLesson {
    pub id: String,
    /// Structural cues established on the new application, never program names.
    pub required_cues: Vec<String>,
    pub invariant: String,
    pub broken_assumption: String,
    /// Analyst reconstruction; never represented as access to the author's thoughts.
    pub reasoning: Vec<String>,
    pub preconditions: Vec<String>,
    pub test: ReasoningTest,
    pub counterexamples: Vec<String>,
    pub transfer_question: String,
    pub sources: Vec<ReasoningSource>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningCurriculum {
    pub version: u32,
    pub lessons: Vec<ReasoningLesson>,
}

fn bounded_text(text: &str) -> bool {
    !text.trim().is_empty()
        && text.len() <= 2048
        && !text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
}

fn cue_name(text: &str) -> bool {
    (2..=64).contains(&text.len())
        && text
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

impl ReasoningCurriculum {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "reasoning curriculum exceeds 16 MiB"
        );
        let result: Self = serde_json::from_slice(bytes)?;
        result.validate()?;
        Ok(result)
    }

    pub fn bundled() -> Result<Self> {
        Self::parse(include_bytes!("reasoning_lessons.json"))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.lessons.len() <= 20_000,
            "unsupported or oversized curriculum"
        );
        let mut ids = BTreeSet::new();
        let mut quotations: BTreeMap<u64, BTreeSet<&str>> = BTreeMap::new();
        for lesson in &self.lessons {
            ensure!(
                cue_name(&lesson.id) && ids.insert(&lesson.id),
                "invalid or duplicate lesson id"
            );
            ensure!(
                (1..=8).contains(&lesson.required_cues.len())
                    && lesson.required_cues.iter().all(|cue| cue_name(cue))
                    && lesson.required_cues.iter().collect::<BTreeSet<_>>().len()
                        == lesson.required_cues.len(),
                "invalid structural cues"
            );
            for text in [
                &lesson.invariant,
                &lesson.broken_assumption,
                &lesson.transfer_question,
                &lesson.test.positive_control,
                &lesson.test.negative_control,
                &lesson.test.discriminating_test,
                &lesson.test.success_oracle,
            ] {
                ensure!(bounded_text(text), "missing or oversized reasoning field");
            }
            ensure!(
                lesson.test.positive_control.trim() != lesson.test.negative_control.trim(),
                "positive and negative controls must differ"
            );
            for texts in [
                &lesson.reasoning,
                &lesson.preconditions,
                &lesson.counterexamples,
            ] {
                ensure!(
                    (1..=12).contains(&texts.len()) && texts.iter().all(|t| bounded_text(t)),
                    "missing reasoning, applicability or counterexample"
                );
            }
            ensure!(
                (1..=8).contains(&lesson.sources.len()),
                "lesson needs grounded sources"
            );
            let mut reports = BTreeSet::new();
            for source in &lesson.sources {
                ensure!(
                    source.report_id > 0
                        && reports.insert(source.report_id)
                        && source.content_sha256.len() == 64
                        && source.content_sha256.bytes().all(|c| c.is_ascii_hexdigit())
                        && matches!(source.basis.as_str(), "narrative" | "summary")
                        && bounded_text(&source.observation)
                        && bounded_text(&source.evidence_quote)
                        && source.evidence_quote.split_whitespace().count() <= 25,
                    "invalid source grounding"
                );
                let quotes = quotations.entry(source.report_id).or_default();
                quotes.insert(&source.evidence_quote);
                ensure!(
                    quotes
                        .iter()
                        .map(|quote| quote.split_whitespace().count())
                        .sum::<usize>()
                        <= 25,
                    "source quotations exceed 25 words across lessons"
                );
            }
        }
        Ok(())
    }

    /// All prerequisite cues must be present. A vulnerability label or target name
    /// cannot activate a rule by itself. Preconditions still need manual checking.
    pub fn applicable<'a>(&'a self, observed: &[String], limit: usize) -> Vec<&'a ReasoningLesson> {
        let cues: BTreeSet<_> = observed.iter().map(String::as_str).collect();
        let mut result: Vec<_> = self
            .lessons
            .iter()
            .filter(|lesson| {
                lesson
                    .required_cues
                    .iter()
                    .all(|cue| cues.contains(cue.as_str()))
            })
            .collect();
        result.sort_by(|a, b| {
            b.required_cues
                .len()
                .cmp(&a.required_cues.len())
                .then_with(|| a.id.cmp(&b.id))
        });
        result.truncate(limit.min(8));
        result
    }

    pub fn cues(&self) -> BTreeSet<&str> {
        self.lessons
            .iter()
            .flat_map(|lesson| lesson.required_cues.iter().map(String::as_str))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_lessons_have_grounding_controls_and_counterexamples() {
        let curriculum = ReasoningCurriculum::bundled().unwrap();
        assert!(curriculum.lessons.len() >= 12);
        assert!(!curriculum.cues().is_empty());
    }

    #[test]
    fn transfer_uses_structure_not_a_known_program_or_bug_label() {
        let curriculum = ReasoningCurriculum::bundled().unwrap();
        assert!(
            curriculum
                .applicable(&["gitlab".into(), "idor".into()], 8)
                .is_empty()
        );
        assert!(
            curriculum
                .applicable(&["revocable_membership".into()], 8)
                .is_empty()
        );
        let matched = curriculum.applicable(
            &[
                "revocable_membership".into(),
                "retained_object_reference".into(),
            ],
            8,
        );
        assert!(
            matched
                .iter()
                .any(|l| l.id == "recheck_authority_after_transition")
        );
        assert!(matched.iter().all(|l| !l.counterexamples.is_empty()));
        assert!(curriculum.applicable(&[], usize::MAX).is_empty());
    }

    #[test]
    fn incomplete_or_metadata_only_lessons_are_rejected() {
        let mut curriculum = ReasoningCurriculum::bundled().unwrap();
        curriculum.lessons[0].sources[0].basis = "title".into();
        assert!(curriculum.validate().is_err());
        curriculum.lessons[0].sources[0].basis = "narrative".into();
        curriculum.lessons[0].counterexamples.clear();
        assert!(curriculum.validate().is_err());
    }

    #[test]
    fn duplicate_rules_and_unbounded_imports_fail_closed() {
        let mut curriculum = ReasoningCurriculum::bundled().unwrap();
        curriculum.lessons.push(curriculum.lessons[0].clone());
        assert!(curriculum.validate().is_err());
        assert!(ReasoningCurriculum::parse(&vec![b' '; 16 * 1024 * 1024 + 1]).is_err());
    }
}
