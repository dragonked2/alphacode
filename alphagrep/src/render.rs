//! Rendering for every mode.
//!
//! Two audiences read these renderings: humans and the alphacode session
//! harness. The harness parses trace output to learn which files, symbols, and
//! regions a search exposed, so the shapes below are part of the contract:
//!
//! * outline and trace structure items are `- <kind> <label> @ <start>-<end>`,
//! * trace file headers are `N. <path>` with nothing else on the line,
//! * trace regions are introduced by `structure:`/`regions:` on their own line
//!   and then `- <label> @ <start>-<end>`, `kind: ...`, and a `full region:` or
//!   `snippet:` marker,
//! * body lines are always prefixed with their line number, which keeps them
//!   from being mistaken for headers.
//!
//! Labels are sanitised in [`crate::structure::safe_label`], so the two spaces
//! around `@` in a header always separate the label from the range.

use crate::cli::{FindArgs, GrepArgs, SmartArgs};
use crate::find::FindResult;
use crate::outline::OutlineResult;
use crate::search::{GrepResult, MAX_FILES_WITH_MATCHES};
use crate::smart_engine::SmartResult;

/// Match lines printed when the caller does not set a limit.
pub const DEFAULT_MAX_MATCH_LINES: usize = 60;
/// Indentation for nested trace sections.
const INDENT: &str = "   ";

/// Render a grep result.
pub fn render_grep_output(
    result: &GrepResult,
    args: &GrepArgs,
    max_regions: Option<usize>,
) -> String {
    let kind = if result.literal { "literal" } else { "regex" };
    let mut lines: Vec<String> = Vec::new();

    if result.files.is_empty() {
        lines.push(format!(
            "No matches for {kind} {:?} under {}",
            result.query, result.root
        ));
        lines.push(format!(
            "note: {} file(s) scanned; try a shorter token, check the spelling, widen --glob/--type, or use mode=find to locate files by name",
            result.files_scanned
        ));
        return join_lines(lines);
    }

    if args.paths_only {
        lines.push(format!(
            "{} file(s) match {kind} {:?} under {}",
            result.total_files, result.query, result.root
        ));
        lines.extend(result.files.iter().map(|file| file.path.clone()));
        return join_lines(lines);
    }

    let limit = max_regions
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_MATCH_LINES);
    lines.push(format!(
        "{} match(es) in {} file(s) for {kind} {:?}",
        result.total_matches, result.total_files, result.query
    ));

    let mut printed = 0usize;
    'files: for file in &result.files {
        lines.push(String::new());
        lines.push(file.path.clone());
        for found in &file.matches {
            if printed >= limit {
                break 'files;
            }
            lines.push(format!("  {}: {}", found.line, found.text));
            printed += 1;
        }
        if file.omitted > 0 {
            lines.push(format!(
                "  ... {} more match(es) in this file",
                file.omitted
            ));
        }
    }

    if result.total_matches > printed {
        lines.push(String::new());
        lines.push(format!(
            "note: showing {printed} of {} matches; narrow the query or raise max_regions",
            result.total_matches
        ));
    }
    if result.truncated {
        lines.push(format!(
            "note: only the first {MAX_FILES_WITH_MATCHES} files with matches are listed"
        ));
    }
    join_lines(lines)
}

/// Render a find result.
pub fn render_find_output(result: &FindResult, args: &FindArgs) -> String {
    let mut lines: Vec<String> = Vec::new();
    if let Some(note) = &result.note {
        lines.push(format!("note: {note}"));
    }

    if result.files.is_empty() {
        if result.query.is_empty() {
            lines.push(format!("No files found under {}", result.root));
        } else {
            lines.push(format!(
                "No files matched {:?} under {}",
                result.query.join(" "),
                result.root
            ));
            lines.push(
                "note: try a single term (a file stem, a name fragment, or *a* glob), or drop the terms and narrow with --path/--glob/--type"
                    .to_string(),
            );
        }
        return join_lines(lines);
    }

    if args.paths_only {
        lines.extend(result.files.iter().map(|file| file.path.clone()));
        return join_lines(lines);
    }

    if result.query.is_empty() {
        lines.push(format!(
            "{} file(s) under {}",
            result.files.len(),
            result.root
        ));
    } else {
        lines.push(format!(
            "{} file(s) ranked for {:?}",
            result.files.len(),
            result.query.join(" ")
        ));
    }

    for (index, file) in result.files.iter().enumerate() {
        lines.push(format!("{}. {}", index + 1, file.path));
        if args.debug_score {
            lines.push(format!(
                "{INDENT}score: {:.1} ({})",
                file.score,
                file.reasons.join(", ")
            ));
        }
    }
    join_lines(lines)
}

