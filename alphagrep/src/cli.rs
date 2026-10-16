//! Command-line surface for the `alphagrep` binary.
//!
//! The same four modes are reachable through the library entry points used by
//! the alphacode agent tool ([`crate::search::run_grep`],
//! [`crate::find::run_find`], [`crate::outline::run_outline`], and
//! [`crate::smart_engine::run_smart`]), so the argument structs in this module
//! double as the public request type for in-process callers.

use clap::{Args, Parser, Subcommand, ValueEnum};

/// How much of a matched region the trace engine may print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum FullRegionMode {
    /// Print whole symbol bodies when they are small, snippets otherwise.
    #[default]
    Auto,
    /// Always print the whole region body.
    Always,
    /// Only ever print short snippets around the matched lines.
    Never,
}

impl FullRegionMode {
    /// Lowercase spelling used by the CLI, the DSL, and debug output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl std::fmt::Display for FullRegionMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Search file contents for a pattern.
#[derive(Debug, Clone, Args)]
pub struct GrepArgs {
    /// Text to look for; treated as a regex only with `--regex`.
    #[arg(value_name = "PATTERN")]
    pub query: String,
    /// Interpret the pattern as a regular expression instead of literal text.
    #[arg(long)]
    pub regex: bool,
    /// Restrict to a file type such as `rs`, `py`, `ts`, or `md`.
    #[arg(short = 't', long = "type", alias = "file-type", value_name = "TYPE")]
    pub file_type: Option<String>,
    /// Emit machine-readable JSON instead of the human summary.
    #[arg(long)]
    pub json: bool,
    /// Print only the matching file paths.
    #[arg(long)]
    pub paths_only: bool,
    /// Include hidden files and directories.
    #[arg(long)]
    pub hidden: bool,
    /// Search files that ignore files would normally skip.
    #[arg(long)]
    pub no_ignore: bool,
    /// Directory or file to search; defaults to the current directory.
    #[arg(short = 'p', long, value_name = "PATH")]
    pub path: Option<String>,
    /// Only consider paths matching this glob, such as `src/**/*.rs`.
    #[arg(short = 'g', long, value_name = "GLOB")]
    pub glob: Option<String>,
}

/// Rank files by how well their names match the given terms.
#[derive(Debug, Clone, Args)]
pub struct FindArgs {
    /// Ranking terms, usually one or two file-name fragments.
    #[arg(value_name = "TERM")]
    pub query_parts: Vec<String>,
    /// Restrict to a file type such as `rs`, `py`, `ts`, or `md`.
    #[arg(short = 't', long = "type", alias = "file-type", value_name = "TYPE")]
    pub file_type: Option<String>,
    /// Emit machine-readable JSON instead of the human summary.
    #[arg(long)]
    pub json: bool,
    /// Print only the ranked file paths.
    #[arg(long)]
    pub paths_only: bool,
    /// Append the per-file score breakdown to the output.
    #[arg(long)]
    pub debug_score: bool,
    /// Maximum number of files to return.
    #[arg(
        short = 'n',
        long,
        alias = "limit",
        value_name = "N",
        default_value_t = 10
    )]
    pub max_files: usize,
    /// Include hidden files and directories.
    #[arg(long)]
    pub hidden: bool,
    /// Search files that ignore files would normally skip.
    #[arg(long)]
    pub no_ignore: bool,
    /// Directory or file to search; defaults to the current directory.
    #[arg(short = 'p', long, value_name = "PATH")]
    pub path: Option<String>,
    /// Only consider paths matching this glob, such as `src/**/*.rs`.
    #[arg(short = 'g', long, value_name = "GLOB")]
    pub glob: Option<String>,
}

/// Summarize the structure of a single file.
#[derive(Debug, Clone, Args)]
pub struct OutlineArgs {
    /// File to inspect.
    #[arg(value_name = "FILE")]
    pub file: String,
    /// Emit machine-readable JSON instead of the human outline.
    #[arg(long)]
    pub json: bool,
    /// Maximum number of structure items to print.
    #[arg(long, value_name = "N")]
    pub max_items: Option<usize>,
    /// Directory the file path is relative to; defaults to the search root.
    #[arg(short = 'p', long, value_name = "PATH")]
    pub path: Option<String>,
    /// Session harness context written by the calling agent.
    #[arg(long, value_name = "FILE")]
    pub context_json: Option<String>,
}

/// Trace a subject across the code base with the small term DSL.
#[derive(Debug, Clone, Args)]
pub struct SmartArgs {
    /// DSL terms such as `subject:auth_status`, `relation:rendered`, `support:ui`.
    #[arg(value_name = "TERM")]
    pub terms: Vec<String>,
    /// Emit machine-readable JSON instead of the human trace.
    #[arg(long)]
    pub json: bool,
    /// Maximum number of files to return.
    #[arg(short = 'n', long, value_name = "N", default_value_t = 5)]
    pub max_files: usize,
    /// Maximum number of regions to return across all files.
    #[arg(long, value_name = "N", default_value_t = 6)]
    pub max_regions: usize,
    /// Whether whole symbol bodies may be printed for matched regions.
    #[arg(long, value_enum, default_value_t = FullRegionMode::Auto)]
    pub full_region: FullRegionMode,
    /// Print how the query was interpreted before the results.
    #[arg(long)]
    pub debug_plan: bool,
    /// Append per-file and per-region score breakdowns.
    #[arg(long)]
    pub debug_score: bool,
    /// Print only the ranked file paths.
    #[arg(long)]
    pub paths_only: bool,
    /// Directory or file to search; defaults to the current directory.
    #[arg(short = 'p', long, value_name = "PATH")]
    pub path: Option<String>,
    /// Restrict to a file type such as `rs`, `py`, `ts`, or `md`.
    #[arg(short = 't', long = "type", alias = "file-type", value_name = "TYPE")]
    pub file_type: Option<String>,
    /// Only consider paths matching this glob, such as `src/**/*.rs`.
    #[arg(short = 'g', long, value_name = "GLOB")]
    pub glob: Option<String>,
    /// Include hidden files and directories.
    #[arg(long)]
    pub hidden: bool,
    /// Search files that ignore files would normally skip.
    #[arg(long)]
    pub no_ignore: bool,
    /// Session harness context written by the calling agent.
    #[arg(long, value_name = "FILE")]
    pub context_json: Option<String>,
}

/// Top-level command line for the `alphagrep` binary.
#[derive(Debug, Parser)]
#[command(
    name = "alphagrep",
    version,
    about = "Code search and retrieval for coding agents",
    arg_required_else_help = true
)]
pub struct Cli {
    /// Mode to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Modes exposed by the binary.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Search file contents for a pattern.
    Grep(GrepArgs),
    /// Rank files by how well their names match the terms.
    Find(FindArgs),
    /// Summarize the structure of a single file.
    Outline(OutlineArgs),
    /// Trace a subject across the code base.
    #[command(alias = "smart")]
    Trace(SmartArgs),
}
