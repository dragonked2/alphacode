//! Validate required fields before hooks, network permits, or tool side effects.
//! Scalar coercion remains the tool's responsibility: several native tools
//! deliberately accept provider-generated string forms of numbers and booleans.
use serde_json::Value;

/// Repair unambiguous provider scalar encodings using the live tool contract.
/// Do not descend into request bodies or coerce unions that also accept strings.
pub(super) fn normalize_scalar_arguments(schema: &Value, input: &mut Value) {
    let (Some(properties), Some(arguments)) =
        (schema["properties"].as_object(), input.as_object_mut())
    else {
        return;
    };
    for (name, property) in properties {
        let Some(value) = arguments.get_mut(name) else {
            continue;
        };
        let Some(text) = value.as_str().map(str::trim) else {
            continue;
        };
        let replacement = match property["type"].as_str() {
            Some("integer") => text
                .parse::<i64>()
                .map(Value::from)
                .or_else(|_| text.parse::<u64>().map(Value::from))
                .ok(),
            Some("boolean") => match text {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                _ => None,
            },
            _ => None,
        };
        if let Some(replacement) = replacement {
            *value = replacement;
        }
    }
}

/// Recover a provider that serialized a schema-declared array/object as a
/// JSON string. This is unambiguous only when the property does not also allow
/// strings; ordinary string fields and request bodies are left untouched.
/// Returns the number of top-level fields repaired.
pub(super) fn normalize_structured_arguments(schema: &Value, input: &mut Value) -> usize {
    let (Some(properties), Some(arguments)) =
        (schema["properties"].as_object(), input.as_object_mut())
    else {
        return 0;
    };

    let mut repaired = 0;
    for (name, property) in properties {
        let Some(value) = arguments.get_mut(name) else {
            continue;
        };
        let Some(text) = value.as_str() else {
            continue;
        };
        let Some(types) = property["type"].as_array() else {
            if !matches!(property["type"].as_str(), Some("array" | "object")) {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<Value>(text)
                && ((property["type"] == "array" && parsed.is_array())
                    || (property["type"] == "object" && parsed.is_object()))
            {
                *value = parsed;
                repaired += 1;
            }
            continue;
        };

        // A union that accepts a string is intentionally not rewritten.
        if types.iter().any(|kind| kind.as_str() == Some("string")) {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        let parsed_type = if parsed.is_array() {
            "array"
        } else if parsed.is_object() {
            "object"
        } else if parsed.is_null() {
            "null"
        } else {
            continue;
        };
        if types.iter().any(|kind| kind.as_str() == Some(parsed_type)) {
            *value = parsed;
            repaired += 1;
        }
    }
    repaired
}

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
        // These tools perform their own alias/required-field recovery, but a
        // value whose declared structural type is wrong is still not safely
        // recoverable by the tool parser. Check it without imposing the
        // schema's stricter required-key list.
        return validate_structured_types(schema, input, "$", 0);
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
    validate_one_structured_type(schema, input, path)?;
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

fn validate_structured_types(
    schema: &Value,
    input: &Value,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    if depth >= 64 {
        return Ok(());
    }
    validate_one_structured_type(schema, input, path)?;
    if let (Some(object), Some(properties)) = (input.as_object(), schema["properties"].as_object())
    {
        for (key, child_schema) in properties {
            if let Some(value) = object.get(key) {
                validate_structured_types(
                    child_schema,
                    value,
                    &format!("{path}.{key}"),
                    depth + 1,
                )?;
            }
        }
    }
    if let Some(items) = input.as_array() {
        let item_schema = &schema["items"];
        if item_schema.is_object() {
            for (index, item) in items.iter().enumerate() {
                validate_structured_types(
                    item_schema,
                    item,
                    &format!("{path}[{index}]"),
                    depth + 1,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_one_structured_type(schema: &Value, input: &Value, path: &str) -> Result<(), String> {
    let allowed: Vec<&str> = match &schema["type"] {
        Value::String(kind) => vec![kind.as_str()],
        Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => return Ok(()),
    };
    if !allowed
        .iter()
        .any(|kind| matches!(*kind, "array" | "object"))
    {
        return Ok(());
    }
    // `Option<T>` inputs are often represented by schemas that omit `null`.
    // Preserve serde's established acceptance of null for absent optional
    // fields instead of making the preflight stricter than the tool parser.
    if input.is_null() {
        return Ok(());
    }
    let actual_matches = allowed.iter().any(|kind| match *kind {
        "array" => input.is_array(),
        "object" => input.is_object(),
        "string" => input.is_string(),
        "boolean" => input.is_boolean(),
        "integer" => input.as_i64().is_some() || input.as_u64().is_some(),
        "number" => input.is_number(),
        "null" => input.is_null(),
        _ => false,
    });
    if !actual_matches {
        let expected = allowed
            .iter()
            .map(|kind| match *kind {
                "array" => "JSON array",
                "object" => "JSON object",
                other => other,
            })
            .collect::<Vec<_>>()
            .join(" or ");
        return Err(format!(
            "{path}: expected {expected}, received {}; pass structured data as an unquoted JSON array/object, not a quoted JSON string",
            crate::alphacode_app_core::tool::json_type_name(input)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalar_repair_is_lossless_and_limited_to_declared_top_level_fields() {
        let schema = json!({"properties": {
            "limit":{"type":"integer"},"enabled":{"type":"boolean"},
            "query":{"type":"string"},"union":{"type":["integer","string"]},
            "input":{"type":"object","properties":{"limit":{"type":"integer"}}}
        }});
        let mut input = json!({"limit":" 50 ","enabled":"false","query":"50",
            "union":"50","input":{"limit":"50"},"unknown":"50"});
        normalize_scalar_arguments(&schema, &mut input);
        assert_eq!(
            input,
            json!({"limit":50,"enabled":false,"query":"50",
            "union":"50","input":{"limit":"50"},"unknown":"50"})
        );
        for invalid in ["1.5", "1e2", "18446744073709551616", "NaN"] {
            let mut input = json!({"limit":invalid});
            normalize_scalar_arguments(&schema, &mut input);
            assert_eq!(input["limit"], invalid);
        }
    }

    #[test]
    fn repairs_a_stringified_browser_form_fields_array_from_its_schema() {
        let schema = json!({
            "required": ["action"],
            "properties": {
                "action": {"type": "string"},
                "fields": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["selector", "value"],
                        "properties": {
                            "selector": {"type": "string"},
                            "value": {"type": "string"}
                        }
                    }
                }
            }
        });
        let mut input = json!({
            "action": "fill_form",
            "fields": "[{\"selector\":\"input[name=\\\"display_name\\\"]\",\"value\":\"Casey Example\"}]"
        });

        assert_eq!(normalize_structured_arguments(&schema, &mut input), 1);
        assert!(input["fields"].is_array());
        assert_eq!(input["fields"][0]["value"], "Casey Example");
        assert!(validate_call("browser", &schema, &input).is_ok());
    }

    #[test]
    fn rejects_an_unparseable_structured_string_with_the_argument_path() {
        let schema = json!({
            "required": ["action"],
            "properties": {
                "action": {"type": "string"},
                "fields": {"type": "array"}
            }
        });
        let mut input = json!({"action": "fill_form", "fields": "not-json"});
        assert_eq!(normalize_structured_arguments(&schema, &mut input), 0);
        let error = validate_call("browser", &schema, &input).unwrap_err();
        assert!(error.contains("$.fields"), "{error}");
        assert!(error.contains("JSON array"), "{error}");
        assert!(error.contains("received a string"), "{error}");
        assert!(
            !error.contains("not-json"),
            "do not echo user data: {error}"
        );
    }

    #[test]
    fn does_not_parse_a_union_that_legitimately_accepts_strings() {
        let schema = json!({"properties": {
            "payload": {"type": ["string", "array"], "items": {"type": "string"}}
        }});
        let mut input = json!({"payload": "[\"literal string\"]"});
        assert_eq!(normalize_structured_arguments(&schema, &mut input), 0);
        assert_eq!(input["payload"], "[\"literal string\"]");
        assert!(validate_structured_types(&schema, &input, "$", 0).is_ok());
    }

    #[test]
    fn optional_structured_fields_still_accept_null() {
        let schema = json!({"properties": {"fields": {"type": "array"}}});
        assert!(validate_call("browser", &schema, &json!({"fields": null})).is_ok());
    }

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