/// Render an outline result.
pub fn render_outline_output(result: &OutlineResult) -> String {
    let mut lines: Vec<String> = Vec::new();

    if result.items.is_empty() {
        lines.push(format!(
            "No structure detected in {} ({} file, {} line(s))",
            result.path, result.language, result.lines
        ));
        lines.push(
            "note: outline recognises common declaration shapes only; use mode=grep to search the raw text"
                .to_string(),
        );
    } else {
        lines.push(format!(
            "{}: {} item(s) ({} file, {} line(s))",
            result.path, result.total_items, result.language, result.lines
        ));
        for item in &result.items {
            lines.push(format!(
                "{}- {} {} @ {}-{}",
                "  ".repeat(item.depth),
                item.kind,
                item.label,
                item.start_line,
                item.end_line
            ));
        }
    }

    if let Some(note) = &result.note {
        lines.push(format!("note: {note}"));
    }
    join_lines(lines)
}

/// Render a trace result.
pub fn render_smart_output(result: &SmartResult, args: &SmartArgs) -> String {
    let mut lines: Vec<String> = Vec::new();

    if args.debug_plan {
        for line in &result.plan {
            lines.push(format!("plan: {line}"));
        }
        if !result.plan.is_empty() {
            lines.push(String::new());
        }
    }

    if result.files.is_empty() {
        lines.push(format!("No trace results for {:?}", result.query_text));
        if let Some(note) = &result.summary.note {
            lines.push(format!("note: {note}"));
        }
        return join_lines(lines);
    }

    if let Some(best) = &result.summary.best_file {
        lines.push(format!("best answer likely in {best}"));
    }

    if args.paths_only {
        lines.extend(result.files.iter().map(|file| file.path.clone()));
        return join_lines(lines);
    }

    lines.push(format!(
        "{} file(s) scored, {} region(s) kept ({} scanned)",
        result.summary.total_files, result.summary.total_regions, result.summary.scanned
    ));

    for (index, file) in result.files.iter().enumerate() {
        lines.push(String::new());
        lines.push(format!("{}. {}", index + 1, file.path));
        if args.debug_score {
            lines.push(format!(
                "{INDENT}score: {:.1} ({})",
                file.score,
                file.reasons.join(", ")
            ));
        }

        if !file.structure.is_empty() {
            lines.push(format!("{INDENT}structure:"));
            for item in &file.structure {
                lines.push(format!(
                    "{INDENT}- {} {} @ {}-{}",
                    item.kind, item.label, item.start_line, item.end_line
                ));
            }
        }

        if !file.regions.is_empty() {
            lines.push(format!("{INDENT}regions:"));
            for region in &file.regions {
                lines.push(format!(
                    "{INDENT}- {} @ {}-{}",
                    region.label, region.start_line, region.end_line
                ));
                if let Some(kind) = &region.kind {
                    lines.push(format!("{INDENT}kind: {kind}"));
                }
                if args.debug_score {
                    lines.push(format!(
                        "{INDENT}score: {:.1} ({} hit(s))",
                        region.score,
                        region.matched_lines.len()
                    ));
                }
                lines.push(format!(
                    "{INDENT}{}",
                    if region.full {
                        "full region:"
                    } else {
                        "snippet:"
                    }
                ));
                for entry in &region.body {
                    if entry.line == 0 {
                        lines.push(format!("{INDENT}{}", entry.text));
                    } else {
                        lines.push(format!("{INDENT}{}: {}", entry.line, entry.text));
                    }
                }
            }
        }
    }

    if let Some(note) = &result.summary.note {
        lines.push(String::new());
        lines.push(format!("note: {note}"));
    }
    join_lines(lines)
}

