mod agentgrep;
mod amass;
pub mod ambient;
mod anew;
mod apply_patch;
mod assetfinder;
mod bash;
mod batch;
mod bg;
mod browser;
mod cariddi;
mod clipboard;
mod communicate;
#[cfg(target_os = "macos")]
mod computer;
mod conversation_search;
mod corsy;
mod crlfuzz;
mod cron;
mod ctf_tools;
mod dalfox;
mod debug_socket;
pub mod desktop;
pub(crate) mod diff_utils;
mod discover;
mod dnsx;
mod doctor;
mod edit;
mod feroxbuster;
mod ffuf;
mod gau;
mod gf;
mod gmail;
mod goal;
mod gobuster;
mod gospider;
mod hakrawler;
mod httpflow;
mod httprobe;
mod httpx;
mod invalid;
mod jwt;
mod katana;
mod kxss;
mod ls;
pub mod mcp;
mod meg;
mod memory;
mod multiedit;
mod naabu;
mod nikto;
mod nmap;
mod nuclei;
mod open;
mod patch;
mod plan;
mod python;
mod qsreplace;
mod read;
pub mod recon_common;
mod repeat_guard;
mod scrapling;
mod self_improve;
pub mod selfdev;
pub(crate) mod serde_coerce;
mod session_search;
pub(crate) mod session_search_index;
mod side_panel;
mod skill;
mod sqlmap;
mod subfinder;
mod todo;
mod unfurl;
mod waybackurls;
mod webfetch;
mod websearch;
mod write;

use crate::alphacode_app_core::compaction::CompactionManager;
use crate::alphacode_app_core::provider::Provider;
use crate::alphacode_app_core::skill::SkillRegistry;
use crate::alphacode_message_types::ToolDefinition;
use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::{LazyLock, RwLock as StdRwLock};
use tokio::sync::RwLock;

pub(crate) use crate::alphacode_tool_core::intent_schema_property;
pub use crate::alphacode_tool_core::{
    StdinInputRequest, Tool, ToolContext, ToolExecutionClass, ToolExecutionMode,
};
pub use crate::alphacode_tool_types::{ToolImage, ToolOutput};
pub(crate) use session_search::spawn_recent_index_warmup;

/// Hard ceiling on text returned by any tool. Context-aware truncation below
/// can permit very large outputs when a provider advertises a large window;
/// this independent cap keeps accidental DOM dumps, search results, and logs
/// from consuming an entire turn.
const MAX_TOOL_OUTPUT_CHARS: usize = 30_000;

fn cap_tool_output(mut output: ToolOutput) -> ToolOutput {
    let char_count = output.output.chars().count();
    if char_count <= MAX_TOOL_OUTPUT_CHARS {
        return output;
    }

    // Keep the useful opening context and final summary, while leaving room
    // for a short, actionable notice.
    const NOTICE: &str = "\n\n[Tool output capped at 30000 characters. Use a narrower query or read the result in smaller ranges.]\n\n";
    let keep = MAX_TOOL_OUTPUT_CHARS.saturating_sub(NOTICE.chars().count());
    let head_chars = keep.saturating_mul(4) / 5;
    let tail_chars = keep.saturating_sub(head_chars);
    let head: String = output.output.chars().take(head_chars).collect();
    let tail: String = output
        .output
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    output.output = format!("{head}{NOTICE}{tail}");
    output.title = Some(match output.title.take() {
        Some(title) => format!("{title} · output capped"),
        None => "output capped".to_string(),
    });
    output
}

#[derive(Clone, Debug, Default)]
struct SessionToolPolicy {
    allowed_tools: Option<HashSet<String>>,
    disabled_tools: HashSet<String>,
}

static SESSION_TOOL_POLICIES: LazyLock<StdRwLock<HashMap<String, SessionToolPolicy>>> =
    LazyLock::new(|| StdRwLock::new(HashMap::new()));

pub(crate) fn set_session_tool_policy(
    session_id: &str,
    allowed_tools: Option<HashSet<String>>,
    disabled_tools: HashSet<String>,
) {
    let mut policies = SESSION_TOOL_POLICIES
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    policies.insert(
        session_id.to_string(),
        SessionToolPolicy {
            allowed_tools,
            disabled_tools,
        },
    );
}

pub(crate) fn clear_session_tool_policy(session_id: &str) {
    let mut policies = SESSION_TOOL_POLICIES
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    policies.remove(session_id);
    repeat_guard::clear_session(session_id);
}

fn session_tool_policy(session_id: &str) -> Option<SessionToolPolicy> {
    SESSION_TOOL_POLICIES
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(session_id)
        .cloned()
}

/// Append a tool-specific "next step" hint to an error before it is stored in
/// the transcript and shown to the model. The unknown-tool error in
/// [`Registry::execute`] already proved this pattern stops hallucination
/// spirals (#104): models recover in one retry when the error names the fix.
/// Hints are capped at one short line each so the history does not fill with
/// coaching text.
/// Input/schema failures are recoverable by changing the call shape. They
/// should remain visible to the model without exhausting the repeat guard,
/// which is reserved for executions that failed after a valid request was
/// dispatched.
pub(crate) fn is_input_validation_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("missing field")
        || message.contains("expects a json object")
        || message.contains("invalid request")
        || message.contains("is required")
        || message.contains("requires a non-empty")
        || message.contains("unsupported browser action")
        || message.contains("unknown action")
}

/// Describe what a malformed tool call actually contained, for the
/// "missing field X" family of errors.
///
/// This used to be copy-pasted into five tools, and every copy rendered
/// `Received keys: .` for the *most common* malformed call — an empty object,
/// which is exactly what a model emits when it calls a tool with no arguments
/// at all (or when argument streaming was truncated). That sentence gives the
/// model nothing to act on, so it repeated the same broken call until the
/// repeat guard blocked it.
///
/// The three cases are worth distinguishing because they need different fixes:
/// - no arguments at all -> "call the tool again with its required fields"
/// - wrong JSON type -> "arguments must be a JSON object"
/// - right shape, wrong key -> list the keys so the model can see the typo
pub(crate) fn describe_received_arguments(input: &Value) -> String {
    match input {
        Value::Object(map) if map.is_empty() => {
            "The call carried no arguments at all (an empty JSON object). \
             Re-issue it with the tool's required fields filled in."
                .to_string()
        }
        Value::Object(map) => {
            // The truncation marker is bookkeeping we injected, not something the
            // model sent. Listing it as a "received key" invites the model to
            // start supplying it, and it crowds out the one key it actually got
            // wrong.
            let mut keys: Vec<&str> = map
                .keys()
                .map(String::as_str)
                .filter(|key| *key != crate::alphacode_message_types::TRUNCATED_INPUT_MARKER)
                .collect();
            keys.sort_unstable();
            if keys.is_empty() {
                return "The call carried no arguments of its own — only an internal \
                        truncation marker. The arguments were cut off before they finished \
                        arriving; re-send the call, splitting a large value across smaller \
                        steps."
                    .to_string();
            }
            let listed = keys
                .iter()
                .map(|key| format!("`{key}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Received keys: {listed}. Match those names to the tool's \
                 required fields, or re-check the schema for the exact spelling."
            )
        }
        Value::Null => {
            "The call carried no arguments (null). Re-issue it with the required fields."
                .to_string()
        }
        other => format!(
            "Expected a JSON object of arguments, got {}. \
             Re-issue the call as a single JSON object with the required fields.",
            json_type_name(other)
        ),
    }
}

/// Object keys a model plausibly sends the target URL under when it does not
/// use the schema's `url`. Matched ASCII-case-insensitively, so `URL`, `Url`
/// and `url` all resolve to the same entry.
const URL_ARG_ALIASES: &[&str] = &[
    "url",
    "uri",
    "link",
    "href",
    "target",
    "target_url",
    "request_url",
    "url_or_path",
    "address",
    "endpoint",
    "page",
    "site",
    "website",
    "host",
    "hostname",
    "domain",
    "resource",
    "location",
    "u",
];

/// Whether `value` is usable verbatim as a request target, stripping the
/// quoting characters models add when they echo a URL back from context.
fn url_candidate(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_matches(|c| matches!(c, '`' | '<' | '>' | '"' | '\'' | ' ' | '\t'))
        .trim();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")).then(|| trimmed.to_string())
}

/// First `http(s)://…` token embedded anywhere in `text`.
///
/// Recovers a target from truncated or garbled arguments — the common failure
/// when a provider's streamed tool-call JSON is cut off mid-string.
fn url_in_text(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let start = match (lower.find("https://"), lower.find("http://")) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let candidate = &text[start..];
    let end = candidate
        .find(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '"' | '\'' | '`' | '<' | '>' | '}' | ']' | ')' | '(' | ',' | '\\'
                )
        })
        .unwrap_or(candidate.len());
    // Prose trails the URL with sentence punctuation ("... see https://x.com."),
    // which is not part of the address.
    let candidate = candidate[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
    url_candidate(candidate)
}

