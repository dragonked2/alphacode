//! alphagrep: code search and retrieval for coding agents.
//!
//! alphagrep is the search engine behind alphacode's `alphagrep` tool. It offers
//! four modes:
//!
//! * **grep** ([`search`]) — literal or regex content search with glob and file
//!   type filters,
//! * **find** ([`find`]) — ranked file-name lookup,
//! * **outline** ([`outline`]) — structure summary of a single file,
//! * **trace** ([`smart_engine`], DSL in [`smart_dsl`]) — "where does this
//!   subject live and what should I read" investigation packets.
//!
//! The crate is dependency-light on purpose: an ignore-aware walker
//! ([`ignore`]), a glob matcher ([`globset`]), a regex engine ([`regex`]), and
//! serde. There is no external process to install and no parser generator; the
//! structure heuristics live in [`structure`] and are honest about being
//! heuristics.
//!
//! [`ignore`]: https://docs.rs/ignore
//! [`globset`]: https://docs.rs/globset
//! [`regex`]: https://docs.rs/regex

pub mod cli;
pub mod find;
pub mod fsutil;
pub mod harness;
pub mod outline;
pub mod render;
pub mod search;
pub mod smart_dsl;
pub mod smart_engine;
pub mod structure;

#[cfg(test)]
mod integration_tests {
    use crate::cli::{FindArgs, FullRegionMode, GrepArgs, OutlineArgs, SmartArgs};
    use crate::render::{
        render_find_output, render_grep_output, render_outline_output, render_smart_output,
    };
    use std::fs;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src/tui")).expect("mkdir");
        fs::write(
            dir.path().join("src/tui/render.rs"),
            concat!(
                "pub struct Frame;\n",
                "\n",
                "pub fn render_frame(frame: &Frame) -> String {\n",
                "    format!(\"frame {}\", frame.id)\n",
                "}\n",
            ),
        )
        .expect("write");
        fs::write(
            dir.path().join("src/tui/mod.rs"),
            "mod render;\npub use render::render_frame;\n",
        )
        .expect("write");
        fs::write(dir.path().join("notes.md"), "# Notes\n\ntext\n").expect("write");
        dir
    }

    #[test]
    fn every_mode_answers_over_the_same_repository() {
        let dir = repo();
        let root = dir.path();

        let grep_args = GrepArgs {
            query: "render_frame".to_string(),
            regex: false,
            file_type: Some("rs".to_string()),
            json: false,
            paths_only: false,
            hidden: false,
            no_ignore: true,
            path: None,
            glob: None,
        };
        let grep = crate::search::run_grep(root, &grep_args).expect("grep");
        assert_eq!(grep.total_files, 2);
        assert!(render_grep_output(&grep, &grep_args, None).contains("src/tui/render.rs"));

        let find_args = FindArgs {
            query_parts: vec!["render".to_string()],
            file_type: None,
            json: false,
            paths_only: false,
            debug_score: false,
            max_files: 5,
            hidden: false,
            no_ignore: true,
            path: None,
            glob: None,
        };
        let find = crate::find::run_find(root, &find_args);
        assert_eq!(find.files[0].path, "src/tui/render.rs");
        assert!(render_find_output(&find, &find_args).contains("1. src/tui/render.rs"));

        let outline_args = OutlineArgs {
            file: "src/tui/render.rs".to_string(),
            json: false,
            max_items: None,
            path: None,
            context_json: None,
        };
        let outline = crate::outline::run_outline(root, &outline_args).expect("outline");
        let outline_text = render_outline_output(&outline);
        assert!(outline_text.contains("- fn render_frame @ 3-5"));

        let smart_args = SmartArgs {
            terms: vec!["subject:render_frame".to_string()],
            json: false,
            max_files: 5,
            max_regions: 6,
            full_region: FullRegionMode::Auto,
            debug_plan: true,
            debug_score: true,
            paths_only: false,
            path: Some("src".to_string()),
            file_type: None,
            glob: None,
            hidden: false,
            no_ignore: true,
            context_json: None,
        };
        let query = crate::smart_dsl::parse_smart_query(&smart_args.terms).expect("query");
        let trace = crate::smart_engine::run_smart(root, &query, &smart_args).expect("trace");
        assert_eq!(
            trace.summary.best_file.as_deref(),
            Some("src/tui/render.rs")
        );

        let trace_text = render_smart_output(&trace, &smart_args);
        assert!(trace_text.contains("best answer likely in src/tui/render.rs"));
        assert!(trace_text.contains("plan: subject=[\"render_frame\"]"));
        assert!(trace_text.contains("structure:"));
        assert!(trace_text.contains("full region:"));
    }

    #[test]
    fn trace_reports_paths_relative_to_the_root_it_is_given() {
        let dir = repo();
        let root = dir.path().join("src/tui");
        let smart_args = SmartArgs {
            terms: vec!["subject:render".to_string()],
            json: false,
            max_files: 5,
            max_regions: 6,
            full_region: FullRegionMode::Never,
            debug_plan: false,
            debug_score: false,
            paths_only: false,
            path: Some(root.display().to_string()),
            file_type: None,
            glob: None,
            hidden: false,
            no_ignore: true,
            context_json: None,
        };
        let query = crate::smart_dsl::parse_smart_query(&smart_args.terms).expect("query");
        let trace = crate::smart_engine::run_smart(&root, &query, &smart_args).expect("trace");
        assert!(!trace.files.is_empty());
        // Paths are relative to the root the caller scoped the search to.
        assert!(trace.files.iter().all(|file| !file.path.contains('/')));
        assert!(
            trace
                .files
                .iter()
                .flat_map(|file| file.regions.iter())
                .all(|region| !region.full)
        );
    }
}