fn join_lines(lines: Vec<String>) -> String {
    let mut text = lines.join("\n");
    while text.ends_with('\n') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::FullRegionMode;
    use crate::find::FoundFile;
    use crate::search::{GrepFile, GrepMatch};
    use crate::smart_engine::{RegionLine, SmartFile, SmartRegion, SmartSummary};
    use crate::structure::StructureItem;

    fn grep_args() -> GrepArgs {
        GrepArgs {
            query: "needle".to_string(),
            regex: false,
            file_type: None,
            json: false,
            paths_only: false,
            hidden: false,
            no_ignore: false,
            path: None,
            glob: None,
        }
    }

    fn find_args() -> FindArgs {
        FindArgs {
            query_parts: vec!["parser".to_string()],
            file_type: None,
            json: false,
            paths_only: false,
            debug_score: false,
            max_files: 10,
            hidden: false,
            no_ignore: false,
            path: None,
            glob: None,
        }
    }

    fn smart_args() -> SmartArgs {
        SmartArgs {
            terms: Vec::new(),
            json: false,
            max_files: 5,
            max_regions: 6,
            full_region: FullRegionMode::Auto,
            debug_plan: false,
            debug_score: false,
            paths_only: false,
            path: None,
            file_type: None,
            glob: None,
            hidden: false,
            no_ignore: false,
            context_json: None,
        }
    }

    #[test]
    fn paths_only_grep_lists_bare_paths() {
        let result = GrepResult {
            root: "repo".to_string(),
            query: "needle".to_string(),
            literal: true,
            files: vec![GrepFile {
                path: "src/a.rs".to_string(),
                matches: vec![GrepMatch {
                    line: 3,
                    column: 1,
                    text: "needle".to_string(),
                }],
                omitted: 0,
            }],
            total_files: 1,
            total_matches: 1,
            omitted_matches: 0,
            truncated: false,
            files_scanned: 9,
        };
        let mut args = grep_args();
        args.paths_only = true;
        let output = render_grep_output(&result, &args, None);
        assert!(output.contains("src/a.rs"));
        assert!(!output.contains("  3: "));
    }

    #[test]
    fn grep_marks_truncated_output() {
        let result = GrepResult {
            root: "repo".to_string(),
            query: "needle".to_string(),
            literal: true,
            files: vec![GrepFile {
                path: "src/a.rs".to_string(),
                matches: vec![GrepMatch {
                    line: 1,
                    column: 1,
                    text: "needle".to_string(),
                }],
                omitted: 4,
            }],
            total_files: 1,
            total_matches: 5,
            omitted_matches: 4,
            truncated: false,
            files_scanned: 1,
        };
        let output = render_grep_output(&result, &grep_args(), Some(1));
        assert!(output.contains("showing 1 of 5 matches"));
        assert!(output.contains("4 more match(es) in this file"));
    }

    #[test]
    fn find_headers_are_numbered_when_not_paths_only() {
        let result = FindResult {
            root: "repo".to_string(),
            query: vec!["parser".to_string()],
            files: vec![FoundFile {
                path: "src/parser.rs".to_string(),
                score: 130.0,
                reasons: vec!["exact file stem (+110)".to_string()],
            }],
            files_scanned: 4,
            note: None,
        };
        let output = render_find_output(&result, &find_args());
        assert!(output.contains("1. src/parser.rs"));
    }

    #[test]
    fn outline_items_keep_the_parseable_shape() {
        let result = OutlineResult {
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            items: vec![StructureItem {
                kind: "fn",
                label: "parse".to_string(),
                start_line: 10,
                end_line: 20,
                depth: 1,
                signature: "fn parse() {".to_string(),
            }],
            total_items: 1,
            lines: 40,
            note: None,
        };
        let output = render_outline_output(&result);
        assert!(output.contains("\n  - fn parse @ 10-20"));
    }

    #[test]
    fn trace_output_carries_every_harness_marker() {
        let result = SmartResult {
            query_text: "render_status".to_string(),
            plan: vec!["subject=[\"render_status\"]".to_string()],
            files: vec![SmartFile {
                path: "src/auth.rs".to_string(),
                score: 42.0,
                reasons: vec!["symbols match: render_status (+30)".to_string()],
                structure: vec![StructureItem {
                    kind: "fn",
                    label: "render_status".to_string(),
                    start_line: 5,
                    end_line: 7,
                    depth: 0,
                    signature: "pub fn render_status() {".to_string(),
                }],
                regions: vec![SmartRegion {
                    label: "render_status".to_string(),
                    kind: Some("fn".to_string()),
                    start_line: 5,
                    end_line: 7,
                    matched_lines: vec![5],
                    body: vec![RegionLine {
                        line: 5,
                        text: "pub fn render_status() {".to_string(),
                    }],
                    full: true,
                    score: 38.0,
                }],
            }],
            summary: SmartSummary {
                total_files: 1,
                total_regions: 1,
                best_file: Some("src/auth.rs".to_string()),
                scanned: 12,
                note: None,
            },
        };
        let output = render_smart_output(&result, &smart_args());
        assert!(output.contains("best answer likely in src/auth.rs\n"));
        assert!(output.contains("\n1. src/auth.rs\n"));
        assert!(output.contains("   structure:\n   - fn render_status @ 5-7\n"));
        assert!(
            output
                .contains("   regions:\n   - render_status @ 5-7\n   kind: fn\n   full region:\n")
        );
        assert!(output.ends_with("   5: pub fn render_status() {"));

        // Nothing below a section header may read as one to the harness parser:
        // body lines always carry a `NN: ` prefix, so the indented headers are
        // the only lines that are allowed to trim down to a bare header.
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed == "structure:" || trimmed == "regions:" {
                assert!(line.starts_with("   "), "header is indented: {line:?}");
                continue;
            }
            assert!(
                !trimmed.starts_with("- ") || trimmed.contains(" @ "),
                "unparseable item line: {line:?}"
            );
        }
    }
}
