use super::*;
use crate::alphacode_app_core::tool::{ToolContext, ToolExecutionMode};
use serde_json::json;

fn make_ctx(working_dir: std::path::PathBuf) -> ToolContext {
    ToolContext {
        session_id: "test-session".to_string(),
        message_id: "test-message".to_string(),
        tool_call_id: "test-call".to_string(),
        working_dir: Some(working_dir),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    }
}

/// The path is recovered through the shared coercion ladder, so a bare string
/// or a truncated payload must produce the same result as the documented shape.
#[test]
fn read_input_recovers_the_path_through_the_coercion_ladder() {
    let parse = |v: serde_json::Value| ReadInput::from_value(&v).expect("path should be recovered");

    // Documented key.
    assert_eq!(parse(json!({"file_path": "a.rs"})).file_path, "a.rs");
    // Aliases, in the spellings models actually use.
    for key in [
        "path",
        "file",
        "filename",
        "file_name",
        "filePath",
        "filepath",
    ] {
        assert_eq!(
            parse(json!({ key: "a.rs" })).file_path,
            "a.rs",
            "alias {key}"
        );
    }
    // Bare string where an object was expected.
    assert_eq!(parse(json!("a.rs")).file_path, "a.rs");
    // Single-element array wrapper.
    assert_eq!(parse(json!([{"file_path": "a.rs"}])).file_path, "a.rs");
    // Surrounding whitespace, which the transport would otherwise reject.
    assert_eq!(parse(json!({"file_path": "  a.rs  "})).file_path, "a.rs");
}

/// A numeric string for a range field is what providers that coerce tool
/// arguments to strings send; rejecting it broke reads the model had otherwise
/// spelled correctly.
#[test]
fn read_input_accepts_numeric_strings_for_range_fields() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": "10",
        "limit": "20"
    }))
    .expect("numeric strings should parse");
    assert_eq!(params.start_line, Some(10));
    assert_eq!(params.limit, Some(20));

    let range = normalize_read_range(&params).expect("range should normalize");
    assert_eq!(range.offset, 9);
    assert_eq!(range.limit, 20);
}

/// A missing path must name the field, the fix, and what actually arrived —
/// a bare serde "missing field `file_path`" is what the model kept retrying.
#[test]
fn read_input_reports_a_missing_path_actionably() {
    let err = ReadInput::from_value(&json!({}))
        .expect_err("empty payload has no path")
        .to_string();
    assert!(err.contains("missing field `file_path`"), "{err}");
    assert!(err.contains("no arguments at all"), "{err}");
}

#[test]
fn normalize_read_range_supports_start_and_end_lines() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": 10,
        "end_line": 20
    }))
    .expect("params should parse");

    let range = normalize_read_range(&params).expect("normalize range");
    assert_eq!(
        range,
        NormalizedReadRange {
            offset: 9,
            limit: 11,
            style: ReadRangeStyle::StartEnd,
        }
    );
}

#[test]
fn normalize_read_range_supports_start_line_and_limit() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": 10,
        "limit": 20
    }))
    .expect("params should parse");

    let range = normalize_read_range(&params).expect("start_line + limit should work");
    assert_eq!(
        range,
        NormalizedReadRange {
            offset: 9,
            limit: 20,
            style: ReadRangeStyle::StartEnd,
        }
    );
}

#[test]
fn normalize_read_range_prefers_end_line_over_limit() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": 10,
        "end_line": 20,
        "limit": 999
    }))
    .expect("params should parse");

    let range = normalize_read_range(&params).expect("end_line should take precedence");
    assert_eq!(
        range,
        NormalizedReadRange {
            offset: 9,
            limit: 11,
            style: ReadRangeStyle::StartEnd,
        }
    );
}

#[test]
fn normalize_read_range_rejects_start_line_and_offset() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": 10,
        "offset": 20
    }))
    .expect("params should parse");

    let err = normalize_read_range(&params).expect_err("mixed range styles should fail");
    assert!(
        err.to_string().contains("Use either start_line/end_line")
            || err.to_string().contains("not both"),
        "unexpected error: {err}"
    );
}

#[test]
fn normalize_read_range_accepts_matching_start_line_and_offset() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": 10,
        "offset": 9,
        "limit": 20
    }))
    .expect("params should parse");

    let range = normalize_read_range(&params).expect("matching range styles should work");
    assert_eq!(
        range,
        NormalizedReadRange {
            offset: 9,
            limit: 20,
            style: ReadRangeStyle::StartEnd,
        }
    );
}

#[test]
fn normalize_read_range_accepts_end_line_with_zero_offset() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "end_line": 20,
        "offset": 0
    }))
    .expect("params should parse");

    let range = normalize_read_range(&params).expect("redundant zero offset should work");
    assert_eq!(
        range,
        NormalizedReadRange {
            offset: 0,
            limit: 20,
            style: ReadRangeStyle::StartEnd,
        }
    );
}

#[test]
fn normalize_read_range_rejects_invalid_end_before_start() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": 20,
        "end_line": 10
    }))
    .expect("params should parse");

    let err = normalize_read_range(&params).expect_err("invalid range should fail");
    assert!(
        err.to_string()
            .contains("greater than or equal to start_line"),
        "unexpected error: {err}"
    );
}

#[test]
fn normalize_read_range_saturates_extreme_line_numbers() {
    let params = ReadInput::from_value(&json!({
        "file_path": "src/lib.rs",
        "start_line": u64::MAX,
        "end_line": u64::MAX
    }))
    .expect("extreme JSON integers should be parsed without narrowing overflow");

    let range = normalize_read_range(&params).expect("equal extreme endpoints form one line");
    assert_eq!(range.offset, usize::MAX - 1);
    assert_eq!(range.limit, 1);
    assert_eq!(range.next_offset(), usize::MAX);
    assert_eq!(range.next_start_line(), usize::MAX);
}

