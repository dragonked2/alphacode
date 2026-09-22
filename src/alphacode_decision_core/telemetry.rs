use crate::alphacode_decision_core::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A decision trace event for telemetry and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTraceEvent {
    /// Unique event ID.
    pub event_id: u64,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// The question that was asked.
    pub question_id: String,
    /// Question version.
    pub question_version: u32,
    /// The result.
    pub outcome: DecisionOutcome,
    /// Confidence of the decision.
    pub confidence: f32,
    /// Which provider produced this.
    pub provider: String,
    /// Which model.
    pub model: String,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Whether this was a cache hit.
    pub cache_hit: bool,
    /// Whether native or emulated.
    pub native: bool,
    /// The chosen choice (if Choice decision).
    pub chosen: Option<String>,
    /// The probability (if Probability decision).
    pub probability: Option<f32>,
    /// The score (if Score decision).
    pub score: Option<f32>,
}

/// Aggregated decision telemetry for reporting.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionTelemetrySummary {
    pub total_decisions: u64,
    pub decided_count: u64,
    pub abstained_count: u64,
    pub deferred_count: u64,
    pub needs_more_context_count: u64,
    pub avg_confidence: f32,
    pub avg_latency_ms: u64,
    pub cache_hit_rate: f32,
    pub fallback_rate: f32,
    pub native_rate: f32,
    /// Per-question breakdown.
    pub per_question: Vec<QuestionTelemetry>,
}

/// Per-question telemetry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuestionTelemetry {
    pub question_id: String,
    pub count: u64,
    pub decided: u64,
    pub abstained: u64,
    pub avg_confidence: f32,
    pub avg_latency_ms: u64,
}

/// In-memory trace buffer for decision events.
pub struct DecisionTrace {
    events: std::sync::Mutex<Vec<DecisionTraceEvent>>,
    next_id: std::sync::atomic::AtomicU64,
    max_events: usize,
}