fn missing_url_error(tool: &str, input: &Value) -> anyhow::Error {
    missing_field_error(tool, "url", URL_EXAMPLE, input)
}

/// Recover the target URL from loosely-shaped tool input.
///
/// The strict path is a bare `serde_json::from_value`, which fails with
/// "missing field `url`" for every argument shape a model actually produces:
/// a different key name (`uri`, `target`, `href`, …), a bare URL string where
/// an object was expected, a single-element array wrapper, or a payload
/// truncated mid-JSON by the provider's streaming decoder. Each of those is a
/// recoverable typo, not a user error, and failing them wastes a whole agent
/// turn.
///
/// Salvage ladder, most trustworthy first:
/// 1. the documented `url` key, accepted verbatim (it is unambiguous, so a
///    scheme-less value still gets through and is normalized by the caller);
/// 2. known alias keys, but only when the value really is an http(s) URL;
/// 3. *any* string value in the object that is an http(s) URL;
/// 4. a bare or embedded URL when the whole payload is a string.
pub(crate) fn coerce_url_arg(input: &Value, tool: &str) -> Result<String> {
    match input {
        Value::Object(map) => {
            if let Some(raw) = map.get("url").and_then(Value::as_str)
                && !raw.trim().is_empty()
            {
                // `url` is unambiguous, so a value that is already a target is
                // used as-is. Stripping the backticks and quotes a model adds
                // when it echoes a URL back out of its own context is still
                // worth doing: those would otherwise be sent to the transport.
                if let Some(url) = url_candidate(raw) {
                    return Ok(url);
                }
                return Ok(raw.trim().to_string());
            }
            for (key, value) in map {
                let Some(text) = value.as_str() else {
                    continue;
                };
                if !URL_ARG_ALIASES.contains(&key.to_ascii_lowercase().as_str()) {
                    continue;
                }
                if let Some(url) = url_candidate(text) {
                    return Ok(url);
                }
            }
            // Last resort inside the object: the model picked an unusual key
            // (`"full_link"`, `"page_url"`, …). Any field that *is* a URL is
            // unambiguous enough to act on. `serde_json::Map` is a `BTreeMap`
            // here, so this is deterministic rather than insertion-ordered.
            for value in map.values() {
                if let Some(url) = value.as_str().and_then(url_candidate) {
                    return Ok(url);
                }
            }
            Err(missing_url_error(tool, input))
        }
        Value::Array(items) if items.len() == 1 => coerce_url_arg(&items[0], tool),
        Value::String(text) => url_candidate(text)
            .or_else(|| url_in_text(text))
            .ok_or_else(|| missing_url_error(tool, input)),
        _ => Err(missing_url_error(tool, input)),
    }
}

/// Object keys a model plausibly uses for a bare host target (`nmap`,
/// `subfinder`, `amass`, …) when it does not use the schema's field name.
const HOST_ARG_ALIASES: &[&str] = &[
    "target",
    "host",
    "hostname",
    "domain",
    "ip",
    "address",
    "t",
    "d",
    "host_name",
    "site",
    "server",
    "addr",
    "url",
    "uri",
    "endpoint",
];

/// Recover a bare host target (`example.com`, `10.0.0.1`, `example.com:8443`)
/// from loosely-shaped tool input.
///
/// Shares the intent of [`coerce_url_arg`] but deliberately does *not*
/// require an `http(s)` scheme: recon tools take a host, and a model that sends
/// `{"hosts": "example.com"}` or a bare `example.com` string means the host.
/// A value carrying a scheme is unwrapped to its authority, since
/// `validate_hostname` rejects colons and slashes.
pub(crate) fn coerce_host_arg(input: &Value, tool: &str, field: &str) -> Result<String> {
    let bare = |value: &str| -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > 4096 || trimmed.contains(char::is_whitespace) {
            return None;
        }
        // `https://example.com/path` -> `example.com`
        if let Some(rest) = trimmed
            .strip_prefix("http://")
            .or_else(|| trimmed.strip_prefix("https://"))
        {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
            return (!authority.is_empty()).then(|| authority.to_string());
        }
        let valid = trimmed.contains('.') || trimmed.contains(':') || trimmed.starts_with('*');
        valid.then(|| trimmed.to_string())
    };

    match input {
        Value::Object(map) => {
            if let Some(raw) = map.get(field).and_then(Value::as_str)
                && let Some(host) = bare(raw)
            {
                return Ok(host);
            }
            for (key, value) in map {
                let Some(text) = value.as_str() else {
                    continue;
                };
                if !HOST_ARG_ALIASES.contains(&key.to_ascii_lowercase().as_str()) {
                    continue;
                }
                if let Some(host) = bare(text) {
                    return Ok(host);
                }
            }
            for value in map.values() {
                if let Some(host) = value.as_str().and_then(bare) {
                    return Ok(host);
                }
            }
            Err(missing_field_error(tool, field, HOST_EXAMPLE, input))
        }
        Value::Array(items) if items.len() == 1 => coerce_host_arg(&items[0], tool, field),
        Value::String(text) => bare(text)
            .or_else(|| url_in_text(text).and_then(|u| bare(&u)))
            .ok_or_else(|| missing_field_error(tool, field, HOST_EXAMPLE, input)),
        // Anything else (a number, a bool, an array) cannot name a host, but
        // the fix-it line must still be a host or the model retries the same
        // number under a different key.
        _ => Err(missing_field_error(tool, field, HOST_EXAMPLE, input)),
    }
}

/// Build the "missing field X" error for one field.
///
/// `example` is the value to show in the fix-it line, because the most useful
/// example is usually field-specific: `webfetch` is told to send a URL, while
/// `read` is told to send a path. `"..."` would tell the model nothing it did
/// not already have from the schema.
fn missing_field_error(tool: &str, field: &str, example: &str, input: &Value) -> anyhow::Error {
    // Keep the literal "missing field" wording: `is_input_validation_error`
    // keys off it to keep a malformed call from burning the repeat guard.
    anyhow::anyhow!(
        "{tool}: missing field `{field}`. Send {{\"{field}\": {example}}} — the value must be a \
         non-empty string under the `{field}` key. {}",
        describe_received_arguments(input)
    )
}

/// A URL-shaped example for the `webfetch`/`unfurl` error line.
const URL_EXAMPLE: &str = "\"https://example.com\"";

/// A path-shaped example for the file tools' error line.
const PATH_EXAMPLE: &str = "\"/path/to/file\"";

/// A host-shaped example for the recon tools' error line.
const HOST_EXAMPLE: &str = "\"example.com\"";

/// A shell-command-shaped example for `bash`'s error line.
pub(crate) const COMMAND_EXAMPLE: &str = "\"ls -la\"";

/// Recover a required free-text argument from loosely-shaped tool input.
///
/// The generalisation of [`coerce_url_arg`] for arguments that are neither a
/// URL nor a host: `websearch`'s `query`, `read`'s `file_path`, and so on. The
/// documented key is accepted verbatim; the caller's `extra` aliases and then
/// any single string value are tried in turn, and a bare string payload is
/// treated as the value itself.
///
/// The single-string last resort is only safe when there is exactly one string
/// field, because it cannot tell which field was meant otherwise. Callers that
/// need two text arguments from one payload (`write`'s path and body) must use
/// [`coerce_text_field`], which never guesses.
pub(crate) fn coerce_text_arg(
    input: &Value,
    tool: &str,
    field: &str,
    extra: &[&str],
) -> Result<String> {
    coerce_text_field(input, tool, field, extra, true, PATH_EXAMPLE)
}

/// [`coerce_text_arg`] without the single-string fallback.
///
/// `write` reads two text arguments out of one payload, so the fallback is not
/// merely imprecise there — it is destructive. Given `{"file_path": "a.rs"}`
/// (a call whose body was cut off), the "exactly one string field" rule hands
/// back `"a.rs"` as the *content*, and the tool writes a file containing its own
/// path. There is no value of `allow_single_string` that makes that safe, so
/// this variant is the one a multi-field caller must use.
pub(crate) fn coerce_text_field(
    input: &Value,
    tool: &str,
    field: &str,
    extra: &[&str],
    allow_single_string: bool,
    example: &str,
) -> Result<String> {
    let usable = |value: &str| -> Option<String> {
        let trimmed = value.trim();
        (!trimmed.is_empty() && trimmed.len() <= 32_768).then(|| trimmed.to_string())
    };
    let lookup = |map: &serde_json::Map<String, Value>, key: &str| -> Option<String> {
        map.get(key)
            .and_then(Value::as_str)
            .and_then(usable)
            .or_else(|| {
                // `URL` and `Url` should reach the same entry as `url`.
                map.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .and_then(|(_, v)| v.as_str())
                    .and_then(usable)
            })
    };

    match input {
        Value::Object(map) => {
            if let Some(value) = lookup(map, field) {
                return Ok(value);
            }
            for alias in extra {
                if let Some(value) = lookup(map, alias) {
                    return Ok(value);
                }
            }
            // Last resort inside the object: exactly one string field. With two
            // or more there is no way to tell which was meant, and guessing
            // would send the wrong value rather than an actionable error.
            if allow_single_string {
                let mut strings = map.values().filter_map(Value::as_str).filter_map(usable);
                if let (Some(only), None) = (strings.next(), strings.next()) {
                    return Ok(only);
                }
            }
            Err(missing_field_error(tool, field, example, input))
        }
        Value::Array(items) if items.len() == 1 => {
            coerce_text_field(&items[0], tool, field, extra, allow_single_string, example)
        }
        Value::String(text) => {
            usable(text).ok_or_else(|| missing_field_error(tool, field, example, input))
        }
        _ => Err(missing_field_error(tool, field, example, input)),
    }
}

