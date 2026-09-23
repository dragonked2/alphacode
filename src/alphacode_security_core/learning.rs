use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Failure cause taxonomy — converts raw logs into reusable knowledge.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Environmental,
    Strategic,
    ToolRelated,
    ReasoningRelated,
}

impl FailureKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Environmental => "environmental",
            Self::Strategic => "strategic",
            Self::ToolRelated => "tool_related",
            Self::ReasoningRelated => "reasoning_related",
        }
    }
}

/// A structured lesson distilled from a completed investigation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lesson {
    pub id: String,
    pub what_failed_or_worked: String,
    pub why: String,
    pub kind: Option<FailureKind>,
    pub reusable: bool,
    pub avoid_when: Vec<String>,
    pub useful_when: Vec<String>,
    pub timestamp: String,
}

impl Lesson {
    pub fn new(what: String, why: String, kind: Option<FailureKind>, reusable: bool) -> Self {
        let id = format!("les_{:016x}", {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            what.hash(&mut h);
            why.hash(&mut h);
            h.finish()
        });
        Self {
            id,
            what_failed_or_worked: what,
            why,
            kind,
            reusable,
            avoid_when: Vec::new(),
            useful_when: Vec::new(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Empirical action performance — foundation for future policy selection.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ActionOutcomeStats {
    pub executions: u32,
    pub successes: u32,
    pub useful_evidence_count: u32,
    pub total_cost_units: u64,
    pub total_information_gain: f32,
    pub failure_modes: HashMap<String, u32>,
    pub verification_successes: u32,
}

impl ActionOutcomeStats {
    pub fn record(&mut self, success: bool, useful_evidence: bool, cost: u64, info_gain: f32) {
        self.executions += 1;
        if success {
            self.successes += 1;
        }
        if useful_evidence {
            self.useful_evidence_count += 1;
        }
        self.total_cost_units += cost;
        self.total_information_gain += info_gain;
    }

    pub fn record_failure_mode(&mut self, mode: String) {
        *self.failure_modes.entry(mode).or_insert(0) += 1;
    }

    pub fn success_rate(&self) -> f32 {
        if self.executions == 0 {
            0.0
        } else {
            self.successes as f32 / self.executions as f32
        }
    }

    pub fn evidence_rate(&self) -> f32 {
        if self.executions == 0 {
            0.0
        } else {
            self.useful_evidence_count as f32 / self.executions as f32
        }
    }

    pub fn avg_cost(&self) -> f32 {
        if self.executions == 0 {
            0.0
        } else {
            self.total_cost_units as f32 / self.executions as f32
        }
    }

    pub fn avg_information_gain(&self) -> f32 {
        if self.executions == 0 {
            0.0
        } else {
            self.total_information_gain / self.executions as f32
        }
    }

    /// Information per cost — used to prefer efficient actions under budget.
    pub fn efficiency(&self) -> f32 {
        let cost = self.avg_cost().max(0.1);
        self.avg_information_gain() / cost
    }
}

/// Post-task self-improvement: WHAT WORKED / FAILED / WASTED / etc.
/// Produces structured lessons, never raw log dumps.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct AfterActionReview {
    pub what_worked: Vec<String>,
    pub what_failed: Vec<String>,
    pub what_wasted: Vec<String>,
    pub wrong_hypotheses: Vec<String>,
    pub most_useful_tool: Option<String>,
    pub verification_that_worked: Vec<String>,
    pub loop_points: Vec<String>,
    pub do_differently: Vec<String>,
}

impl AfterActionReview {
    pub fn to_lessons(&self) -> Vec<Lesson> {
        let mut lessons = Vec::new();
        for w in &self.what_failed {
            lessons.push(Lesson::new(
                w.clone(),
                "recorded in after-action review".to_string(),
                Some(FailureKind::Strategic),
                true,
            ));
        }
        for w in &self.what_worked {
            let mut l = Lesson::new(
                w.clone(),
                "successful strategy worth reusing".to_string(),
                None,
                true,
            );
            l.useful_when.push("similar state".to_string());
            lessons.push(l);
        }
        lessons
    }
}

/// Memory-aware adjustment: lessons influence action ranking without
/// overriding fresh evidence. Returns a bounded bonus/penalty in [-0.3, +0.3].
pub fn lesson_adjustment(action_id: &str, state_hint: &str, lessons: &[Lesson]) -> f32 {
    let mut adj: f32 = 0.0;
    for lesson in lessons.iter().filter(|l| l.reusable) {
        let mentions_action = lesson.what_failed_or_worked.contains(action_id)
            || lesson.avoid_when.iter().any(|s| s == action_id)
            || lesson.useful_when.iter().any(|s| s == action_id);
        if !mentions_action {
            continue;
        }
        let relevant = state_hint.is_empty()
            || lesson.avoid_when.iter().any(|s| state_hint.contains(s))
            || lesson.useful_when.iter().any(|s| state_hint.contains(s));
        if !relevant && !state_hint.is_empty() {
            continue;
        }
        if lesson.kind.is_some() {
            // Failure lesson: penalize.
            adj -= 0.15;
        } else {
            adj += 0.1;
        }
    }
    adj.clamp(-0.3, 0.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_stats_efficiency() {
        let mut s = ActionOutcomeStats::default();
        s.record(true, true, 2, 0.8);
        s.record(false, false, 4, 0.1);
        assert!((s.success_rate() - 0.5).abs() < 0.01);
        assert!(s.efficiency() > 0.0);
    }

    #[test]
    fn lesson_adjustment_penalizes_failures() {
        let mut l = Lesson::new(
            "http_probe failed".into(),
            "timeout".into(),
            Some(FailureKind::Environmental),
            true,
        );
        l.avoid_when.push("http_probe".into());
        let adj = lesson_adjustment("http_probe", "http_probe", &[l]);
        assert!(adj < 0.0);
    }

    #[test]
    fn review_converts_to_lessons() {
        let r = AfterActionReview {
            what_failed: vec!["repeated scan".into()],
            what_worked: vec!["auth compare".into()],
            ..Default::default()
        };
        assert_eq!(r.to_lessons().len(), 2);
    }
}
