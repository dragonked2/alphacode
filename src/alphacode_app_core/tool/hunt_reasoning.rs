//! Apply distilled research logic to declared observations on a new target.
use std::collections::BTreeSet;
use std::io::Read;

use super::{Tool, ToolContext, ToolExecutionClass, ToolOutput};
use crate::alphacode_security_core::reasoning::{RESEARCH_DISCIPLINE, ReasoningCurriculum};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct HuntReasoningTool;

#[derive(Deserialize)]
struct Observation {
    cue: String,
    evidence: String,
}

#[derive(Deserialize)]
struct Input {
    action: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    observations: Vec<Observation>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    3
}

fn load() -> Result<ReasoningCurriculum> {
    let mut curriculum = ReasoningCurriculum::bundled()?;
    let path = crate::storage::alphacode_dir()?.join("research/reasoning_lessons.json");
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(curriculum),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    let imported = ReasoningCurriculum::parse(&bytes)?;
    curriculum.lessons.extend(imported.lessons);
    // An imported lesson cannot silently replace a shipped rule.
    curriculum.validate()?;
    Ok(curriculum)
}

fn apply(input: Value, curriculum: &ReasoningCurriculum) -> Result<Value> {
    ensure!(
        input.to_string().len() <= 64 * 1024,
        "reasoning input exceeds 64 KiB"
    );
    let input: Input = serde_json::from_value(input)?;
    if input.action == "cues" {
        let cues = curriculum.cues();
        ensure!(
            input.offset <= cues.len(),
            "cue offset exceeds vocabulary size"
        );
        let end = input.offset.saturating_add(64).min(cues.len());
        let page: Vec<_> = cues.iter().skip(input.offset).take(64).collect();
        return Ok(
            json!({"cues":page,"offset":input.offset,"next_offset":(end < cues.len()).then_some(end),"total_cues":cues.len(),"lesson_count":curriculum.lessons.len(),
            "use":"Select structural cues only after observing them; attach a capture or source reference and an explanation. No program-name or vulnerability-label matching."}),
        );
    }
    ensure!(input.action == "apply", "unknown reasoning action");
    ensure!(
        (1..=8).contains(&input.limit) && (1..=64).contains(&input.observations.len()),
        "supply 1-64 observations and limit 1-8"
    );
    let known = curriculum.cues();
    let mut seen = BTreeSet::new();
    for observation in &input.observations {
        ensure!(
            known.contains(observation.cue.as_str())
                && seen.insert(&observation.cue)
                && !observation.evidence.trim().is_empty()
                && observation.evidence.len() <= 1024,
            "each observation needs a unique known cue and bounded evidence explanation; use action=cues"
        );
    }
    let cues: Vec<_> = input.observations.iter().map(|o| o.cue.clone()).collect();
    let mut advice = Vec::new();
    let mut omitted = false;
    for lesson in curriculum.applicable(&cues, input.limit) {
        let item = json!({"rule_id":lesson.id,"invariant":lesson.invariant,
            "challenged_assumption":lesson.broken_assumption,"reasoning":lesson.reasoning,
            "check_preconditions":lesson.preconditions,"proposed_test":lesson.test,
            "counterexamples":lesson.counterexamples,"transfer_question":lesson.transfer_question,
            "provenance":lesson.sources.iter().map(|s|json!({"url":format!("https://hackerone.com/reports/{}",s.report_id),"basis":s.basis})).collect::<Vec<_>>()});
        if serde_json::to_vec(&advice)?.len() + serde_json::to_vec(&item)?.len() > 20 * 1024 {
            omitted = true;
            break;
        }
        advice.push(item);
    }
    Ok(
        json!({"discipline":RESEARCH_DISCIPLINE,"reasoning_rules":advice,"output_bounded":omitted,
        "interpretation":"Analyst-distilled logic applied to caller-declared cues; observations and applicability are not independently verified. Proposed tests need current scope, policy, accounts and budgets. Rules are hypotheses, never target evidence, finding confirmation, severity or authorization. Source summaries support less detail than full narratives."}),
    )
}