/// Recover a required shell-command argument, without ever guessing.
///
/// `bash` carries several optional string fields (`intent`, `justification`), so
/// the single-string fallback is unsafe here in a way it is not for a
/// single-field tool: `{"intent": "list the src directory"}` would otherwise be
/// executed as a command. Only the documented key, its aliases, and an
/// explicitly-shaped payload are accepted.
pub(crate) fn coerce_command_field(input: &Value, tool: &str) -> Result<String> {
    coerce_text_field(
        input,
        tool,
        "command",
        &["cmd", "commands", "script", "shell"],
        false,
        COMMAND_EXAMPLE,
    )
}

/// Human-readable JSON type name for an error message.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Always-on prompt budget for one tool description, in estimated tokens.
///
/// Shared with `tool_descriptions_stay_under_token_cap` so the generator and
/// its guard can never drift apart. Tool descriptions are sent on every
/// request, so this is a hard budget, not a guideline.
pub(crate) const TOOL_DESCRIPTION_TOKEN_CAP: usize = 250;

/// One short sentence naming a tool's mandatory arguments, for appending to
/// its description.
///
/// Returns `None` when the tool declares no required fields, when the
/// description already names every field, or — importantly — when appending
/// would push the description over [`TOOL_DESCRIPTION_TOKEN_CAP`]. The budget
/// wins: a tool whose own guidance already fills the cap gets the schema's
/// `required` array (which every provider honours to some degree) but no
/// extra prose.
fn required_argument_contract(schema: &Value, description: &str) -> Option<String> {
    let required = schema.get("required")?.as_array()?;
    if required.is_empty() {
        return None;
    }
    let mut names = Vec::with_capacity(required.len());
    for name in required {
        names.push(name.as_str()?);
    }

    // If the prose already spells every field out, adding it again is noise.
    if names.iter().all(|name| description.contains(name)) {
        return None;
    }

    let listed = names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let sentence = format!(" Required: {listed}.");

    let projected =
        crate::util::estimate_tokens(description) + crate::util::estimate_tokens(&sentence);
    if projected > TOOL_DESCRIPTION_TOKEN_CAP {
        return None;
    }
    Some(sentence)
}

/// Build a "here is how to call this tool" line from the tool's own schema.
///
/// Nearly every tool deserializes its input with a bare
/// `serde_json::from_value`, so a malformed call surfaces as a raw serde
/// complaint ("missing field `file_path`") that names one field at a time and
/// never shows the model the other required ones. Models then retry field by
/// field, one failure per attempt. Reading the required set straight from the
/// schema means the *first* error can state the whole contract, and it stays
/// correct automatically as schemas change.
pub(crate) fn schema_call_hint(schema: &Value) -> Option<String> {
    let properties = schema.get("properties")?.as_object()?;
    let required = schema.get("required")?.as_array()?;

    let mut parts = Vec::new();
    for name in required {
        let name = name.as_str()?;
        let spec = properties.get(name);
        let kind = spec
            .and_then(|spec| spec.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("string");
        let sample = sample_for_kind(kind);
        parts.push(format!("{name}: {sample}"));
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "Required arguments: {}. Example: {}",
        parts.join(", "),
        render_example(parts)
    ))
}

/// A short, obviously-fake placeholder for a JSON type.
fn sample_for_kind(kind: &str) -> &'static str {
    match kind {
        "integer" | "number" => "0",
        "boolean" => "false",
        "array" => "[]",
        "object" => "{}",
        _ => "\"...\"",
    }
}

/// Render the required-argument list back into a JSON object literal, quoting
/// the field names so the model can copy the shape exactly.
fn render_example(parts: Vec<String>) -> String {
    let mut out = String::from("{");
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let (name, value) = part.split_once(": ").unwrap_or((part.as_str(), "\"...\""));
        out.push('"');
        out.push_str(name.trim());
        out.push_str("\": ");
        out.push_str(value);
    }
    out.push('}');
    out
}

pub(crate) fn agent_facing_error(tool_name: &str, error: &anyhow::Error) -> String {
    let base = error.to_string();
    let hint = match tool_name {
        "edit" | "multiedit" => {
            if base.contains("not found") || base.contains("No match") {
                Some(
                    "Re-read the exact file section first and match the old_string character-for-character (whitespace, indentation, line endings).",
                )
            } else {
                None
            }
        }
        "bash" => {
            if base.contains("exit code") || base.contains("exit status") {
                Some(
                    "Read the command's stderr above and fix the cause; do not re-run the identical command unchanged.",
                )
            } else if base.contains("missing field") {
                Some(
                    "The tool call is missing required fields. For bash, provide {\"command\": \"your command here\"}.",
                )
            } else {
                None
            }
        }
        "read" => {
            if base.contains("binary") || base.contains("too large") || base.contains("oversized") {
                Some(
                    "Use offset/limit to read a smaller range, or grep to locate the relevant lines first.",
                )
            } else if base.contains("missing field") {
                Some(
                    "The tool call is missing required fields. For read, provide {\"file_path\": \"/path/to/file\"}.",
                )
            } else {
                None
            }
        }
        "write" => {
            if base.contains("permission")
                || base.contains("read-only")
                || base.contains("readonly")
            {
                Some(
                    "Check whether the path is outside the working directory or read-only; ask the user before writing outside the project.",
                )
            } else if base.contains("missing field") {
                Some(
                    "The tool call is missing required fields. For write, provide {\"file_path\": \"/path/to/file\", \"content\": \"file content\"}.",
                )
            } else {
                None
            }
        }
        "apply_patch" | "patch" => {
            if base.contains("permission")
                || base.contains("read-only")
                || base.contains("readonly")
            {
                Some(
                    "Check whether the path is outside the working directory or read-only; ask the user before writing outside the project.",
                )
            } else if base.contains("missing field") {
                Some(
                    "The tool call is missing required fields. For apply_patch, provide {\"patch_text\": \"*** Begin Patch\\n*** Update File: path/to/file\\n@@\\n- old\\n+ new\\n*** End Patch\"}.",
                )
            } else {
                None
            }
        }
        "webfetch" | "websearch" | "scrapling" => {
            if base.contains("missing field") {
                Some(
                    "The tool call is missing required fields. For webfetch, provide \
                     {\"url\": \"https://example.com\"} as a single JSON object.",
                )
            } else if base.contains("timeout")
                || base.contains("timed out")
                || base.contains("connect")
            {
                Some(
                    "The site may be unreachable; retry once, then report the failure and continue with other work.",
                )
            } else {
                None
            }
        }
        _ => {
            if base.contains("missing field") {
                Some(
                    "The tool call is missing required fields. Check the tool's JSON schema for required parameters.",
                )
            } else {
                None
            }
        }
    };
    match hint {
        Some(hint) => format!("Error: {base}\nHint: {hint}"),
        None => format!("Error: {base}"),
    }
}

/// A model-supplied tool name resolved against the live tool registry.
///
/// `name` is the registry key that will be executed. `action` is set when the
/// model encoded the action in the tool name (`desktop_find`,
/// `desktop.snapshot`) instead of passing the tool's `action` parameter.
struct ResolvedToolCall {
    name: String,
    action: Option<String>,
}

/// Registry of available tools (Arc-wrapped for sharing)
///
/// Clone creates a fresh CompactionManager so each subagent gets independent
/// message history tracking. Tools and skills are shared via Arc.
pub struct Registry {
    tools: Arc<RwLock<HashMap<String, Arc<dyn Tool>>>>,
    skills: Arc<RwLock<SkillRegistry>>,
    compaction: Arc<RwLock<CompactionManager>>,
}

impl Clone for Registry {
    fn clone(&self) -> Self {
        Self {
            tools: self.tools.clone(),
            skills: self.skills.clone(),
            // Each clone gets a fresh CompactionManager to prevent parallel
            // subagents from corrupting each other's message history
            compaction: Arc::new(RwLock::new(CompactionManager::new())),
        }
    }
}

impl Registry {
    fn shared_skills_registry() -> Arc<RwLock<SkillRegistry>> {
        SkillRegistry::shared_registry()
    }

    fn insert_tool<T>(tools: &mut HashMap<String, Arc<dyn Tool>>, name: &str, tool: T)
    where
        T: Tool + 'static,
    {
        tools.insert(name.into(), Arc::new(tool) as Arc<dyn Tool>);
    }

