//! M11: the parallel grep scan must match the serial result exactly.
//!
//! `run_grep` walks the tree serially (deterministic file order) and then scans
//! each file on rayon's thread pool. These tests pin the contract that matters:
//! the parallel pass returns the same files, in the same order, with the same
//! matches and the same counters as the serial implementation it replaced.

use alphagrep::cli::GrepArgs;
use alphagrep::fsutil::{self, SearchScope};
use alphagrep::search::{GrepFile, GrepMatch, GrepResult, run_grep};
use regex::Regex;
use std::fs;
use std::path::Path;

/// A reference implementation of the pre-M11 serial scan, kept here so the
/// parallel path can be compared against it rather than against a hand-written
/// expectation.
fn serial_grep(root: &Path, args: &GrepArgs) -> GrepResult {
    let query = args.query.clone();
    let pattern = if args.regex {
        query.clone()
    } else {
        regex::escape(&query)
    };
    let matcher = Regex::new(&pattern).expect("pattern");
    let scope = SearchScope::new(root)
        .with_glob(args.glob.as_deref())
        .expect("glob")
        .with_file_type(args.file_type.as_deref())
        .expect("type")
        .with_flags(args.hidden, args.no_ignore);

    let mut result = GrepResult {
        root: scope.root.display().to_string(),
        query,
        literal: !args.regex,
        files: Vec::new(),
        total_files: 0,
        total_matches: 0,
        omitted_matches: 0,
        truncated: false,
        files_scanned: 0,
    };

    for file in fsutil::collect_files(&scope) {
        result.files_scanned += 1;
        let Some(text) = fsutil::read_text(&file.absolute) else {
            continue;
        };
        let mut matches: Vec<GrepMatch> = Vec::new();
        let mut omitted = 0usize;
        let mut total = 0usize;
        for (index, line) in text.lines().enumerate() {
            let Some(found) = matcher.find(line) else {
                continue;
            };
            total += 1;
            if matches.len() < 40 {
                matches.push(GrepMatch {
                    line: index + 1,
                    column: found.start() + 1,
                    text: alphagrep::search::clip_line(line),
                });
            } else {
                omitted += 1;
            }
        }
        if total == 0 {
            continue;
        }
        if result.files.len() >= 200 {
            result.truncated = true;
            result.omitted_matches += total;
            continue;
        }
        result.total_matches += total;
        result.omitted_matches += omitted;
        result.files.push(GrepFile {
            path: file.relative,
            matches,
            omitted,
        });
    }
    result.total_files = result.files.len();
    result
}

fn args(query: &str) -> GrepArgs {
    GrepArgs {
        query: query.to_string(),
        regex: false,
        file_type: None,
        json: false,
        paths_only: false,
        hidden: false,
        no_ignore: true,
        path: None,
        glob: None,
    }
}

/// Build a tree with several files that all contain the needle, plus decoys
/// that do not, spread across directories so the walk order is non-trivial.
fn corpus() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for folder in ["a", "b", "c", "d", "e"] {
        fs::create_dir(dir.path().join(folder)).expect("mkdir");
    }
    for (folder, index) in [("a", 0), ("b", 1), ("c", 2), ("d", 3), ("e", 4)] {
        let mut body = String::new();
        for line in 0..index {
            body.push_str(&format!("filler line {line}\n"));
        }
        body.push_str("let needle = 1;\n");
        body.push_str("let after = 2;\n");
        body.push_str("needle again\n");
        fs::write(dir.path().join(folder).join("match.rs"), body).expect("write");
    }
    // Decoys with no match at all.
    fs::write(dir.path().join("a/decoy.txt"), "nothing here\n").expect("write");
    fs::write(dir.path().join("e/decoy.md"), "# title\n").expect("write");
    // A file that sorts before the others at the top level.
    fs::write(dir.path().join("aaa.rs"), "needle at root\n").expect("write");
    dir
}