#[async_trait]
impl Tool for HuntReasoningTool {
    fn name(&self) -> &str {
        "hunt_reasoning"
    }
    fn description(&self) -> &str {
        "Apply learned bug-hunting reasoning to observed product structure. Returns challenged assumptions, causal reasoning, applicability checks, counterexamples and controlled experiments. Learn the transferable logic, not historical targets or payloads. Use cues to discover the structural vocabulary, then apply with evidence explanations. Offline; no finding verification or authorization."
    }
    fn execution_class(&self, _input: &Value) -> ToolExecutionClass {
        ToolExecutionClass::ReadOnly
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","required":["action"],"properties":{
            "intent":super::intent_schema_property(),
            "action":{"type":"string","enum":["cues","apply"]},
            "offset":{"type":"integer","minimum":0,"description":"Action=cues: next_offset from the previous vocabulary page (64 cues per page)."},
            "observations":{"type":"array","minItems":1,"maxItems":64,"items":{
                "type":"object","required":["cue","evidence"],"properties":{
                    "cue":{"type":"string","description":"Observed structural cue from action=cues."},
                    "evidence":{"type":"string","maxLength":1024,"description":"Capture/source reference and why it establishes this cue; not a credential or raw private data."}
                }}},
            "limit":{"type":"integer","minimum":1,"maximum":8,"default":3}
        }})
    }
    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::new(serde_json::to_string(&apply(
            input,
            &load()?,
        )?)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expanded_curriculum_has_bounded_vocabulary_pages() {
        let mut curriculum = ReasoningCurriculum::bundled().unwrap();
        let seed = curriculum.lessons[0].clone();
        for index in 0..100 {
            let mut rule = seed.clone();
            rule.id = format!("extra_rule_{index}");
            rule.required_cues = vec![format!("new_observed_property_{index}")];
            curriculum.lessons.push(rule);
        }
        curriculum.validate().unwrap();
        let first = apply(json!({"action":"cues"}), &curriculum).unwrap();
        assert_eq!(first["cues"].as_array().unwrap().len(), 64);
        assert_eq!(first["next_offset"], 64);
        let second = apply(json!({"action":"cues","offset":64}), &curriculum).unwrap();
        assert!(
            second["cues"]
                .as_array()
                .unwrap()
                .iter()
                .all(|cue| !first["cues"].as_array().unwrap().contains(cue))
        );
        assert!(apply(json!({"action":"cues","offset":99999}), &curriculum).is_err());
    }

    #[test]
    fn novel_product_receives_an_experiment_not_a_past_target_story() {
        let result = apply(json!({"action":"apply","observations":[
            {"cue":"privacy_setting","evidence":"Owned astronomy-club event has a hidden-observatory setting; capture A."},
            {"cue":"derived_representation","evidence":"Capture B includes machine-readable map coordinates."}
        ]}), &ReasoningCurriculum::bundled().unwrap()).unwrap();
        assert_eq!(
            result["reasoning_rules"][0]["rule_id"],
            "follow_equivalent_private_representations"
        );
        assert!(result["reasoning_rules"][0]["proposed_test"]["negative_control"].is_string());
        assert!(!result.to_string().contains("fetlife"));
        assert!(
            result["interpretation"]
                .as_str()
                .unwrap()
                .contains("never target evidence")
        );
    }

    #[test]
    fn missing_structure_does_not_invent_applicability() {
        let curriculum = ReasoningCurriculum::bundled().unwrap();
        let result = apply(json!({"action":"apply","observations":[{"cue":"privacy_setting","evidence":"Owned setting observed"}]}), &curriculum).unwrap();
        assert!(result["reasoning_rules"].as_array().unwrap().is_empty());
        for observation in [
            json!({"cue":"idor","evidence":"scanner says IDOR"}),
            json!({"cue":"privacy_setting","evidence":""}),
        ] {
            assert!(
                apply(
                    json!({"action":"apply","observations":[observation]}),
                    &curriculum
                )
                .is_err()
            );
        }
    }
}