    fn insert_tool_timed<T>(
        tools: &mut HashMap<String, Arc<dyn Tool>>,
        timings: &mut Vec<(String, u128)>,
        name: &str,
        make_tool: impl FnOnce() -> T,
    ) where
        T: Tool + 'static,
    {
        let start = std::time::Instant::now();
        Self::insert_tool(tools, name, make_tool());
        timings.push((name.to_string(), start.elapsed().as_millis()));
    }

    /// Create a lightweight empty registry (no tools, no skill loading).
    /// Used by remote-mode clients that don't execute tools locally.
    pub fn empty() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            skills: Arc::new(RwLock::new(SkillRegistry::default())),
            compaction: Arc::new(RwLock::new(CompactionManager::new())),
        }
    }

    /// Base tools that are stateless and can be shared across sessions.
    /// Created once and cached in a OnceLock, then cloned (cheap Arc bumps) per session.
    fn base_tools(skills: &Arc<RwLock<SkillRegistry>>) -> HashMap<String, Arc<dyn Tool>> {
        use std::sync::OnceLock;
        static BASE: OnceLock<HashMap<String, Arc<dyn Tool>>> = OnceLock::new();
        let base = BASE.get_or_init(|| {
            let init_start = std::time::Instant::now();
            let mut timings = Vec::new();
            let mut m = HashMap::new();
            Self::insert_tool_timed(&mut m, &mut timings, "read", read::ReadTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "write", write::WriteTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "agentgrep",
                agentgrep::AgentGrepTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "side_panel",
                side_panel::SidePanelTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "edit", edit::EditTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "multiedit",
                multiedit::MultiEditTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "patch", patch::PatchTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "apply_patch",
                apply_patch::ApplyPatchTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "ls", ls::LsTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "bash", bash::BashTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "browser", browser::BrowserTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "open", open::OpenTool::new);
            #[cfg(target_os = "macos")]
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "macos_computer_use",
                computer::ComputerTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "webfetch",
                webfetch::WebFetchTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "websearch",
                websearch::WebSearchTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "scrapling",
                scrapling::ScraplingTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "httpflow",
                httpflow::HttpFlowTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "subfinder",
                subfinder::SubfinderTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "httpx", httpx::HttpxTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "waybackurls",
                waybackurls::WaybackurlsTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "gau", gau::GauTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "katana", katana::KatanaTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "ffuf", ffuf::FfufTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "dnsx", dnsx::DnsxTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "nuclei", nuclei::NucleiTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "naabu", naabu::NaabuTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "amass", amass::AmassTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "assetfinder",
                assetfinder::AssetfinderTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "gobuster",
                gobuster::GobusterTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "sqlmap", sqlmap::SqlmapTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "nmap", nmap::NmapTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "nikto", nikto::NiktoTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "unfurl", unfurl::UnfurlTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "meg", meg::MegTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "gf", gf::GfTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "feroxbuster",
                feroxbuster::FeroxbusterTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "hakrawler",
                hakrawler::HakrawlerTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "gospider",
                gospider::GospiderTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "dalfox", dalfox::DalfoxTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "kxss", kxss::KxssTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "corsy", corsy::CorsyTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "crlfuzz", crlfuzz::CrlfuzzTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "cariddi", cariddi::CariddiTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "httprobe",
                httprobe::HttprobeTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "anew", anew::AnewTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "qsreplace",
                qsreplace::QsreplaceTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "jwt", || jwt::JwtTool);
            Self::insert_tool_timed(&mut m, &mut timings, "invalid", invalid::InvalidTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "todo", todo::TodoTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "bg", bg::BgTool::new);
            // CTF-specific tools
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "pwntools",
                ctf_tools::PwntoolsTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "binwalk", ctf_tools::BinwalkTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "steghide",
                ctf_tools::SteghideTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "radare2", ctf_tools::Radare2Tool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "z3", ctf_tools::Z3Tool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "exiftool",
                ctf_tools::ExiftoolTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "volatility",
                ctf_tools::VolatilityTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "tshark", ctf_tools::TsharkTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "ghidra", ctf_tools::GhidraTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "angr", ctf_tools::AngrTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "hashcat", ctf_tools::HashcatTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "john", ctf_tools::JohnTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "flag_scanner",
                ctf_tools::FlagScannerTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "challenge_classifier",
                ctf_tools::ChallengeClassifierTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "ctf_auto_solver",
                ctf_tools::CtfAutoSolverTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "swarm",
                communicate::CommunicateTool::new,
            );
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "session_search",
                session_search::SessionSearchTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "memory", memory::MemoryTool::new);
            Self::insert_tool_timed(
                &mut m,
                &mut timings,
                "initiative",
                goal::InitiativeTool::new,
            );
            Self::insert_tool_timed(&mut m, &mut timings, "gmail", gmail::GmailTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "schedule", ambient::ScheduleTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "selfdev", selfdev::SelfDevTool::new);
            Self::insert_tool_timed(&mut m, &mut timings, "clipboard", || {
                clipboard::ClipboardTool
            });
            Self::insert_tool_timed(&mut m, &mut timings, "cron", || cron::CronTool);
            Self::insert_tool_timed(&mut m, &mut timings, "plan", || plan::PlanModeTool);
            Self::insert_tool_timed(&mut m, &mut timings, "self_improve", || {
                self_improve::SelfImproveTool
            });
            Self::insert_tool_timed(&mut m, &mut timings, "doctor", || doctor::DoctorTool);
            Self::insert_tool_timed(&mut m, &mut timings, "desktop", desktop::DesktopTool::new);
            let nonzero: Vec<String> = timings
                .iter()
                .filter(|(_, ms)| *ms > 0)
                .map(|(name, ms)| format!("{name}={ms}ms"))
                .collect();
            crate::logging::info(&format!(
                "[TIMING] registry_base_tools_init: total={}ms, nonzero=[{}]",
                init_start.elapsed().as_millis(),
                nonzero.join(", ")
            ));
            m
        });
        // Clone the Arc entries (cheap refcount bumps, not deep copies)
        let mut tools = base.clone();
        // SkillTool needs the skills registry reference (shared across sessions)
        Self::insert_tool(
            &mut tools,
            "skill_manage",
            skill::SkillTool::new(skills.clone()),
        );
        tools
    }

    pub async fn new(_provider: Arc<dyn Provider>) -> Self {
        let start = std::time::Instant::now();
        let skills_start = std::time::Instant::now();
        let skills = Self::shared_skills_registry();
        let skills_ms = skills_start.elapsed().as_millis();
        let compaction_start = std::time::Instant::now();
        let compaction = Arc::new(RwLock::new(CompactionManager::new()));
        let compaction_ms = compaction_start.elapsed().as_millis();
        let registry_struct_start = std::time::Instant::now();
        let registry = Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            skills: skills.clone(),
            compaction: compaction.clone(),
        };
        let registry_struct_ms = registry_struct_start.elapsed().as_millis();

        let base_start = std::time::Instant::now();
        let mut tools_map = Self::base_tools(&skills);
        let base_ms = base_start.elapsed().as_millis();

        // Per-session tools that need provider/registry references
        let session_tools_start = std::time::Instant::now();
        Self::insert_tool(
            &mut tools_map,
            "batch",
            batch::BatchTool::new(registry.clone()),
        );
        Self::insert_tool(
            &mut tools_map,
            "conversation_search",
            conversation_search::ConversationSearchTool::new(compaction),
        );
        // Sponsored discovery is on by default (opt-out); when disabled the
        // tool is never registered and no discovery endpoint is ever
        // contacted.
        if crate::config::config().sponsors.enabled {
            Self::insert_tool(
                &mut tools_map,
                "discover_tools",
                discover::DiscoverToolsTool::new(),
            );
        }
        let session_tools_ms = session_tools_start.elapsed().as_millis();

        let write_start = std::time::Instant::now();
        *registry.tools.write().await = tools_map;
        let write_ms = write_start.elapsed().as_millis();
        crate::logging::info(&format!(
            "[TIMING] registry_new: skills={}ms, compaction={}ms, registry_struct={}ms, base_tools={}ms, session_tools={}ms, write={}ms, total={}ms",
            skills_ms,
            compaction_ms,
            registry_struct_ms,
            base_ms,
            session_tools_ms,
            write_ms,
            start.elapsed().as_millis()
        ));
        registry
    }

    /// Get all tool definitions for the API
    pub async fn definitions(
        &self,
        allowed_tools: Option<&HashSet<String>>,
    ) -> Vec<ToolDefinition> {
        let tools = self.tools.read().await;
        let mut defs: Vec<ToolDefinition> = tools
            .iter()
            .filter(|(name, _)| allowed_tools.map(|set| set.contains(*name)).unwrap_or(true))
            .map(|(name, tool)| {
                let mut def = tool.to_definition();
                // Use registry key as the tool name (important for MCP tools where
                // the registry key is "mcp__server__tool" but Tool::name() returns
                // just the raw tool name)
                if def.name != *name {
                    def.name = name.clone();
                }
                // State the mandatory arguments in prose as well as in the
                // JSON schema's `required` array. Providers and models vary in
                // how much weight they give `required`, and a call that omits a
                // mandatory field is the single most common tool error — it
                // costs a full round-trip to discover. Deriving this from each
                // tool's own schema keeps it correct with zero per-tool upkeep.
                if let Some(contract) =
                    required_argument_contract(&def.input_schema, &def.description)
                {
                    def.description.push_str(&contract);
                }
                def
            })
            .collect();

        // Sort by name for deterministic ordering - critical for prompt cache hits
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    pub async fn tool_names(&self) -> Vec<String> {
        let tools = self.tools.read().await;
        tools.keys().cloned().collect()
    }

    /// Enable test mode for memory tools (isolated storage)
    /// Called when session is marked as debug
    pub async fn enable_memory_test_mode(&self) {
        let mut tools = self.tools.write().await;

        // Replace memory tool with test version
        tools.insert(
            "memory".to_string(),
            Arc::new(memory::MemoryTool::new_test()) as Arc<dyn Tool>,
        );

        crate::logging::info("Memory test mode enabled - using isolated storage");
    }

    /// Resolve tool name aliases.
    ///
    /// When using OAuth, the API presents tools with Claude Code names
    /// (e.g. `file_grep`, `shell_exec`). The model uses those names in
    /// sub-tool calls (e.g. inside `batch`), but our registry uses internal
    /// names (`grep`, `bash`). This mapping ensures both forms resolve
    /// correctly.
    ///
    /// The canonical mapping lives in `alphacode-tool-types::resolve_tool_name` so
    /// lower-level crates (e.g. config) can normalize tool names without
    /// depending on the tool subsystem; this method delegates to it.
    pub(crate) fn resolve_tool_name(name: &str) -> &str {
        crate::alphacode_tool_types::resolve_tool_name(name)
    }

    /// Resolve a model-supplied tool name against the live registry, tolerating
    /// the shapes models actually emit for an existing tool:
    ///
    /// * alias spellings and transport namespaces (`shell_exec`, `functions.bash`)
    ///   via [`Self::resolve_tool_name`];
    /// * `<tool>.<schema-field>`, the name shape a model produces when it merges
    ///   the tool with its required `intent` parameter (`ls.intent`);
    /// * `<tool>.<action>` (`desktop.snapshot`) and `<tool>_<action>`
    ///   (`desktop_find`) — the flattened form our own desktop-tool hints used to
    ///   advertise. The action is injected into the input when the model did not
    ///   pass `action` itself and the tool actually declares that action.
    ///
    /// Anything else resolves to the plain alias mapping and is reported as
    /// unknown by the caller, exactly as before.
    fn resolve_tool_call(name: &str, tools: &HashMap<String, Arc<dyn Tool>>) -> ResolvedToolCall {
        let canonical = Self::resolve_tool_name(name);
        if tools.contains_key(canonical) {
            return ResolvedToolCall {
                name: canonical.to_string(),
                action: None,
            };
        }

        // `<tool>.<selector>`: drop trailing dotted segments, longest tool first,
        // and remember the outermost dropped segment as a candidate action.
        let mut cut = name.len();
        while let Some(dot) = name[..cut].rfind('.') {
            let dropped = &name[dot + 1..cut];
            cut = dot;
            let candidate = Self::resolve_tool_name(&name[..cut]);
            if tools.contains_key(candidate) {
                return ResolvedToolCall {
                    name: candidate.to_string(),
                    action: Self::declared_action(tools, candidate, dropped),
                };
            }
        }

        // `<tool>_<action>`: accepted only when the head is a registered tool
        // that really declares the tail as an action, so names like
        // `conversation_search` and `apply_patch` are never split.
        if let Some((head, tail)) = name.split_once('_') {
            let candidate = Self::resolve_tool_name(head);
            if tools.contains_key(candidate)
                && Self::declared_action(tools, candidate, tail).is_some()
            {
                return ResolvedToolCall {
                    name: candidate.to_string(),
                    action: Some(tail.to_string()),
                };
            }
        }

        ResolvedToolCall {
            name: canonical.to_string(),
            action: None,
        }
    }

    /// `Some(action)` when `tool_key` declares an `action` parameter that accepts
    /// `action` — either as an enum member or as a free-form string.
    fn declared_action(
        tools: &HashMap<String, Arc<dyn Tool>>,
        tool_key: &str,
        action: &str,
    ) -> Option<String> {
        if action.is_empty() {
            return None;
        }
        let tool = tools.get(tool_key)?;
        let schema = tool.parameters_schema();
        let property = schema.get("properties")?.get("action")?;
        let accepted = match property.get("enum").and_then(Value::as_array) {
            Some(values) => values.iter().any(|value| value.as_str() == Some(action)),
            None => property
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "string"),
        };
        accepted.then(|| action.to_string())
    }

    /// Suggest up to 3 available tool names that look similar to `name`.
    /// Uses cheap, dependency-free heuristics: case-insensitive equality,
    /// prefix/substring containment, then bounded edit distance. Helps the
    /// model recover from hallucinated tool names (#104).
    fn closest_tool_names(name: &str, available: &[&str]) -> Vec<String> {
        let needle = name.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, &str)> = available
            .iter()
            .filter_map(|candidate| {
                let hay = candidate.to_ascii_lowercase();
                let score = if hay == needle {
                    0
                } else if hay.starts_with(&needle) || needle.starts_with(&hay) {
                    1
                } else if hay.contains(&needle) || needle.contains(&hay) {
                    2
                } else {
                    let dist = crate::alphacode_core::util::levenshtein(&needle, &hay);
                    // Only suggest near-misses, scaled to the longer name.
                    let threshold = (hay.len().max(needle.len()) / 3).max(2);
                    if dist <= threshold {
                        3 + dist
                    } else {
                        return None;
                    }
                };
                Some((score, *candidate))
            })
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        scored
            .into_iter()
            .take(3)
            .map(|(_, name)| name.to_string())
            .collect()
    }

    /// Estimate token count for a string (chars / 4, matching compaction heuristic)
    fn estimate_tokens(s: &str) -> usize {
        crate::util::estimate_tokens(s)
    }

    fn tool_lifecycle_fields(
        phase: &str,
        requested_name: &str,
        resolved_name: &str,
        input: &Value,
        ctx: &ToolContext,
    ) -> Vec<(String, String)> {
        let cwd = ctx
            .working_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string());
        let input_json = serde_json::to_string(input).unwrap_or_default();
        let mut fields = vec![
            ("phase".to_string(), phase.to_string()),
            ("tool_name".to_string(), requested_name.to_string()),
            ("resolved_tool_name".to_string(), resolved_name.to_string()),
            ("session_id".to_string(), ctx.session_id.clone()),
            ("message_id".to_string(), ctx.message_id.clone()),
            ("tool_call_id".to_string(), ctx.tool_call_id.clone()),
            (
                "execution_mode".to_string(),
                format!("{:?}", ctx.execution_mode),
            ),
            ("cwd".to_string(), cwd),
            ("input_json_bytes".to_string(), input_json.len().to_string()),
        ];

        if let Some(object) = input.as_object() {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            fields.push(("input_keys".to_string(), keys.join(",")));

            let path_fields = [
                "file_path",
                "path",
                "target",
                "target_path",
                "old_path",
                "new_path",
            ];
            let mut touched_paths = Vec::new();
            for key in path_fields {
                if let Some(path) = object.get(key).and_then(Value::as_str) {
                    touched_paths.push(format!(
                        "{key}:{}",
                        ctx.resolve_path(std::path::Path::new(path)).display()
                    ));
                }
            }
            if let Some(paths) = object.get("paths").and_then(Value::as_array) {
                for path in paths.iter().filter_map(Value::as_str).take(8) {
                    touched_paths.push(format!(
                        "paths:{}",
                        ctx.resolve_path(std::path::Path::new(path)).display()
                    ));
                }
            }
            if !touched_paths.is_empty() {
                fields.push(("touched_paths".to_string(), touched_paths.join(",")));
                fields.push((
                    "touched_path_count".to_string(),
                    touched_paths.len().to_string(),
                ));
            }

            for text_key in ["command", "prompt", "task", "query", "content"] {
                if let Some(text) = object.get(text_key).and_then(Value::as_str) {
                    fields.push((format!("{text_key}_bytes"), text.len().to_string()));
                    fields.push((
                        format!("{text_key}_chars"),
                        text.chars().count().to_string(),
                    ));
                }
            }
        }

        fields
    }

    /// Maximum fraction of context budget a single tool output may consume.
    /// Outputs that would push total context beyond this are truncated.
    const CONTEXT_GUARD_THRESHOLD: f32 = 0.90;

    /// Fire the `post_tool` observer hook with tool outcome metadata.
    /// No-op (without building the payload) when the hook is not configured.
    fn fire_post_tool_hook(
        resolved_name: &str,
        ctx: &ToolContext,
        result: &Result<ToolOutput>,
        latency_ms: u64,
    ) {
        if !crate::hooks::hook_configured("post_tool") {
            return;
        }
        let mut event = crate::hooks::HookEvent::new("post_tool")
            .session_id(ctx.session_id.clone())
            .field("TOOL_NAME", resolved_name)
            .field("STATUS", if result.is_ok() { "ok" } else { "error" })
            .field("DURATION_MS", latency_ms.to_string());
        if let Some(dir) = &ctx.working_dir {
            event = event.cwd(dir.display().to_string());
        }
        match result {
            Ok(output) => {
                event = event.field("OUTPUT_BYTES", output.output.len().to_string());
            }
            Err(error) => {
                const ERROR_LIMIT: usize = 1000;
                let message: String = error.to_string().chars().take(ERROR_LIMIT).collect();
                event = event.field("ERROR", message);
            }
        }
        crate::hooks::dispatch_observer(event);
    }

    /// Maximum fraction of context budget a single tool output may occupy.
    /// Even if we have room, a single output shouldn't dominate the context.
    const SINGLE_OUTPUT_MAX_FRACTION: f32 = 0.30;

    /// Return the scheduler-visible side-effect class for a concrete call.
    /// Unknown and unclassified tools remain serialized by default.
    pub async fn execution_class(&self, name: &str, input: Value) -> ToolExecutionClass {
        let input = crate::alphacode_message_types::ToolCall::normalize_input_to_object(input);
        let tools = self.tools.read().await;
        let resolved = Self::resolve_tool_call(name, &tools);
        tools
            .get(&resolved.name)
            .map(|tool| tool.execution_class(&input))
            .unwrap_or(ToolExecutionClass::Mutating)
    }

    /// Execute a tool by name
    pub async fn execute(
        &self,
        name: &str,
        mut input: Value,
        ctx: ToolContext,
    ) -> Result<ToolOutput> {
        // Last line of defense: normalize provider quirks here so *every*
        // caller (agent loops, batch, SDK bridge, server actions) benefits,
        // even if it forgot to normalize. Stringified JSON, single-wrapped
        // arrays, bare URLs, and null/empty all become objects.
        input = crate::alphacode_message_types::ToolCall::normalize_input_to_object(input);
        let tools = self.tools.read().await;
        let resolved = Self::resolve_tool_call(name, &tools);
        let resolved_name: &str = &resolved.name;
        if resolved_name != name {
            crate::logging::info(&format!(
                "Tool name recovery: requested '{name}' -> '{resolved_name}'{} [session {}]",
                resolved
                    .action
                    .as_deref()
                    .map(|action| format!(" (action '{action}')"))
                    .unwrap_or_default(),
                ctx.session_id
            ));
        }
        if let Some(policy) = session_tool_policy(&ctx.session_id) {
            if let Some(allowed) = policy.allowed_tools.as_ref()
                && !allowed.contains(resolved_name)
            {
                repeat_guard::record_failure(&ctx.session_id, name, false, &input);
                return Err(anyhow::anyhow!("Tool '{}' is not allowed", resolved_name));
            }
            if policy.disabled_tools.contains(resolved_name) {
                repeat_guard::record_failure(&ctx.session_id, name, false, &input);
                return Err(anyhow::anyhow!("Tool '{}' is disabled", resolved_name));
            }
        }
        let tool = match tools.get(resolved_name) {
            Some(tool) => tool.clone(),
            None => {
                let mut available: Vec<&str> = tools.keys().map(|k| k.as_str()).collect();
                available.sort_unstable();
                let suggestions = Self::closest_tool_names(name, &available);
                let prior = repeat_guard::prior_failures(&ctx.session_id, name, false, &input);
                let mut msg = format!("Unknown tool: {name}.");
                if !suggestions.is_empty() {
                    msg.push_str(&format!(" Did you mean: {}?", suggestions.join(", ")));
                }
                // Always list available tools: a bare "did you mean" leaves the
                // model with nothing to fall back to when the guess is wrong (#104).
                msg.push_str(&format!(" Available tools: {}.", available.join(", ")));
                if prior >= repeat_guard::UNKNOWN_NAME_LIMIT {
                    msg.push_str(&format!(
                        " This name has now failed {} times; it cannot start working. Stop \
                         calling it and use a different tool.",
                        prior.saturating_add(1)
                    ));
                }
                repeat_guard::record_failure(&ctx.session_id, name, false, &input);
                crate::logging::warn(&format!(
                    "Unknown tool '{name}' requested (prior failures: {prior}) [session {}]",
                    ctx.session_id
                ));
                return Err(anyhow::anyhow!(msg));
            }
        };

        // Drop the lock before executing
        drop(tools);

        // A malformed call that has already been rejected repeatedly will not become
        // well-formed by being repeated. This used to have no bound at all --
        // validation errors called `clear_failure`, so an identical empty
        // argument object could be re-sent indefinitely, which is exactly the
        // observed "eight identical `write` failures in a row" symptom.
        //
        // Corrected calls are unaffected: the streak is keyed on the input, so
        // a fixed call hashes differently and starts from zero.
        let prior_malformed = repeat_guard::prior_malformed(&ctx.session_id, resolved_name, &input);
        if prior_malformed >= repeat_guard::MALFORMED_CALL_LIMIT {
            let last_error = repeat_guard::last_error(&ctx.session_id, resolved_name, true, &input)
                .map(|e| format!("\nLast error: {e}"))
                .unwrap_or_default();
            let received = describe_received_arguments(&input);
            let msg = format!(
                "Refusing to run `{resolved_name}` again: this identical malformed call has \
                 already been rejected {prior_malformed} times in this session. Re-sending it \
                 byte-for-byte cannot succeed. Either send a call with different arguments, \
                 switch to a different tool, or explain to the user what is blocking you.{last_error}\n{received}"
            );
            crate::logging::warn(&format!(
                "Malformed-call guard blocked '{resolved_name}' (prior rejections: \
                 {prior_malformed}) [session {}]",
                ctx.session_id
            ));
            return Err(anyhow::anyhow!(msg));
        }

        // A call that already failed with identical input will not start
        // working; refuse it here so the model gets one corrective message
        // instead of another identical failure to loop on.
        let prior_failures =
            repeat_guard::prior_failures(&ctx.session_id, resolved_name, true, &input);
        if prior_failures >= repeat_guard::IDENTICAL_FAILURE_LIMIT {
            // Quote the last failure so the model knows WHAT to change: a
            // generic "change the arguments" gives it nothing to adapt to and
            // it repeats the call byte-identically until this guard fires.
            let last_error = repeat_guard::last_error(&ctx.session_id, resolved_name, true, &input)
                .map(|e| format!("\nLast error: {e}"))
                .unwrap_or_default();
            let received = describe_received_arguments(&input);
            let msg = format!(
                "Refusing to run `{resolved_name}` again: this identical call already failed \
                 {prior_failures} times in this session. Repeating it cannot succeed — \
                 change the arguments or use a different tool.{last_error}\n{received}"
            );
            crate::logging::warn(&format!(
                "Repeat-failure guard blocked '{resolved_name}' (prior failures: {prior_failures}) \
                 [session {}]",
                ctx.session_id
            ));
            return Err(anyhow::anyhow!(msg));
        }

        // The model named the action instead of the tool (`desktop_find`,
        // `desktop.snapshot`). Inject it when the call does not carry one.
        if let Some(action) = resolved.action.as_deref()
            && let Some(object) = input.as_object_mut()
        {
            object
                .entry("action".to_string())
                .or_insert(Value::String(action.to_string()));
        }

        // User-configured pre_tool gate: external policy hook that can block
        // this call (exit 2). Skipped entirely when not configured.
        if crate::hooks::hook_configured("pre_tool") {
            let input_json = input.to_string();
            let working_dir = ctx
                .working_dir
                .as_ref()
                .map(|dir| dir.display().to_string());
            let decision = crate::hooks::run_pre_tool_gate(
                &ctx.session_id,
                working_dir.as_deref(),
                resolved_name,
                &input_json,
            )
            .await;
            if let crate::hooks::GateDecision::Block { reason } = decision {
                let mut fields =
                    Self::tool_lifecycle_fields("blocked", name, resolved_name, &input, &ctx);
                fields.push(("block_reason".to_string(), reason.clone()));
                crate::logging::event_warn("TOOL_LIFECYCLE", fields);
                return Err(anyhow::anyhow!(
                    "Tool call blocked by pre_tool hook: {reason}"
                ));
            }
        }

        crate::logging::event_info(
            "TOOL_LIFECYCLE",
            Self::tool_lifecycle_fields("start", name, resolved_name, &input, &ctx),
        );

        let started_at = std::time::Instant::now();
        let result = tool.execute(input.clone(), ctx.clone()).await;
        let latency_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        match &result {
            Ok(_) => repeat_guard::record_success(&ctx.session_id, resolved_name, &input),
            Err(error) if is_input_validation_error(error) => {
                // Count it, in its own tier, rather than clearing the streak.
                //
                // A *corrected* retry needs no exemption: the streak is keyed on
                // the input, so a fixed call hashes differently and starts from
                // zero. What the exemption actually permitted was an unbounded
                // loop on the identical broken payload -- the same empty
                // argument object re-sent turn after turn, each one re-reporting
                // "missing field `file_path`" and burning a round trip. The
                // limit is deliberately more forgiving than the runtime tier.
                repeat_guard::record_malformed_with_error(
                    &ctx.session_id,
                    resolved_name,
                    &input,
                    Some(&crate::util::format_error_chain(error)),
                );
            }
            Err(error) => repeat_guard::record_failure_with_error(
                &ctx.session_id,
                resolved_name,
                true,
                &input,
                Some(&crate::util::format_error_chain(error)),
            ),
        }

        crate::telemetry::record_tool_execution(resolved_name, &input, result.is_ok(), latency_ms);
        Self::fire_post_tool_hook(resolved_name, &ctx, &result, latency_ms);

        let mut output = match result {
            Ok(output) => output,
            Err(error) => {
                let mut fields =
                    Self::tool_lifecycle_fields("error", name, resolved_name, &input, &ctx);
                fields.push(("elapsed_ms".to_string(), latency_ms.to_string()));
                fields.push(("error".to_string(), crate::util::format_error_chain(&error)));
                crate::logging::event_warn("TOOL_LIFECYCLE", fields);
                // A malformed call is the model's mistake, not a runtime
                // failure, and almost every tool reports it as a bare serde
                // "missing field X" that names one field at a time. Append the
                // tool's real required set (read from its own schema, so it
                // cannot drift) so one error is enough to fix the whole call
                // instead of one error per missing field.
                if is_input_validation_error(&error) {
                    return Err(anyhow::anyhow!(
                        "{}\n{}",
                        error,
                        schema_call_hint(&tool.parameters_schema()).unwrap_or_else(|| {
                            format!(
                                "Check the `{resolved_name}` schema for its required arguments."
                            )
                        })
                    ));
                }
                return Err(error);
            }
        };

        // Keep a hard ceiling even when a provider advertises a very large
        // context window, then apply the tighter session-specific budget.
        output = cap_tool_output(output);
        output = self.guard_context_overflow(name, output).await;

        let mut fields = Self::tool_lifecycle_fields("done", name, resolved_name, &input, &ctx);
        fields.push(("elapsed_ms".to_string(), latency_ms.to_string()));
        fields.push(("output_bytes".to_string(), output.output.len().to_string()));
        fields.push((
            "output_chars".to_string(),
            output.output.chars().count().to_string(),
        ));
        fields.push(("image_count".to_string(), output.images.len().to_string()));
        crate::logging::event_info("TOOL_LIFECYCLE", fields);

        Ok(output)
    }

    /// Check if a tool output would overflow the context window and truncate if needed.
    /// Returns the (possibly truncated) output.
    async fn guard_context_overflow(&self, tool_name: &str, output: ToolOutput) -> ToolOutput {
        let compaction = self.compaction.read().await;
        let budget = compaction.token_budget();
        if budget == 0 {
            return output;
        }

        let current_tokens = compaction.effective_token_count();
        let output_tokens = Self::estimate_tokens(&output.output);

        // Check 1: Would adding this output push us over the safety threshold?
        let projected = current_tokens + output_tokens;
        let threshold_tokens = (budget as f32 * Self::CONTEXT_GUARD_THRESHOLD) as usize;

        // Check 2: Is this single output unreasonably large relative to budget?
        let single_max_tokens = (budget as f32 * Self::SINGLE_OUTPUT_MAX_FRACTION) as usize;

        let needs_truncation = projected > threshold_tokens || output_tokens > single_max_tokens;

        if !needs_truncation {
            return output;
        }

        // Calculate how many tokens we can afford for this output
        let remaining = if current_tokens < threshold_tokens {
            threshold_tokens - current_tokens
        } else {
            // Already over threshold — allow a small amount for the error message
            budget / 50 // ~2% of budget for the truncation notice
        };
        let max_tokens = remaining.min(single_max_tokens);

        // Convert token limit back to approximate character limit
        let max_chars = max_tokens * 4;

        if output.output.len() <= max_chars {
            return output;
        }

        crate::logging::info(&format!(
            "Context guard: truncating {} output from ~{}k to ~{}k tokens \
             (context: {}k/{}k, {:.0}% used)",
            tool_name,
            output_tokens / 1000,
            max_tokens / 1000,
            current_tokens / 1000,
            budget / 1000,
            (current_tokens as f32 / budget as f32) * 100.0,
        ));

        // Truncate the output, keeping the beginning (usually most relevant)
        let truncated = if max_chars > 200 {
            // Keep beginning of output + truncation notice
            let kept = &output.output[..output.output.floor_char_boundary(max_chars - 150)];
            format!(
                "{}\n\n⚠️ OUTPUT TRUNCATED: This tool output was {:.0}k tokens which would \
                 exceed the context window ({:.0}k/{}k tokens used, {}k budget). \
                 Only the first ~{:.0}k tokens are shown. Use more targeted queries \
                 (e.g., smaller line ranges, specific grep patterns) to get the content \
                 you need without exceeding context limits.",
                kept,
                output_tokens as f32 / 1000.0,
                current_tokens as f32 / 1000.0,
                budget / 1000,
                budget / 1000,
                max_tokens as f32 / 1000.0,
            )
        } else {
            // Context is almost completely full — just return error
            format!(
                "⚠️ CONTEXT LIMIT REACHED: Cannot return this tool output (~{:.0}k tokens) \
                 because the context window is nearly full ({:.0}k/{}k tokens). \
                 Consider using /compact to free up space, or use more targeted queries.",
                output_tokens as f32 / 1000.0,
                current_tokens as f32 / 1000.0,
                budget / 1000,
            )
        };

        ToolOutput {
            output: truncated,
            // Surface the truncation on the transcript row so a user can tell
            // "the model saw a short file" from "the output was cut" without
            // inspecting session history. The existing title is preserved as a
            // prefix when the tool set one.
            title: Some(match output.title {
                Some(existing) => format!("{existing} · truncated ~{}k tok", max_tokens / 1000),
                None => format!("truncated ~{}k tok", max_tokens / 1000),
            }),
            metadata: output.metadata,
            images: output.images,
        }
    }

    /// Register a tool dynamically (for MCP tools, etc.)
    pub async fn register(&self, name: String, tool: Arc<dyn Tool>) {
        let mut tools = self.tools.write().await;
        tools.insert(name, tool);
    }

    /// Register MCP tools (MCP management and server tools)
    /// Connections happen in background to avoid blocking startup.
    /// If `event_tx` is provided, sends an McpStatus event when connections complete.
    /// If `shared_pool` is provided, shared servers reuse processes from the pool.
    pub async fn register_mcp_tools(
        &self,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::protocol::ServerEvent>>,
        shared_pool: Option<std::sync::Arc<crate::mcp::SharedMcpPool>>,
        session_id: Option<String>,
    ) {
        self.register_mcp_tools_for_dir(event_tx, shared_pool, session_id, None)
            .await
    }

    /// Like [`Self::register_mcp_tools`], but resolves project-local MCP config
    /// (`.mcp.json`, `.alphacode/mcp.json`, `.claude/mcp.json`) against
    /// `working_dir` instead of the server process cwd. Remote/client sessions
    /// must pass their session working directory here (issue #420).
    pub async fn register_mcp_tools_for_dir(
        &self,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::protocol::ServerEvent>>,
        shared_pool: Option<std::sync::Arc<crate::mcp::SharedMcpPool>>,
        session_id: Option<String>,
        working_dir: Option<std::path::PathBuf>,
    ) {
        use crate::alphacode_app_core::mcp::McpManager;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let mcp_manager = if let Some(pool) = shared_pool {
            let sid = session_id.unwrap_or_else(|| "unknown".to_string());
            Arc::new(RwLock::new(McpManager::with_shared_pool_for_dir(
                pool,
                sid,
                working_dir,
            )))
        } else {
            Arc::new(RwLock::new(McpManager::new()))
        };

        // Register MCP management tool immediately (with registry for dynamic tool registration)
        let mcp_tool =
            mcp::McpManagementTool::new(Arc::clone(&mcp_manager)).with_registry(self.clone());
        self.register("mcp".to_string(), Arc::new(mcp_tool) as Arc<dyn Tool>)
            .await;

        // Check if we have enabled servers to connect to. Disabled servers stay
        // configured (visible to the mcp management tool, connectable by name)
        // but are not spawned, advertised, or shown as connecting (issue #436).
        let (enabled_count, disabled_count) = {
            let manager = mcp_manager.read().await;
            let enabled = manager
                .config()
                .servers
                .values()
                .filter(|cfg| cfg.is_enabled())
                .count();
            (enabled, manager.config().servers.len() - enabled)
        };

        if disabled_count > 0 {
            crate::logging::info(&format!(
                "MCP: {} disabled server(s) in config (kept, not spawned)",
                disabled_count
            ));
        }

        if enabled_count > 0 {
            crate::logging::info(&format!("MCP: Found {} server(s) in config", enabled_count));

            // Send immediate "connecting" status so the TUI shows loading state
            // Server names with count 0 means "connecting..."
            if let Some(ref tx) = event_tx {
                let server_names: Vec<String> = {
                    let manager = mcp_manager.read().await;
                    manager
                        .config()
                        .servers
                        .iter()
                        .filter(|(_, cfg)| cfg.is_enabled())
                        .map(|(name, _)| format!("{}:0", name))
                        .collect()
                };
                let _ = tx.send(crate::protocol::ServerEvent::McpStatus {
                    servers: server_names,
                });
            }

            // Advertise-early: register proxy tools for each configured server
            // from the on-disk schema cache *before* connections settle, so the
            // first locked tool snapshot already contains MCP tools and we avoid
            // the intentional prompt-cache miss entirely (#206 Phase 2). The
            // proxies connect-on-first-call. Servers with no cached schemas yet
            // (cold start, or reconfigured) fall back to the post-connect
            // registration + one-shot late-register rebuild below.
            let schema_cache = crate::mcp::McpSchemaCache::load();
            let mut advertised_servers: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            {
                let config_servers: Vec<(String, crate::mcp::McpServerConfig)> = {
                    let manager = mcp_manager.read().await;
                    manager
                        .config()
                        .servers
                        .iter()
                        .filter(|(_, cfg)| cfg.is_enabled())
                        .map(|(name, cfg)| (name.clone(), cfg.clone()))
                        .collect()
                };
                let mut advertised_tool_count = 0usize;
                for (server, cfg) in &config_servers {
                    if let Some(cached) = schema_cache.tools_for(server, cfg) {
                        let tools = crate::mcp::create_mcp_tools_from_cached(
                            server,
                            cached,
                            Arc::clone(&mcp_manager),
                        );
                        advertised_tool_count += tools.len();
                        for (name, tool) in tools {
                            self.register(name, tool).await;
                        }
                        advertised_servers.insert(server.clone());
                    }
                }
                if advertised_tool_count > 0 {
                    crate::logging::info(&format!(
                        "MCP: advertised {} cached tool(s) from {} server(s) at spawn \
                         (connect-on-first-call); zero prompt-cache miss expected (#206)",
                        advertised_tool_count,
                        advertised_servers.len()
                    ));
                    // Reflect the advertised tools in the status indicator
                    // immediately so the UI shows them before connections settle.
                    if let Some(ref tx) = event_tx {
                        let mut counts: std::collections::BTreeMap<String, usize> =
                            std::collections::BTreeMap::new();
                        for (server, cfg) in &config_servers {
                            if let Some(cached) = schema_cache.tools_for(server, cfg) {
                                counts.insert(server.clone(), cached.len());
                            }
                        }
                        let servers: Vec<String> = counts
                            .into_iter()
                            .map(|(name, count)| format!("{}:{}", name, count))
                            .collect();
                        let _ = tx.send(crate::protocol::ServerEvent::McpStatus { servers });
                    }
                }
            }

            // Spawn connection and tool registration in background
            let registry = self.clone();
            tokio::spawn(async move {
                let (successes, failures) = {
                    let manager = mcp_manager.write().await;
                    match manager.connect_all().await {
                        Ok(result) => result,
                        Err(e) => {
                            crate::logging::error(&format!(
                                "MCP: connect_all() failed unexpectedly: {}",
                                e
                            ));
                            (0, Vec::new())
                        }
                    }
                };

                if successes > 0 {
                    crate::logging::info(&format!("MCP: Connected to {} server(s)", successes));
                }
                if !failures.is_empty() {
                    for (name, error) in &failures {
                        crate::logging::event_rate_limited(
                            crate::logging::LogLevel::Error,
                            &format!("mcp_register_failed:{name}"),
                            std::time::Duration::from_secs(60),
                            "MCP_REGISTER_FAILED",
                            vec![("server", name.to_string()), ("error", error.to_string())],
                        );
                    }
                }

                // Register MCP server tools and collect server info
                let tools = crate::mcp::create_mcp_tools(Arc::clone(&mcp_manager)).await;
                let mut server_counts: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for (name, tool) in &tools {
                    if let Some(rest) = name.strip_prefix("mcp__")
                        && let Some((server, _)) = rest.split_once("__")
                    {
                        *server_counts.entry(server.to_string()).or_default() += 1;
                    }
                    // Idempotent: advertise-early may have already registered an
                    // identical proxy. Re-registering refreshes it with the live
                    // schema, which is correct (handles schema drift).
                    registry.register(name.clone(), tool.clone()).await;
                }

                // Reconcile the on-disk schema cache with the live schemas so the
                // next spawn can advertise the up-to-date tools with zero cache
                // miss. Group live tool defs by server and update each entry
                // under the current config fingerprint; prune servers that are
                // no longer configured. (#206 Phase 2)
                {
                    // Live tool defs grouped by server, plus a snapshot of the
                    // configured servers, captured under one read lock.
                    type LiveToolsByServer =
                        std::collections::BTreeMap<String, Vec<crate::mcp::McpToolDef>>;
                    type ConfigSnapshot = Vec<(String, crate::mcp::McpServerConfig)>;
                    let (live_by_server, config_snapshot): (LiveToolsByServer, ConfigSnapshot) = {
                        let manager = mcp_manager.read().await;
                        let mut grouped: std::collections::BTreeMap<
                            String,
                            Vec<crate::mcp::McpToolDef>,
                        > = std::collections::BTreeMap::new();
                        for (server, def) in manager.all_tools().await {
                            grouped.entry(server).or_default().push(def);
                        }
                        let configs = manager
                            .config()
                            .servers
                            .iter()
                            .map(|(name, cfg)| (name.clone(), cfg.clone()))
                            .collect();
                        (grouped, configs)
                    };

                    let mut cache = crate::mcp::McpSchemaCache::load();
                    let mut dirty = false;
                    for (server, cfg) in &config_snapshot {
                        if let Some(defs) = live_by_server.get(server) {
                            // Only cache servers that actually exposed tools.
                            if cache.update(server, cfg, defs.clone()) {
                                dirty = true;
                            }
                        }
                    }
                    let configured_names: Vec<String> =
                        config_snapshot.iter().map(|(n, _)| n.clone()).collect();
                    if cache.retain_servers(&configured_names) {
                        dirty = true;
                    }
                    if dirty {
                        cache.save();
                        crate::logging::info(
                            "MCP: updated on-disk tool-schema cache from live connection (#206)",
                        );
                    }
                }

                // Notify client of MCP status
                if let Some(tx) = event_tx {
                    let servers: Vec<String> = server_counts
                        .into_iter()
                        .map(|(name, count)| format!("{}:{}", name, count))
                        .collect();
                    let _ = tx.send(crate::protocol::ServerEvent::McpStatus { servers });
                }
            });
        }
    }

    /// Register self-dev tools (only for canary/self-dev sessions)
    pub async fn register_selfdev_tools(&self) {
        // Self-dev management tool
        let selfdev_tool = selfdev::SelfDevTool::new();
        self.register(
            "selfdev".to_string(),
            Arc::new(selfdev_tool) as Arc<dyn Tool>,
        )
        .await;

        // Debug socket tool for direct debug socket access
        let debug_socket_tool = debug_socket::DebugSocketTool::new();
        self.register(
            "debug_socket".to_string(),
            Arc::new(debug_socket_tool) as Arc<dyn Tool>,
        )
        .await;
    }

    /// Register ambient-mode tools (only for ambient sessions)
    pub async fn register_ambient_tools(&self) {
        self.register(
            "end_ambient_cycle".to_string(),
            Arc::new(ambient::EndAmbientCycleTool::new()) as Arc<dyn Tool>,
        )
        .await;

        self.register(
            "schedule_ambient".to_string(),
            Arc::new(ambient::ScheduleAmbientTool::new()) as Arc<dyn Tool>,
        )
        .await;

        self.register(
            "request_permission".to_string(),
            Arc::new(ambient::RequestPermissionTool::new()) as Arc<dyn Tool>,
        )
        .await;

        self.register(
            "send_message".to_string(),
            Arc::new(ambient::SendChannelMessageTool::new()) as Arc<dyn Tool>,
        )
        .await;
    }

    /// Unregister a tool
    pub async fn unregister(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let mut tools = self.tools.write().await;
        tools.remove(name)
    }

    /// Unregister all tools matching a prefix
    pub async fn unregister_prefix(&self, prefix: &str) -> Vec<String> {
        let mut tools = self.tools.write().await;
        let to_remove: Vec<String> = tools
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        for name in &to_remove {
            tools.remove(name);
        }
        to_remove
    }

    /// Get shared access to the skill registry
    pub fn skills(&self) -> Arc<RwLock<SkillRegistry>> {
        self.skills.clone()
    }

    /// Get shared access to the compaction manager
    pub fn compaction(&self) -> Arc<RwLock<CompactionManager>> {
        self.compaction.clone()
    }
}

#[cfg(test)]
mod tests;