#[test]
fn parallel_scan_matches_the_serial_result_exactly() {
    let dir = corpus();
    let parallel = run_grep(dir.path(), &args("needle")).expect("grep");
    let serial = serial_grep(dir.path(), &args("needle"));

    assert_eq!(
        parallel
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>(),
        serial
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>(),
        "file order must be identical to the serial implementation"
    );
    assert_eq!(parallel.files, serial.files, "per-file matches must match");
    assert_eq!(parallel.total_files, serial.total_files);
    assert_eq!(parallel.total_matches, serial.total_matches);
    assert_eq!(parallel.omitted_matches, serial.omitted_matches);
    assert_eq!(parallel.files_scanned, serial.files_scanned);
    assert_eq!(parallel.truncated, serial.truncated);
    assert_eq!(parallel.root, serial.root);
    assert_eq!(parallel.query, serial.query);
    assert_eq!(parallel.literal, serial.literal);
}

#[test]
fn results_stay_in_walk_order_across_repeated_runs() {
    let dir = corpus();
    let first = run_grep(dir.path(), &args("needle")).expect("grep");
    for _ in 0..8 {
        let again = run_grep(dir.path(), &args("needle")).expect("grep");
        assert_eq!(
            first
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            again
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            "repeated runs must produce a stable order"
        );
    }
}

#[test]
fn file_order_is_the_walk_order() {
    let dir = corpus();
    let result = run_grep(dir.path(), &args("needle")).expect("grep");
    let paths: Vec<&str> = result.files.iter().map(|file| file.path.as_str()).collect();

    // The walker's own order is the contract: the result set must follow it.
    let scope = SearchScope::new(dir.path()).with_flags(false, true);
    let walked: Vec<String> = fsutil::collect_files(&scope)
        .into_iter()
        .map(|file| file.relative)
        .collect();
    for path in &paths {
        assert!(
            walked.contains(&path.to_string()),
            "result path {path:?} must come from the walked set"
        );
    }
    // And the result must be in that same relative order.
    let mut expected: Vec<&str> = paths.clone();
    expected.sort_by_key(|path| {
        walked
            .iter()
            .position(|candidate| candidate == path)
            .expect("walked path")
    });
    assert_eq!(paths, expected, "result files must follow the walk order");

    assert!(
        paths.contains(&"aaa.rs"),
        "root file must be present: {paths:?}"
    );
    assert_eq!(
        result.total_matches, 11,
        "two matches in each of five folder files plus one at the root"
    );
    assert_eq!(
        result.files_scanned, 8,
        "six matching files plus two decoys"
    );
}

#[test]
fn regex_mode_matches_the_serial_result() {
    let dir = corpus();
    let mut regex_args = args("need[a-z]+");
    regex_args.regex = true;
    let parallel = run_grep(dir.path(), &regex_args).expect("grep");
    let serial = serial_grep(dir.path(), &regex_args);
    assert_eq!(parallel.files, serial.files);
    assert_eq!(parallel.total_matches, serial.total_matches);
    assert_eq!(parallel.files_scanned, serial.files_scanned);
}

#[test]
fn a_pattern_with_no_matches_reports_nothing() {
    let dir = corpus();
    let parallel = run_grep(dir.path(), &args("zzz_nonexistent_token_xyz")).expect("grep");
    let serial = serial_grep(dir.path(), &args("zzz_nonexistent_token_xyz"));
    assert!(parallel.files.is_empty());
    assert_eq!(parallel.total_matches, 0);
    assert_eq!(parallel.total_files, 0);
    assert_eq!(parallel.files_scanned, serial.files_scanned);
    assert_eq!(parallel.files_scanned, 8);
}

#[test]
fn per_file_match_cap_is_applied_in_walk_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    for name in ["a.rs", "b.rs", "c.rs"] {
        let mut body = String::new();
        for line in 0..50 {
            body.push_str(&format!("needle on line {line}\n"));
        }
        fs::write(dir.path().join(name), body).expect("write");
    }

    let parallel = run_grep(dir.path(), &args("needle")).expect("grep");
    let serial = serial_grep(dir.path(), &args("needle"));

    assert_eq!(parallel.files, serial.files, "capped matches must match");
    assert_eq!(parallel.total_matches, 150);
    assert_eq!(parallel.omitted_matches, 30, "10 dropped per file");
    for file in &parallel.files {
        assert_eq!(file.matches.len(), 40, "per-file cap must hold");
        assert_eq!(file.omitted, 10);
    }
}
