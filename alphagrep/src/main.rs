//! The `alphagrep` binary: a thin wrapper over the library modes.
//!
//! Exit codes follow ripgrep: `0` when something matched, `1` when nothing did,
//! and `2` for errors, so alphagrep can be used in shell pipelines.

use alphagrep::cli::{Cli, Command};
use alphagrep::find::run_find;
use alphagrep::outline::run_outline;
use alphagrep::render::{
    render_find_output, render_grep_output, render_outline_output, render_smart_output,
};
use alphagrep::search::run_grep;
use alphagrep::smart_dsl::parse_smart_query;
use alphagrep::smart_engine::run_smart;
use clap::Parser;
use std::io::Write;
use std::path::PathBuf;

fn resolve_root(path: &Option<String>) -> PathBuf {
    match path {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

fn emit(text: &str) {
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

fn emit_error(message: &str) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "alphagrep: {message}");
}

fn emit_json<T: serde::Serialize>(value: &T) -> i32 {
    match serde_json::to_string_pretty(value) {
        Ok(text) => {
            emit(&text);
            0
        }
        Err(err) => {
            emit_error(&format!("could not serialize JSON output: {err}"));
            2
        }
    }
}

fn emit_query_help() {
    emit_error("trace needs at least one term, for example:");
    emit_error("  alphagrep trace subject:auth_status relation:rendered support:ui");
    emit_error("  alphagrep trace subject:render_status path:src kind:fn");
}

fn main() {
    let cli = Cli::parse();

    let code = match cli.command {
        Command::Grep(args) => {
            let root = resolve_root(&args.path);
            match run_grep(&root, &args) {
                Ok(result) => {
                    if args.json {
                        emit_json(&result)
                    } else {
                        emit(&render_grep_output(&result, &args, None));
                        i32::from(result.total_matches == 0)
                    }
                }
                Err(err) => {
                    emit_error(&err);
                    2
                }
            }
        }
        Command::Find(args) => {
            let root = resolve_root(&args.path);
            let result = run_find(&root, &args);
            let empty = result.files.is_empty();
            if args.json {
                let code = emit_json(&result);
                if code != 0 { code } else { i32::from(empty) }
            } else {
                emit(&render_find_output(&result, &args));
                i32::from(empty)
            }
        }
        Command::Outline(args) => {
            let root = resolve_root(&args.path);
            match run_outline(&root, &args) {
                Ok(result) => {
                    let empty = result.items.is_empty();
                    if args.json {
                        let code = emit_json(&result);
                        if code != 0 { code } else { i32::from(empty) }
                    } else {
                        emit(&render_outline_output(&result));
                        i32::from(empty)
                    }
                }
                Err(err) => {
                    emit_error(&err);
                    2
                }
            }
        }
        Command::Trace(args) => {
            if args.terms.is_empty() {
                emit_query_help();
                2
            } else {
                match parse_smart_query(&args.terms) {
                    Err(err) => {
                        emit_error(&err);
                        emit_query_help();
                        2
                    }
                    Ok(query) => {
                        let root = resolve_root(&args.path);
                        match run_smart(&root, &query, &args) {
                            Ok(result) => {
                                let empty = result.files.is_empty();
                                if args.json {
                                    let code = emit_json(&result);
                                    if code != 0 { code } else { i32::from(empty) }
                                } else {
                                    emit(&render_smart_output(&result, &args));
                                    i32::from(empty)
                                }
                            }
                            Err(err) => {
                                emit_error(&err);
                                2
                            }
                        }
                    }
                }
            }
        }
    };

    std::process::exit(code);
}
