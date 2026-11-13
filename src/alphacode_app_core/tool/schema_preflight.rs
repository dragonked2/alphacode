//! Validate required fields before hooks, network permits, or tool side effects.
//! Scalar coercion remains the tool's responsibility: several native tools
//! deliberately accept provider-generated string forms of numbers and booleans.
use serde_json::Value;

pub(super) fn validate_call(tool: &str, schema: &Value, input: &Value) -> Result<(), String> {
    // These native parsers recover aliases and truncated provider payloads.
    // Their advertised required keys are stricter than their accepted input;
    // keep their existing validation until canonicalization is shared here.
    //
    // The exemption list is the contract for what the tool will actually
    // accept. Adding a tool here means that tool's serde aliases / manual
    // normalization are the reason the preflight must not run — not that the
    // tool is permissive. Removing an exemption requires sharing that tool's
    // canonicalization with the preflight instead.
    if matches!(
        tool,
        "bash"
            | "read"
            | "write"
            | "websearch"
            | "webfetch"
            | "scrapling"
            | "feroxbuster"
            | "assetfinder"
            | "amass"
            | "katana"
            | "dalfox"
            | "nmap"
            | "gobuster"
            | "nikto"
            | "subfinder"
            | "sqlmap"
            | "unfurl"
            | "edit"
            | "browser"
            | "batch"
            | "skill_manage"
            | "alphagrep"
    ) {
        return Ok(());
    }
    validate_required(schema, input)
}

pub(super) fn validate_required(schema: &Value, input: &Value) -> Result<(), String> {
    check(schema, input, "$", 0)
}

fn check(schema: &Value, input: &Value, path: &str, depth: usize) -> Result<(), String> {
    // External MCP schemas can be arbitrarily deep. Leave unsupported schema
    // constructs and deeper validation to their owning tool.
    if depth >= 64 {
        return Ok(());
    }
    if let Some(object) = input.as_object() {
        let missing: Vec<_> = schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|key| !object.contains_key(*key))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Missing required arguments at {path}: {}. Correct the arguments before retrying.",
                missing.join(", ")
            ));
        }
        if let Some(properties) = schema["properties"].as_object() {
            for (key, child_schema) in properties {
                if let Some(value) = object.get(key) {
                    check(child_schema, value, &format!("{path}.{key}"), depth + 1)?;
                }
            }
        }
    }
    if let Some(items) = input.as_array()
        && schema["items"].is_object()
    {
        for (index, item) in items.iter().enumerate() {
            check(
                &schema["items"],
                item,
                &format!("{path}[{index}]"),
                depth + 1,
            )?;
        }
    }
    if let Some(branches) = schema["allOf"].as_array() {
        for branch in branches {
            check(branch, input, path, depth + 1)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reports_all_missing_fields_without_echoing_values() {
        let schema = json!({"required": ["url", "action"]});
        let error = validate_required(&schema, &json!({"token": "secret"})).unwrap_err();
        assert!(error.contains("url, action"));
        assert!(!error.contains("secret"));
    }

    #[test]
    fn validates_nested_batch_arguments() {
        let schema = json!({"properties": {"calls": {"items": {
            "allOf": [{"required": ["name"]}, {"required": ["input"]}]
        }}}});
        let error = validate_required(&schema, &json!({"calls": [{"name": "read"}]})).unwrap_err();
        assert!(error.contains("$.calls[0]: input"));
    }

    #[test]
    fn preserves_optional_fields_and_native_coercion() {
        let schema = json!({"required": ["count"], "properties": {
            "count": {"type": "integer"}, "optional": {"required": ["value"]}
        }});
        assert!(validate_required(&schema, &json!({"count": "2"})).is_ok());
        assert!(validate_required(&schema, &json!({"count": 2, "optional": {}})).is_err());
    }

    #[test]
    fn native_alias_recovery_remains_available_without_exempting_external_tools() {
        let schema = json!({"required": ["url"]});
        let input = json!({"target": "https://example.com"});
        assert!(validate_call("webfetch", &schema, &input).is_ok());
        assert!(validate_call("mcp_webfetch", &schema, &input).is_err());
    }

    /// `edit` recovers a half-dozen serde aliases (`path`, `file`, `old`,
    /// `new`, …) that the advertised schema rejects. The preflight must not
    /// block a call the tool itself accepts, or the model burns its retry
    /// budget on calls that would have succeeded.
    #[test]
    fn edit_alias_recovery_is_not_blocked_by_preflight() {
        let schema = json!({
            "required": ["file_path", "old_string", "new_string"],
            "properties": {
                "file_path": {"type": "string"},
                "old_string": {"type": "string"},
                "new_string": {"type": "string"}
            }
        });
        let input = json!({"file": "a.rs", "old": "x", "new": "y"});
        assert!(
            validate_call("edit", &schema, &input).is_ok(),
            "edit must accept the `file`/`old`/`new` aliases it advertises recovery for"
        );
    }

    /// `browser` accepts `tabId`/`windowId`/`containerId` aliases for the
    /// `tab_id`/`window_id`/`container_id` fields it advertises.
    #[test]
    fn browser_id_alias_recovery_is_not_blocked_by_preflight() {
        let schema = json!({
            "required": ["action"],
            "properties": {
                "action": {"type": "string"},
                "tab_id": {"type": "string"},
                "window_id": {"type": "string"},
                "container_id": {"type": "string"}
            }
        });
        let input =
            json!({"action": "navigate", "tabId": "1", "windowId": "2", "containerId": "c"});
        assert!(
            validate_call("browser", &schema, &input).is_ok(),
            "browser must accept camelCase id aliases it advertises recovery for"
        );
    }

    /// `batch` accepts `name`/`arguments`/`args`/`input` aliases for the
    /// `tool`/`parameters` fields it advertises.
    #[test]
    fn batch_alias_recovery_is_not_blocked_by_preflight() {
        let schema = json!({
            "required": ["tool_calls"],
            "properties": {
                "tool_calls": {
                    "type": "array",
                    "items": {
                        "required": ["tool", "parameters"],
                        "properties": {
                            "tool": {"type": "string"},
                            "parameters": {"type": "object"}
                        }
                    }
                }
            }
        });
        let input = json!({"tool_calls": [{"name": "read", "arguments": {"path": "a.rs"}}]});
        assert!(
            validate_call("batch", &schema, &input).is_ok(),
            "batch must accept the `name`/`arguments` aliases it advertises recovery for"
        );
    }

    /// `skill_manage` accepts `skill` as an alias for `name`.
    #[test]
    fn skill_alias_recovery_is_not_blocked_by_preflight() {
        let schema = json!({
            "properties": {
                "action": {"type": "string"},
                "name": {"type": "string"}
            }
        });
        let input = json!({"action": "load", "skill": "bugbounty"});
        assert!(
            validate_call("skill_manage", &schema, &input).is_ok(),
            "skill_manage must accept the `skill` alias it advertises recovery for"
        );
    }

    /// `alphagrep` accepts `pattern`/`file_path`/`include` aliases for the
    /// `query`/`file`/`glob` fields it advertises.
    #[test]
    fn alphagrep_alias_recovery_is_not_blocked_by_preflight() {
        let schema = json!({
            "properties": {
                "query": {"type": "string"},
                "file": {"type": "string"},
                "glob": {"type": "string"}
            }
        });
        let input = json!({"pattern": "foo", "file_path": "a.rs", "include": "*.rs"});
        assert!(
            validate_call("alphagrep", &schema, &input).is_ok(),
            "alphagrep must accept the `pattern`/`file_path`/`include` aliases it advertises recovery for"
        );
    }
}
