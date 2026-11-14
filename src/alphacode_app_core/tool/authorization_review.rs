//! Offline authorization evidence review; supplied labels are not live captures.
use super::{Tool, ToolContext, ToolExecutionClass, ToolOutput};
use crate::alphacode_security_core::authz::{AccessCase, AuthorizationVerdict, analyze_matrix};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct AuthorizationReviewTool;

#[derive(Deserialize)]
struct Input {
    case: AccessCase,
    #[serde(default)]
    offset: usize,
}

fn review(input: Value) -> Result<Value> {
    ensure!(
        input.to_string().len() <= 2 * 1024 * 1024,
        "authorization_review input exceeds 2 MiB"
    );
    let input: Input = serde_json::from_value(input)?;
    let case = input.case;
    ensure!(
        case.identities.len() <= 32
            && case.resources.len() <= 256
            && case.observations.len() <= 10_000,
        "authorization_review limits: 32 identities, 256 resources, 10000 observations"
    );
    let mut ids = std::collections::BTreeSet::new();
    ensure!(
        case.identities
            .iter()
            .all(|i| !i.id.trim().is_empty() && ids.insert(i.id.as_str())),
        "authorization_review requires nonempty unique identity IDs"
    );
    let mut resources = std::collections::BTreeSet::new();
    ensure!(
        case.resources
            .iter()
            .all(|r| resources.insert(r.report_label())),
        "authorization_review requires unique resource labels"
    );
    let assessments = analyze_matrix(&case);
    ensure!(
        input.offset <= assessments.len(),
        "authorization_review offset exceeds matrix size"
    );
    let supported = assessments.iter().filter(|a| a.is_reportable()).count();
    let inconclusive = assessments
        .iter()
        .filter(|a| a.verdict == AuthorizationVerdict::Inconclusive)
        .count();
    let end = input.offset.saturating_add(16).min(assessments.len());
    Ok(json!({
        "basis": "offline review of supplied observations; captures, identity ownership, policy and security impact require independent verification",
        "operation": "read",
        "total_pairs": assessments.len(),
        "evidence_supported_pairs": supported,
        "inconclusive_pairs": inconclusive,
        "other_pairs": assessments.len() - supported - inconclusive,
        "distinct_root_causes": null,
        "offset": input.offset,
        "next_offset": (end < assessments.len()).then_some(end),
        "assessments": &assessments[input.offset..end],
        "limitations": "Does not send requests, authenticate captures, count distinct root causes or test writes. Never equate pairs with discoveries."
    }))
}

#[async_trait]
impl Tool for AuthorizationReviewTool {
    fn name(&self) -> &str {
        "authorization_review"
    }

    fn description(&self) -> &str {
        "Review controlled multi-user read-authorization evidence offline. Returns identity/resource assessments, missing proof and false-positive causes. Supply recorded observations and declared ownership; this does not execute tests or independently verify them. Paginates 16 pairs per call."
    }

    fn execution_class(&self, _input: &Value) -> ToolExecutionClass {
        ToolExecutionClass::ReadOnly
    }