impl DecisionTrace {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::with_capacity(max_events.min(1000))),
            next_id: std::sync::atomic::AtomicU64::new(1),
            max_events,
        }
    }

    /// Record a decision trace event.
    pub fn record(
        &self,
        question: &DecisionQuestion,
        result: &DecisionResult,
        provider: &str,
        model: &str,
        latency_ms: u64,
        cache_hit: bool,
        native: bool,
    ) {
        let event_id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let (chosen, probability, score) = match result {
            DecisionResult::Probability(d) => (None, Some(d.probability), None),
            DecisionResult::Choice(d) => (Some(d.choice.clone()), None, None),
            DecisionResult::Score(d) => (None, None, Some(d.score)),
        };

        let event = DecisionTraceEvent {
            event_id,
            timestamp: Utc::now(),
            question_id: question.question_id.clone(),
            question_version: question.version,
            outcome: result.outcome().clone(),
            confidence: result.confidence(),
            provider: provider.to_string(),
            model: model.to_string(),
            latency_ms,
            cache_hit,
            native,
            chosen,
            probability,
            score,
        };

        let mut events = self.events.lock().unwrap();
        if events.len() >= self.max_events {
            // Drop oldest 10% to avoid constant reallocation
            let drain_count = (self.max_events / 10).max(1);
            events.drain(..drain_count);
        }
        events.push(event);
    }

    /// Get all recorded events.
    pub fn events(&self) -> Vec<DecisionTraceEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Get the number of recorded events.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Check if the trace is empty.
    pub fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty()
    }

    /// Clear all recorded events.
    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Generate a summary from recorded events.
    pub fn summarize(&self) -> DecisionTelemetrySummary {
        let events = self.events.lock().unwrap();
        if events.is_empty() {
            return DecisionTelemetrySummary::default();
        }

        let total = events.len() as u64;
        let decided = events
            .iter()
            .filter(|e| e.outcome == DecisionOutcome::Decided)
            .count() as u64;
        let abstained = events
            .iter()
            .filter(|e| e.outcome == DecisionOutcome::Abstained)
            .count() as u64;
        let deferred = events
            .iter()
            .filter(|e| e.outcome == DecisionOutcome::Deferred)
            .count() as u64;
        let needs_more = events
            .iter()
            .filter(|e| e.outcome == DecisionOutcome::NeedsMoreContext)
            .count() as u64;

        let avg_confidence = events.iter().map(|e| e.confidence).sum::<f32>() / total as f32;
        let avg_latency = events.iter().map(|e| e.latency_ms).sum::<u64>() / total;
        let cache_hits = events.iter().filter(|e| e.cache_hit).count() as f32 / total as f32;
        let native_count = events.iter().filter(|e| e.native).count() as f32 / total as f32;

        // Per-question breakdown
        let mut question_map: std::collections::HashMap<String, Vec<&DecisionTraceEvent>> =
            std::collections::HashMap::new();
        for event in events.iter() {
            question_map
                .entry(event.question_id.clone())
                .or_default()
                .push(event);
        }

        let per_question: Vec<QuestionTelemetry> = question_map
            .into_iter()
            .map(|(id, evts)| {
                let count = evts.len() as u64;
                let decided = evts
                    .iter()
                    .filter(|e| e.outcome == DecisionOutcome::Decided)
                    .count() as u64;
                let abstained = evts
                    .iter()
                    .filter(|e| e.outcome == DecisionOutcome::Abstained)
                    .count() as u64;
                let avg_conf = evts.iter().map(|e| e.confidence).sum::<f32>() / count as f32;
                let avg_lat = evts.iter().map(|e| e.latency_ms).sum::<u64>() / count;
                QuestionTelemetry {
                    question_id: id,
                    count,
                    decided,
                    abstained,
                    avg_confidence: avg_conf,
                    avg_latency_ms: avg_lat,
                }
            })
            .collect();

        DecisionTelemetrySummary {
            total_decisions: total,
            decided_count: decided,
            abstained_count: abstained,
            deferred_count: deferred,
            needs_more_context_count: needs_more,
            avg_confidence,
            avg_latency_ms: avg_latency,
            cache_hit_rate: cache_hits,
            fallback_rate: 0.0, // Would need to track from engine
            native_rate: native_count,
            per_question,
        }
    }

    /// Format a compact trace line for TUI display.
    pub fn format_compact_event(event: &DecisionTraceEvent) -> String {
        let outcome_char = match event.outcome {
            DecisionOutcome::Decided => "D",
            DecisionOutcome::Abstained => "A",
            DecisionOutcome::Deferred => "F",
            DecisionOutcome::NeedsMoreContext => "?",
        };
        let detail = if let Some(ref c) = event.chosen {
            format!(" {} {}", c, event.confidence)
        } else if let Some(p) = event.probability {
            format!(" {:.2} {}", p, event.confidence)
        } else if let Some(s) = event.score {
            format!(" {:.2} {}", s, event.confidence)
        } else {
            format!(" {}", event.confidence)
        };
        format!(
            "DECIDE {} {} {}{}ms",
            outcome_char, event.question_id, detail, event.latency_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_question(id: &str) -> DecisionQuestion {
        DecisionQuestion::probability(id, "test")
    }

    #[test]
    fn trace_records_and_summarizes() {
        let trace = DecisionTrace::new(100);
        let q = make_question("q1");
        let r = DecisionResult::Probability(ProbabilityDecision::new(0.9, 0.85));
        trace.record(&q, &r, "mock", "mock-v1", 10, false, true);

        let q2 = make_question("q2");
        let r2 = DecisionResult::Choice(ChoiceDecision::abstained());
        trace.record(&q2, &r2, "mock", "mock-v1", 5, false, false);

        assert_eq!(trace.len(), 2);

        let summary = trace.summarize();
        assert_eq!(summary.total_decisions, 2);
        assert_eq!(summary.decided_count, 1);
        assert_eq!(summary.abstained_count, 1);
        assert_eq!(summary.per_question.len(), 2);
    }

    #[test]
    fn trace_evicts_old_events() {
        let trace = DecisionTrace::new(5);
        let q = make_question("q");
        for i in 0..10 {
            let r = DecisionResult::Probability(ProbabilityDecision::new(i as f32 / 10.0, 0.5));
            trace.record(&q, &r, "m", "m", 1, false, false);
        }
        assert!(trace.len() <= 5);
    }

    #[test]
    fn compact_event_format() {
        let event = DecisionTraceEvent {
            event_id: 1,
            timestamp: Utc::now(),
            question_id: "tool_result.sufficient".into(),
            question_version: 1,
            outcome: DecisionOutcome::Decided,
            confidence: 0.85,
            provider: "mock".into(),
            model: "mock-v1".into(),
            latency_ms: 12,
            cache_hit: false,
            native: false,
            chosen: Some("continue".into()),
            probability: None,
            score: None,
        };
        let formatted = DecisionTrace::format_compact_event(&event);
        assert!(formatted.contains("DECIDE"));
        assert!(formatted.contains("continue"));
        assert!(formatted.contains("12ms"));
    }
}