#[test]
fn read_tool_schema_avoids_openai_incompatible_combinators() {
    let schema = ReadTool::new().parameters_schema();

    assert_eq!(schema.get("type"), Some(&json!("object")));
    assert!(schema.get("allOf").is_none());
    assert!(schema.get("not").is_none());
}

#[test]
fn read_tool_schema_advertises_only_canonical_public_fields() {
    let schema = ReadTool::new().parameters_schema();
    let properties = schema["properties"]
        .as_object()
        .expect("read schema properties should be an object");

    assert!(properties.contains_key("file_path"));
    assert!(properties.contains_key("start_line"));
    assert!(properties.contains_key("limit"));
    assert!(!properties.contains_key("end_line"));
    assert!(!properties.contains_key("offset"));
}

#[test]
fn read_tool_description_advertises_supported_file_types() {
    let tool = ReadTool::new();
    let description = tool.description().to_lowercase();
    assert!(description.contains("text"), "description={description}");
    assert!(description.contains("image"), "description={description}");
    assert!(description.contains("pdf"), "description={description}");

    let schema = tool.parameters_schema();
    let file_path_description = schema["properties"]["file_path"]["description"]
        .as_str()
        .expect("file_path should have a description");
    assert_eq!(file_path_description, "Path to a file.");
}

#[tokio::test]
async fn read_tool_supports_start_line_and_end_line() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("sample.txt");
    std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").expect("write sample file");

    let tool = ReadTool::new();
    let output = tool
        .execute(
            json!({
                "file_path": "sample.txt",
                "start_line": 2,
                "end_line": 4
            }),
            make_ctx(temp.path().to_path_buf()),
        )
        .await
        .expect("read execution should succeed");

    assert!(
        output.output.contains("2\ttwo"),
        "output={:?}",
        output.output
    );
    assert!(
        output.output.contains("3\tthree"),
        "output={:?}",
        output.output
    );
    assert!(
        output.output.contains("4\tfour"),
        "output={:?}",
        output.output
    );
    assert!(
        !output.output.contains("1\tone"),
        "output={:?}",
        output.output
    );
    assert!(
        !output.output.contains("5\tfive"),
        "output={:?}",
        output.output
    );
}

#[tokio::test]
async fn read_tool_continuation_hint_matches_start_line_style() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("sample.txt");
    std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").expect("write sample file");

    let tool = ReadTool::new();
    let output = tool
        .execute(
            json!({
                "file_path": "sample.txt",
                "start_line": 2,
                "end_line": 3
            }),
            make_ctx(temp.path().to_path_buf()),
        )
        .await
        .expect("read execution should succeed");

    assert!(
        output.output.contains("Use `start_line=4` to continue."),
        "output={:?}",
        output.output
    );
}

#[tokio::test]
async fn read_tool_supports_start_line_with_limit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("sample.txt");
    std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").expect("write sample file");

    let tool = ReadTool::new();
    let output = tool
        .execute(
            json!({
                "file_path": "sample.txt",
                "start_line": 2,
                "limit": 2
            }),
            make_ctx(temp.path().to_path_buf()),
        )
        .await
        .expect("read execution should succeed");

    assert!(
        output.output.contains("2\ttwo"),
        "output={:?}",
        output.output
    );
    assert!(
        output.output.contains("3\tthree"),
        "output={:?}",
        output.output
    );
    assert!(
        !output.output.contains("4\tfour"),
        "output={:?}",
        output.output
    );
    assert!(
        output.output.contains("Use `start_line=4` to continue."),
        "output={:?}",
        output.output
    );
}

#[tokio::test]
async fn read_tool_prefers_end_line_over_limit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("sample.txt");
    std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").expect("write sample file");

    let tool = ReadTool::new();
    let output = tool
        .execute(
            json!({
                "file_path": "sample.txt",
                "start_line": 2,
                "end_line": 3,
                "limit": 50
            }),
            make_ctx(temp.path().to_path_buf()),
        )
        .await
        .expect("read execution should succeed");

    assert!(
        output.output.contains("2\ttwo"),
        "output={:?}",
        output.output
    );
    assert!(
        output.output.contains("3\tthree"),
        "output={:?}",
        output.output
    );
    assert!(
        !output.output.contains("4\tfour"),
        "output={:?}",
        output.output
    );
    assert!(
        output.output.contains("Use `start_line=4` to continue."),
        "output={:?}",
        output.output
    );
}

#[tokio::test]
async fn read_tool_reports_an_out_of_range_request_without_calling_the_file_empty() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(temp.path().join("sample.txt"), "one\ntwo\n").expect("write sample file");

    let output = ReadTool::new()
        .execute(
            json!({"file_path": "sample.txt", "offset": 20, "limit": 5}),
            make_ctx(temp.path().to_path_buf()),
        )
        .await
        .expect("an out-of-range request is not an I/O error");

    assert!(output.output.contains("no lines in the requested range"));
    assert!(output.output.contains("file contains 2 lines"));
    assert_eq!(output.metadata.as_ref().unwrap()["total_lines"], 2);
}

#[test]
fn binary_detection_accepts_utf8_text_in_non_latin_scripts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("unicode.txt");
    std::fs::write(&path, "日本語 العربية русский язык\n".repeat(300)).expect("write text");

    assert!(
        !is_binary_file(&path),
        "valid UTF-8 text should not be classified as binary"
    );
}
