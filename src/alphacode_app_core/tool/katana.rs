use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const DEFAULT_DEPTH: usize = 3;
const DEFAULT_THREADS: usize = 50;

pub struct KatanaTool;

#[derive(Deserialize, Debug)]
struct KatanaInput {
    url: String,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    threads: Option<usize>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    json_output: bool,
    #[serde(default)]
    no_color: bool,
    #[serde(default)]
    f: Option<String>,
    /// Crawl-scope regex (`-crawl-scope`). Replaces the old `d` field, which
    /// was emitted as `-d` — that is `-depth`, an int, so a domain string
    /// there was a hard parse error and with both fields set the second `-d`
    /// silently overwrote the requested depth.
    #[serde(default)]
    domain_scope: Option<String>,
    #[serde(default)]
    robots: bool,
    /// Extra request headers as `Name: value` (katana `-H`). Was a bare bool,
    /// which could never produce a valid `-H` value.
    #[serde(default)]
    headers: Vec<String>,
    #[serde(default)]
    include_body: bool,
    #[serde(default)]
    include_params: bool,
}

/// Recover the seed URL before deserializing the rest.
///
/// Splitting the required `url` out of the struct means one mistyped optional
/// field (`"depth": "3"`, `"headers": "Accept: */*"`) can no longer take the
/// whole crawl down with it.
fn normalize_katana_input(input: &Value) -> Result<KatanaInput> {
    let url = super::coerce_url_arg(input, "katana")?;
    let with_url = |mut map: serde_json::Map<String, Value>| {
        map.insert("url".to_string(), Value::String(url.clone()));
        map
    };
    let base = input.as_object().cloned().unwrap_or_default();

    // Preferred path: every optional field is the documented type.
    if let Ok(params) = serde_json::from_value::<KatanaInput>(Value::Object(with_url(base.clone())))
    {
        return Ok(params);
    }
    // Otherwise retry without each field in turn. A crawl is expensive to set up
    // and expensive to abandon, so dropping one unusable optional field is
    // always better than refusing the call.
    for key in base.keys() {
        if key == "url" {
            continue;
        }
        let mut trial = base.clone();
        trial.remove(key);
        if let Ok(params) = serde_json::from_value::<KatanaInput>(Value::Object(with_url(trial))) {
            crate::logging::debug(&format!(
                "katana: ignoring unusable `{key}` from the request"
            ));
            return Ok(params);
        }
    }
    // Nothing salvageable; report the original complaint.
    serde_json::from_value::<KatanaInput>(Value::Object(with_url(base))).map_err(Into::into)
}

#[async_trait]
impl Tool for KatanaTool {
    fn name(&self) -> &str {
        "katana"
    }

    fn description(&self) -> &str {
        "Fast, passive web crawler. Use AFTER direct-test triage (webfetch homepage + headers + fingerprint + visible params) has produced a crawl hypothesis, or for organization-scope mapping. Do NOT use as the first step on single-service work — fetch the homepage directly first."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "intent": super::intent_schema_property(),
                "url": {
                    "type": "string",
                    "description": "Target URL to crawl."
                },
                "depth": {
                    "type": "integer",
                    "description": "Maximum crawl depth (katana -d). Default: 3."
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of concurrent fetchers (katana -c). Katana's own default is 10."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Per-request timeout in seconds. Katana's own default is 10."
                },
                "json_output": {
                    "type": "boolean",
                    "description": "Output in JSONL format. Default: false."
                },
                "f": {
                    "type": "string",
                    "description": "Field selector (e.g., 'u', 'd', 'r', 'ru', 'rd', 'ri', 'm', 'rdi', 'f')"
                },
                "domain_scope": {
                    "type": "string",
                    "description": "Regex limiting which hosts/paths katana may crawl (katana -crawl-scope). Use to keep a crawl inside the authorized scope."
                },
                "robots": {
                    "type": "boolean",
                    "description": "Crawl known files including robots.txt and sitemap.xml (katana -kf all). Default: false."
                },
                "headers": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Extra request headers as 'Name: value' (katana -H). Use to test authenticated or role-scoped surfaces."
                },
                "include_body": {
                    "type": "boolean",
                    "description": "Include response bodies. Bodies are included by default; false adds katana -ob. Default: true."
                },
                "include_params": {
                    "type": "boolean",
                    "description": "Crawl the same path with differing query-param values. Enabled by default; false adds katana -iqp. Default: true."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        // The seed URL is recovered rather than deserialized, so a bare string,
        // an alias key, or a truncated payload still starts the crawl instead
        // of failing with "missing field `url`" before any request is made.
        let params = normalize_katana_input(&input)?;
        let args = build_args(&params)?;

        let output = super::recon_common::run_bounded(
            "katana",
            &args,
            super::recon_common::DEFAULT_TOOL_TIMEOUT,
        )
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. {}", super::recon_common::install_hint("katana"))
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "{}",
                super::recon_common::describe_failure("katana", &output)
            ));
        }

        let (urls, total, truncated) = super::recon_common::parse_lines(&output.stdout);

        let mut result = format!("katana found {total} URLs from {}:\n\n", params.url);

        for url in &urls {
            result.push_str(url);
            result.push('\n');
        }
        result.push_str(&super::recon_common::truncation_notice(urls.len(), total));

        let mut metadata = HashMap::new();
        metadata.insert("url".to_string(), json!(params.url));
        metadata.insert("count".to_string(), json!(urls.len()));
        metadata.insert("total_found".to_string(), json!(total));
        metadata.insert("truncated".to_string(), json!(truncated));
        metadata.insert(
            "depth".to_string(),
            // Report what katana actually used. Depth/threads are only sent
            // when explicitly requested, so claiming the wrapper's advertised
            // default here was reporting a number katana never received.
            json!(params.depth.unwrap_or(DEFAULT_DEPTH)),
        );
        metadata.insert(
            "thread_count".to_string(),
            json!(params.threads.unwrap_or(DEFAULT_THREADS)),
        );

        Ok(ToolOutput::new(result)
            .with_title(format!("katana: {} URLs crawled", urls.len()))
            .with_metadata(json!(metadata)))
    }
}

