# alphagrep

CLI-first code search and retrieval for coding agents.

`alphagrep` is the search engine behind the `alphagrep` tool in
[alphacode](https://github.com/dragonked2/alphacode). It answers the four
questions an agent actually asks when it lands in an unfamiliar code base:

| Mode | Question | Command |
| --- | --- | --- |
| **grep** | *Where does this text appear?* | `alphagrep grep` |
| **find** | *Which file is this?* | `alphagrep find` |
| **outline** | *What is in this file?* | `alphagrep outline` |
| **trace** | *Where does this subject live, and what should I read?* | `alphagrep trace` |

It is dependency-light on purpose: an ignore-aware walker, a glob matcher, a
regex engine, and serde. There is no external process to install and no parser
generator; the structure heuristics in `structure` are honest about being
heuristics.

## Install

```sh
cargo install alphagrep
```

Or use it as a library:

```toml
[dependencies]
alphagrep = "0.1"
```

## Usage

### grep — content search

Literal by default, regex with `--regex`.

```sh
alphagrep grep "render_frame"                       # literal search
alphagrep grep "fn\s+\w+\(" --regex                # regex search
alphagrep grep "auth" -t rs -g "src/**/*.rs"        # file type + glob filter
alphagrep grep "auth" --json                        # machine-readable output
alphagrep grep "auth" --paths-only                  # just the file paths
```

### find — ranked file lookup

Ranks files by how well their names match the given terms.

```sh
alphagrep find render                              # rank files by name
alphagrep find auth status -n 20                   # more terms, more results
alphagrep find auth --debug-score                  # show the score breakdown
```

### outline — structure summary

Summarises the structure of a single file (items, functions, types) without
reading the whole body.

```sh
alphagrep outline src/tui/app.rs
alphagrep outline src/tui/app.rs --json
alphagrep outline src/tui/app.rs --max-items 50
```

### trace — investigation packet

Traces a subject across the code base and returns the ranked files, the matched
regions, and the structure around them — the "read this, not all of it"
answer. The DSL is a handful of `field:value` terms:

| Field | Aliases | Meaning |
| --- | --- | --- |
| `subject` | `subj`, `term`, `text` | What to find. |
| `relation` | `rel` | How it connects to the code. |
| `support` | `supports`, `supporting` | Extra ranking terms. |
| `path` | `file`, `under` | Scope filter. |
| `kind` | `symbol` | Declaration kind filter (`fn`, `class`, …). |

Bare terms are subjects, `!term` / `-term` exclude, and an unrecognised
`field:value` pair is kept verbatim as a ranking term — so a path such as
`src/lib.rs:12` never fails to parse.

```sh
alphagrep trace subject:auth_status relation:rendered support:ui
alphagrep trace subject:render_status path:src kind:fn
alphagrep trace subject:auth '!legacy'             # exclude a term
alphagrep trace subject:auth --debug-plan           # show how the query was read
```

> Use the `!term` form for exclusions. A leading `-term` is read by the
> argument parser as a flag, so it has to be passed after `--`.

## Common options

| Option | Applies to | Meaning |
| --- | --- | --- |
| `-p, --path <PATH>` | all | Root to search. Defaults to the current directory. |
| `-t, --type <TYPE>` | grep, find, trace | Restrict to a file type (`rs`, `py`, `ts`, `md`, …). |
| `-g, --glob <GLOB>` | grep, find, trace | Only consider paths matching the glob. |
| `--hidden` | grep, find, trace | Include hidden files and directories. |
| `--no-ignore` | grep, find, trace | Search files that ignore files would normally skip. |
| `--json` | all | Emit machine-readable JSON instead of the human-readable report. |
| `--paths-only` | grep, find, trace | Print only the ranked file paths. |
| `--context-json <FILE>` | outline, trace | Session harness context written by the calling agent. |

## Exit codes

`alphagrep` follows ripgrep: `0` when something matched, `1` when nothing did,
and `2` for errors — so it drops into shell pipelines unchanged.

```sh
if alphagrep grep "TODO" --paths-only > changed.txt; then
  echo "found matches"
fi
```

## Library

Every mode is available in-process; the CLI argument structs double as the
public request types.

```rust
use alphagrep::cli::GrepArgs;
use alphagrep::search::run_grep;

let args = GrepArgs {
    query: "render_frame".to_string(),
    regex: false,
    file_type: Some("rs".to_string()),
    json: false,
    paths_only: false,
    hidden: false,
    no_ignore: false,
    path: None,
    glob: None,
};
let result = run_grep(std::path::Path::new("."), &args).expect("search failed");
println!(
    "{} matches in {} files",
    result.total_matches, result.total_files
);
```

## License

MIT