    fn parameters_schema(&self) -> Value {
        json!({"type":"object", "required":["case"], "properties":{
            "intent": super::intent_schema_property(),
            "offset":{"type":"integer", "minimum":0, "description":"Next offset from a previous result; default 0."},
            "case":{"type":"object", "required":["identities","resources","observations","confirmed_absent"], "properties":{
                "identities":{"type":"array", "maxItems":32, "items":{"type":"object", "required":["id","label","roles","authenticated"], "properties":{
                    "id":{"type":"string"}, "label":{"type":"string"}, "roles":{"type":"array","items":{"type":"string","enum":["anonymous","member","support","admin","owner"]}},
                    "authenticated":{"type":"boolean"}, "tenant":{"type":["string","null"]}
                }}},
                "resources":{"type":"array", "maxItems":256, "description":"Resource labels in observations are kind:id. Use only controlled objects and established policy.", "items":{"type":"object", "required":["kind","id","visibility","sensitive","shared_with"], "properties":{
                    "kind":{"type":"string"}, "id":{"type":"string"}, "owner":{"type":["string","null"]}, "tenant":{"type":["string","null"]},
                    "visibility":{"type":"string","enum":["public","shared","private","undetermined"]}, "sensitive":{"type":"boolean"}, "shared_with":{"type":"array","items":{"type":"string"}}
                }}},
                "observations":{"type":"array", "maxItems":10000, "items":{"type":"object", "required":["identity","resource","operation","outcome","exchange_id","independent_repeat"], "properties":{
                    "identity":{"type":"string"}, "resource":{"type":"string"}, "operation":{"type":"string","enum":["read","create","update","delete","mutation","admin_action"]},
                    "outcome":{"type":"string","enum":["denied","returned","empty","error","not_observed"]},
                    "protected_data_marker":{"type":["string","null"], "description":"Report-safe label for actually observed protected content; never credentials or personal data."},
                    "exchange_id":{"type":"string"}, "independent_repeat":{"type":"boolean"}
                }}},
                "confirmed_absent":{"type":"array", "description":"Pairs [identity ID, resource label] recorded as absent; [] when none.","items":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":2}}
            }}
        }})
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::new(serde_json::to_string(&review(input)?)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::authz::{ResourceRef, Role, TestIdentity};

    #[test]
    fn review_exposes_missing_evidence_without_claiming_discovery() {
        let case = AccessCase::new()
            .with_identity(TestIdentity::new("a", "tester-a", vec![Role::Member]))
            .with_resource(ResourceRef::owned_by("document", "1", "b"));
        let output = review(json!({"case":case})).unwrap();
        assert_eq!(output["inconclusive_pairs"], 1);
        assert_eq!(output["evidence_supported_pairs"], 0);
        assert!(output["distinct_root_causes"].is_null());
        assert!(
            !output["assessments"][0]["unmet_requirements"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn pages_preserve_every_pair_and_reject_ambiguous_identities() {
        let mut case =
            AccessCase::new().with_identity(TestIdentity::new("a", "tester-a", vec![Role::Member]));
        for i in 0..20 {
            case.resources
                .push(ResourceRef::public_("document", i.to_string()));
        }
        let first = review(json!({"case":case})).unwrap();
        assert_eq!(first["total_pairs"], 20);
        assert_eq!(first["assessments"].as_array().unwrap().len(), 16);
        assert_eq!(first["next_offset"], 16);
        let last = review(json!({"case":case,"offset":16})).unwrap();
        assert_eq!(last["assessments"].as_array().unwrap().len(), 4);
        assert!(last["next_offset"].is_null());
        case.identities.push(case.identities[0].clone());
        assert!(review(json!({"case":case})).is_err());
    }

    #[test]
    fn rejects_excessive_work_and_invalid_pages() {
        let mut case = AccessCase::new();
        for i in 0..33 {
            case.identities.push(TestIdentity::new(
                i.to_string(),
                "tester",
                vec![Role::Member],
            ));
        }
        assert!(review(json!({"case":case})).is_err());
        assert!(review(json!({"case":AccessCase::new(), "offset":1})).is_err());
        assert!(
            review(json!({"case":AccessCase::new(), "padding":"x".repeat(2 * 1024 * 1024)}))
                .is_err()
        );
    }

    #[tokio::test]
    async fn advertised_contract_executes_without_network_or_files() {
        let tool = AuthorizationReviewTool;
        let input = json!({"case":{
            "identities":[{"id":"a","label":"controlled-a","roles":["member"],"authenticated":true}],
            "resources":[{"kind":"document","id":"1","owner":"b","visibility":"private","sensitive":true,"shared_with":[]}],
            "observations":[], "confirmed_absent":[]
        }});
        super::super::schema_preflight::validate_call(
            tool.name(),
            &tool.parameters_schema(),
            &input,
        )
        .unwrap();
        assert_eq!(tool.execution_class(&input), ToolExecutionClass::ReadOnly);
        let ctx = ToolContext {
            session_id: "offline-review-test".into(),
            message_id: "test".into(),
            tool_call_id: "test".into(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: super::super::ToolExecutionMode::Direct,
        };
        let output = tool.execute(input, ctx).await.unwrap();
        let output: Value = serde_json::from_str(&output.output).unwrap();
        assert_eq!(output["inconclusive_pairs"], 1);
        assert!(output["next_offset"].is_null());
    }
}