impl KatanaTool {
    pub fn new() -> Self {
        Self
    }
}

/// Build argv for katana.
///
/// Every flag below is verified against katana's `readFlags()`. The previous
/// version of this function was broken in several ways at once:
///
/// * `-threads` is not a katana flag (it is `-c` / `-concurrency`), so
///   setting `threads` made every call exit 2.
/// * `-robots` is not a flag; the known-files enum is `-kf` (`all`,
///   `robotstxt`, `sitemapxml`).
/// * `-no-remote` and `-no-store` do not exist. `-ns` is *no-scope* and
///   `-sr` is *store-response* — the opposite of what the names imply.
/// * `-include-body` / `-include-params` do not exist; the real flags
///   `-ob` (omit-body) and `-iqp` (ignore-query-params) are their inverses.
/// * `d` was emitted as `-d`, which is `-depth` (an *int*). Passing a domain
///   string there was a parse error, and with both `depth` and `d` set the
///   second `-d` silently overwrote the first. `domain_scope` now maps to the
///   real `-crawl-scope` regex flag.
fn build_args(params: &KatanaInput) -> Result<Vec<String>> {
    let mut args = Vec::new();
    args.push("-u".to_string());
    args.push(super::recon_common::validate_target(&params.url)?);

    if let Some(depth) = params.depth {
        args.push("-d".to_string());
        args.push(depth.to_string());
    }

    if let Some(threads) = params.threads {
        args.push("-c".to_string());
        args.push(threads.to_string());
    }

    if let Some(timeout) = params.timeout {
        args.push("-timeout".to_string());
        args.push(timeout.to_string());
    }

    if params.json_output {
        args.push("-j".to_string());
    }
    if params.no_color {
        args.push("-nc".to_string());
    }
    if let Some(ref f) = params.f {
        args.push("-f".to_string());
        args.push(f.clone());
    }
    if let Some(ref scope) = params.domain_scope {
        args.push("-crawl-scope".to_string());
        args.push(scope.clone());
    }
    if params.robots {
        // `-kf` is an enum: robotstxt and sitemapxml are both wanted here.
        args.push("-kf".to_string());
        args.push("all".to_string());
    }
    super::recon_common::append_default_user_agent_header(&mut args, &params.headers);
    for header in &params.headers {
        let h = header.trim();
        if h.is_empty() {
            continue;
        }
        if !h.contains(':') {
            return Err(anyhow::anyhow!(
                "invalid header `{h}`: expected the form `Name: value`"
            ));
        }
        args.push("-H".to_string());
        args.push(h.to_string());
    }
    if !params.include_body {
        // Bodies are included by default; the flag that removes them is `-ob`.
        args.push("-ob".to_string());
    }
    if !params.include_params {
        // Query-param variants are crawled by default; `-iqp` collapses them.
        args.push("-iqp".to_string());
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The seed URL is the only field that must be right; everything else is
    /// optional. These are the shapes that used to fail before any crawl
    /// started, costing a full agent turn.
    #[test]
    fn normalizes_the_seed_url_from_the_shapes_models_emit() {
        let parse = |v: serde_json::Value| {
            normalize_katana_input(&v).expect("seed url should be recovered")
        };

        assert_eq!(parse(json!({"url": "https://a.com"})).url, "https://a.com");
        // Alias keys.
        for key in ["uri", "target", "link", "href"] {
            assert_eq!(
                parse(json!({ key: "https://a.com" })).url,
                "https://a.com",
                "alias {key} was not recovered"
            );
        }
        // Bare string, and a single-element array wrapper.
        assert_eq!(parse(json!("https://a.com")).url, "https://a.com");
        assert_eq!(
            parse(json!([{"url": "https://a.com"}])).url,
            "https://a.com"
        );
        // Optional fields still parse as documented.
        let params = parse(json!({"url": "https://a.com", "depth": 5, "robots": true}));
        assert_eq!(params.depth, Some(5));
        assert!(params.robots);
    }

    /// A single mistyped optional field must not take the whole crawl down: the
    /// field is dropped and the rest is honoured, because abandoning a crawl
    /// costs far more than ignoring one flag.
    #[test]
    fn a_mistyped_optional_field_is_dropped_not_fatal() {
        let params = normalize_katana_input(&json!({
            "url": "https://a.com",
            "depth": 5,
            // `depth` is an int; a string is a very common provider coercion.
            "threads": "20",
        }))
        .expect("crawl should start despite the bad field");
        assert_eq!(params.depth, Some(5));
        // The unusable field is ignored rather than guessed at.
        assert_eq!(params.threads, None);
    }

    #[test]
    fn a_missing_seed_url_is_reported_actionably() {
        let err = normalize_katana_input(&json!({"depth": 3}))
            .expect_err("no seed url means no crawl")
            .to_string();
        assert!(err.contains("missing field `url`"), "{err}");
        assert!(err.contains("https://example.com"), "{err}");
    }

    #[test]
    fn test_build_args_basic() {
        let input = KatanaInput {
            url: "https://example.com".to_string(),
            depth: None,
            threads: None,
            timeout: None,
            json_output: false,
            no_color: false,
            f: None,
            domain_scope: None,
            robots: false,
            headers: Vec::new(),
            include_body: false,
            include_params: false,
        };
        let args = build_args(&input).expect("build_args");
        assert!(args.contains(&"-u".to_string()));
        assert!(args.contains(&"https://example.com".to_string()));
    }

    /// Regression: the old builder emitted six flags katana does not
    /// register (`-threads`, `-no-remote`, `-no-store`, `-robots`,
    /// `-include-body`, `-include-params`), and mapped `d` onto `-d`, which is
    /// `-depth` (an int). Every one of those made katana exit 2.
    #[test]
    fn only_real_katana_flags_are_emitted() {
        let mut input = KatanaInput {
            url: "https://example.com".to_string(),
            depth: Some(5),
            threads: Some(20),
            timeout: None,
            json_output: false,
            no_color: false,
            f: None,
            domain_scope: Some("example\\.com".to_string()),
            robots: true,
            headers: vec!["Authorization: Bearer x".to_string()],
            include_body: true,
            include_params: true,
        };
        let args = build_args(&input).expect("build_args");
        for bad in [
            "-threads",
            "-no-remote",
            "-no-store",
            "-robots",
            "-include-body",
            "-include-params",
        ] {
            assert!(
                !args.contains(&bad.to_string()),
                "{bad} is not a katana flag: {args:?}"
            );
        }
        // The real spellings.
        assert!(args.contains(&"-c".to_string()), "threads: {args:?}");
        assert!(args.contains(&"-kf".to_string()), "robots: {args:?}");
        assert!(args.contains(&"-crawl-scope".to_string()));
        assert!(args.contains(&"-H".to_string()));
        // `include_body`/`include_params` default to on, so their inverses
        // must be absent.
        assert!(!args.contains(&"-ob".to_string()));
        assert!(!args.contains(&"-iqp".to_string()));

        // Turning them off adds the inverse flags.
        input.include_body = false;
        input.include_params = false;
        let args = build_args(&input).expect("build_args");
        assert!(args.contains(&"-ob".to_string()));
        assert!(args.contains(&"-iqp".to_string()));
    }

    /// `-d` is `-depth`, an int. A domain string there was a parse error, and
    /// with both set the second `-d` overwrote the requested depth.
    #[test]
    fn depth_is_the_only_thing_mapped_to_dash_d() {
        let input = KatanaInput {
            url: "https://example.com".to_string(),
            depth: Some(7),
            threads: None,
            timeout: None,
            json_output: false,
            no_color: false,
            f: None,
            domain_scope: Some("example\\.com".to_string()),
            robots: false,
            headers: Vec::new(),
            include_body: true,
            include_params: true,
        };
        let args = build_args(&input).expect("build_args");
        let d_positions: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "-d")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(d_positions.len(), 1, "more than one -d: {args:?}");
        assert_eq!(args[d_positions[0] + 1], "7");
    }

    #[test]
    fn malformed_header_is_rejected() {
        let input = KatanaInput {
            url: "https://example.com".to_string(),
            depth: None,
            threads: None,
            timeout: None,
            json_output: false,
            no_color: false,
            f: None,
            domain_scope: None,
            robots: false,
            headers: vec!["not-a-header".to_string()],
            include_body: true,
            include_params: true,
        };
        assert!(build_args(&input).is_err());
    }
}
