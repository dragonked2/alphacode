//! Trace engine: the `trace` mode (alias `smart`).
//!
//! Trace is deliberately different from grep: it starts from a small description
//! of *what* the caller is looking for (`subject:`, `relation:`, `support:`) and
//! returns an investigation packet — which file answers the question, which
//! structure that file has, and which regions carry the evidence, with whole
//! bodies when they are small enough to be worth reading.
//!
//! Scoring is split into named buckets (name, structure, content, support,
//! relation, session context) so `--debug-score` can explain every ranking
//! decision. Every bucket is a plain additive weight; nothing here is a model.

use crate::cli::{FullRegionMode, SmartArgs};
use crate::fsutil::{self, SearchScope};
use crate::harness::HarnessContext;
use crate::smart_dsl::SmartQuery;
use crate::structure::{self, StructureItem};
use globset::Glob;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Match lines recorded per file.
pub const MAX_LINE_HITS_PER_FILE: usize = 200;
/// Structure items listed per file.
pub const MAX_STRUCTURE_ITEMS_PER_FILE: usize = 8;
/// Structural regions considered per file.
pub const MAX_STRUCTURAL_REGIONS: usize = 12;
/// Bodies longer than this are demoted to snippets.
pub const MAX_FULL_REGION_LINES: usize = 160;
/// In `auto` mode, symbol regions up to this size are printed whole.
pub const AUTO_FULL_REGION_LINES: usize = 80;
/// Lines of context around a matched line in a snippet.
pub const SNIPPET_RADIUS: usize = 2;
/// Snippet windows kept per region.
pub const MAX_SNIPPET_WINDOWS: usize = 3;
/// Snippet lines kept per region.
pub const MAX_SNIPPET_LINES: usize = 24;
/// Longest source line stored in a body.
pub const MAX_BODY_LINE_LENGTH: usize = 200;
/// Matched lines closer than this merge into one region.
pub const MAX_CLUSTER_GAP: usize = 4;

/// One body line; `line == 0` marks an elision divider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegionLine {
    /// 1-based source line number, or 0 for a divider.
    pub line: usize,
    /// Source text.
    pub text: String,
}

/// One evidence region inside a file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SmartRegion {
    /// Region label, safe to render next to ` @ start-end`.
    pub label: String,
    /// Declaration kind, or `match` for grouped matches.
    pub kind: Option<String>,
    /// First line of the region.
    pub start_line: usize,
    /// Last line of the region.
    pub end_line: usize,
    /// Lines inside the region that matched a subject term.
    pub matched_lines: Vec<usize>,
    /// Lines to print under `full region:` or `snippet:`.
    pub body: Vec<RegionLine>,
    /// Whether `body` is the whole region.
    pub full: bool,
    /// Region score; higher is better.
    pub score: f32,
}

/// One ranked file in a trace result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SmartFile {
    /// Search-root-relative path.
    pub path: String,
    /// File score; higher is better.
    pub score: f32,
    /// Scoring reasons, most informative first.
    pub reasons: Vec<String>,
    /// Structure section shown for the file.
    pub structure: Vec<StructureItem>,
    /// Evidence regions shown for the file.
    pub regions: Vec<SmartRegion>,
}

/// Headline numbers for a trace result.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SmartSummary {
    /// Files returned.
    pub total_files: usize,
    /// Regions returned.
    pub total_regions: usize,
    /// Best-ranked file.
    pub best_file: Option<String>,
    /// Files inspected.
    pub scanned: usize,
    /// Diagnostic text.
    pub note: Option<String>,
}

/// Result of a trace run.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SmartResult {
    /// Subject terms as searched.
    pub query_text: String,
    /// How the query was interpreted.
    pub plan: Vec<String>,
    /// Ranked files.
    pub files: Vec<SmartFile>,
    /// Headline numbers.
    pub summary: SmartSummary,
}

/// Which kind of connection a `relation:` term asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelationBucket {
    Definition,
    Usage,
    UserFacing,
    Tests,
    Config,
}

#[derive(Debug, Clone, Copy, Default)]
struct RelationWeights {
    definition: f32,
    usage: f32,
    user_facing: f32,
    tests: f32,
    config: f32,
}

impl RelationWeights {
    fn set(&mut self, bucket: RelationBucket, value: f32) {
        match bucket {
            RelationBucket::Definition => self.definition = value,
            RelationBucket::Usage => self.usage = value,
            RelationBucket::UserFacing => self.user_facing = value,
            RelationBucket::Tests => self.tests = value,
            RelationBucket::Config => self.config = value,
        }
    }

    fn value(&self, bucket: RelationBucket) -> f32 {
        match bucket {
            RelationBucket::Definition => self.definition,
            RelationBucket::Usage => self.usage,
            RelationBucket::UserFacing => self.user_facing,
            RelationBucket::Tests => self.tests,
            RelationBucket::Config => self.config,
        }
    }

    fn is_empty(&self) -> bool {
        self.definition == 0.0
            && self.usage == 0.0
            && self.user_facing == 0.0
            && self.tests == 0.0
            && self.config == 0.0
    }

    fn from_relations(
        relations: &[String],
        support_terms: &mut Vec<String>,
        notes: &mut Vec<String>,
    ) -> Self {
        let mut weights = Self::default();
        for relation in relations {
            let lowered = relation.to_ascii_lowercase();
            match classify_relation(&lowered) {
                Some(bucket) => weights.set(bucket, 12.0),
                None => {
                    support_terms.push(lowered);
                    notes.push(format!(
                        "relation {relation:?} is not a known relation; treated as a support term"
                    ));
                }
            }
        }
        weights
    }
}

fn classify_relation(lowered: &str) -> Option<RelationBucket> {
    let starts_with_any =
        |prefixes: &[&str]| prefixes.iter().any(|prefix| lowered.starts_with(prefix));
    if starts_with_any(&["defin", "declar", "implement", "type"]) {
        Some(RelationBucket::Definition)
    } else if starts_with_any(&["call", "invok", "use", "refer", "import", "access", "read"]) {
        Some(RelationBucket::Usage)
    } else if starts_with_any(&[
        "render", "display", "shown", "show", "ui", "label", "string", "text", "message",
    ]) {
        Some(RelationBucket::UserFacing)
    } else if starts_with_any(&["test", "spec", "fixture"]) {
        Some(RelationBucket::Tests)
    } else if starts_with_any(&["config", "setting", "option", "env", "flag", "default"]) {
        Some(RelationBucket::Config)
    } else {
        None
    }
}

#[derive(Debug, Default)]
struct Plan {
    subjects_display: Vec<String>,
    /// Lowercased subjects, longest first, so the most specific term wins.
    subject_terms: Vec<String>,
    support_terms: Vec<String>,
    relations: Vec<String>,
    kinds: Vec<String>,
    paths: Vec<String>,
    exclude: Vec<String>,
    weights: RelationWeights,
    notes: Vec<String>,
}

impl Plan {
    fn build(
        query: &SmartQuery,
        args: &SmartArgs,
        scope: &SearchScope,
        context_present: bool,
    ) -> Self {
        let subjects_display = query.positive_terms();
        let mut subject_terms: Vec<String> = subjects_display
            .iter()
            .map(|term| term.to_ascii_lowercase())
            .collect();
        subject_terms.sort_by(|left, right| {
            right
                .chars()
                .count()
                .cmp(&left.chars().count())
                .then_with(|| left.cmp(right))
        });
        subject_terms.dedup();

        let mut support_terms: Vec<String> = query
            .supports
            .iter()
            .map(|term| term.to_ascii_lowercase())
            .collect();
        support_terms.sort();
        support_terms.dedup();

        let mut notes = Vec::new();
        let weights =
            RelationWeights::from_relations(&query.relations, &mut support_terms, &mut notes);

        if subject_terms.is_empty() && !support_terms.is_empty() {
            subject_terms = std::mem::take(&mut support_terms);
            notes.push("no explicit subject; support terms were promoted to subjects".to_string());
        }

        let kinds: Vec<String> = query
            .kinds
            .iter()
            .map(|kind| kind.to_ascii_lowercase())
            .collect();
        let paths: Vec<String> = query
            .paths
            .iter()
            .map(|path| normalise_path_filter(path))
            .collect();
        let exclude: Vec<String> = query
            .exclude
            .iter()
            .map(|term| term.to_ascii_lowercase())
            .collect();

        let mut plan = Self {
            subjects_display,
            subject_terms,
            support_terms,
            relations: query.relations.clone(),
            kinds,
            paths,
            exclude,
            weights,
            notes,
        };
        plan.notes.push(format!("scope: {}", scope.describe()));
        plan.notes
            .push(format!("full_region: {}", args.full_region));
        plan.notes.push(format!(
            "session context: {}",
            if context_present { "loaded" } else { "none" }
        ));
        plan
    }

    /// Longest subject term present in an already-lowercased haystack.
    fn longest_term_in(&self, haystack_lower: &str) -> Option<&str> {
        self.subject_terms
            .iter()
            .find(|term| term_in_line(haystack_lower, term.as_str()))
            .map(String::as_str)
    }

    fn kind_ok(&self, kind: &str) -> bool {
        if self.kinds.is_empty() {
            return true;
        }
        let lowered = kind.to_ascii_lowercase();
        self.kinds
            .iter()
            .any(|wanted| wanted == &lowered || wanted == "any" || wanted == "symbol")
    }

    fn path_ok(&self, relative: &str) -> bool {
        if self.paths.is_empty() {
            return true;
        }
        let lowered = relative.to_ascii_lowercase();
        self.paths.iter().any(|filter| {
            if filter.contains('*') || filter.contains('?') {
                Glob::new(filter)
                    .map(|glob| glob.compile_matcher().is_match(&lowered))
                    .unwrap_or(false)
            } else {
                lowered.contains(filter.as_str())
            }
        })
    }

    fn excluded(&self, relative: &str, text: &str) -> bool {
        if self.exclude.is_empty() {
            return false;
        }
        let lowered_path = relative.to_ascii_lowercase();
        let lowered_text = text.to_ascii_lowercase();
        self.exclude.iter().any(|term| {
            lowered_path.contains(term.as_str())
                || (term.chars().count() >= 3 && lowered_text.contains(term.as_str()))
        })
    }

    fn describe(&self) -> Vec<String> {
        let mut lines = vec![
            format!("subject={:?}", self.subjects_display),
            format!("relation={:?}", self.relations),
            format!("support={:?}", self.support_terms),
            format!("path={:?}", self.paths),
            format!("kind={:?}", self.kinds),
            format!("exclude={:?}", self.exclude),
        ];
        lines.extend(self.notes.iter().cloned());
        lines
    }
}

#[derive(Debug, Default)]
struct Score {
    name: f32,
    structure: f32,
    content: f32,
    support: f32,
    relation: f32,
    context: f32,
    reasons: Vec<String>,
}

impl Score {
    fn total(&self) -> f32 {
        self.name + self.structure + self.content + self.support + self.relation + self.context
    }
}

#[derive(Debug)]
struct Candidate {
    absolute: PathBuf,
    path: String,
    score: f32,
    reasons: Vec<String>,
    structure: Vec<StructureItem>,
    regions: Vec<SmartRegion>,
}

/// Trace `query` across `root`.
pub fn run_smart(root: &Path, query: &SmartQuery, args: &SmartArgs) -> Result<SmartResult, String> {
    if query.is_empty() {
        return Err("trace query needs at least one subject term".to_string());
    }

    let scope = SearchScope::new(root)
        .with_glob(args.glob.as_deref())?
        .with_file_type(args.file_type.as_deref())?
        .with_flags(args.hidden, args.no_ignore);
    if !scope.root.exists() {
        return Err(format!(
            "search root {} does not exist",
            scope.root.display()
        ));
    }

    let context = HarnessContext::load(args.context_json.as_deref());
    let plan = Plan::build(query, args, &scope, context.is_some());
    let want_regions = !args.paths_only;

    let mut scanned = 0usize;
    let mut candidates: Vec<Candidate> = Vec::new();
    for file in fsutil::collect_files(&scope) {
        if !plan.path_ok(&file.relative) {
            continue;
        }
        let Some(text) = fsutil::read_text(&file.absolute) else {
            continue;
        };
        scanned += 1;
        if plan.excluded(&file.relative, &text) {
            continue;
        }
        if let Some(candidate) = scan_file(&file, &text, &plan, context.as_ref(), args) {
            candidates.push(candidate);
        }
    }

    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.path.cmp(&right.path))
    });

    let matched_files = candidates.len();
    let file_limit = args.max_files.max(1);
    let truncated_files = matched_files > file_limit;
    candidates.truncate(file_limit);

    let budget = args.max_regions.max(1);
    let per_file = budget.div_ceil(candidates.len().max(1));
    let mut remaining = budget;
    if want_regions {
        for candidate in &mut candidates {
            candidate.regions.sort_by(|left, right| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.start_line.cmp(&right.start_line))
            });
            let take = per_file.min(remaining).min(candidate.regions.len());
            candidate.regions.truncate(take);
            remaining = remaining.saturating_sub(take);
            candidate.regions.sort_by_key(|region| region.start_line);
        }
    }

    let mut files: Vec<SmartFile> = Vec::new();
    let mut total_regions = 0usize;
    for candidate in candidates {
        let Candidate {
            absolute,
            path,
            score,
            reasons,
            structure,
            mut regions,
        } = candidate;
        if want_regions {
            expand_full_regions(&absolute, &mut regions);
        } else {
            regions.clear();
        }
        total_regions += regions.len();
        files.push(SmartFile {
            path,
            score,
            reasons,
            structure,
            regions,
        });
    }

    let note = if files.is_empty() {
        Some(format!(
            "no file matched {} (scanned {scanned} files); try a shorter subject such as subject:auth, add support: terms, or widen path:/glob:",
            plan.subjects_display.join(" ")
        ))
    } else if truncated_files {
        Some(format!(
            "{matched_files} files scored; showing the top {}",
            files.len()
        ))
    } else {
        None
    };

    let mut plan_lines = plan.describe();
    if !want_regions {
        plan_lines.push("paths_only: evidence regions are not collected".to_string());
    }

    let total_files = files.len();
    let best_file = files.first().map(|file| file.path.clone());
    Ok(SmartResult {
        query_text: describe_query(query),
        plan: plan_lines,
        files,
        summary: SmartSummary {
            total_files,
            total_regions,
            best_file,
            scanned,
            note,
        },
    })
}

fn scan_file(
    file: &fsutil::WalkedFile,
    text: &str,
    plan: &Plan,
    context: Option<&HarnessContext>,
    args: &SmartArgs,
) -> Option<Candidate> {
    let lines: Vec<&str> = text.lines().collect();
    let lowered: Vec<String> = lines.iter().map(|line| line.to_ascii_lowercase()).collect();

    let name = file
        .relative
        .rsplit('/')
        .next()
        .unwrap_or(&file.relative)
        .to_ascii_lowercase();
    let words = name_words(&name);

    let mut score = Score::default();

    // Name signal: the caller may be asking for a file rather than a symbol.
    let name_matches: Vec<&str> = plan
        .subject_terms
        .iter()
        .filter(|term| name_matches_token(&name, &words, term.as_str()))
        .map(String::as_str)
        .collect();
    if !name_matches.is_empty() {
        let points = name_matches.len() as f32 * 18.0;
        score.name += points;
        score.reasons.push(format!(
            "file name matches {:?} (+{points:.0})",
            name_matches
        ));
    }

    // Structure signal: a declaration named after the subject is strong
    // evidence that the file answers the question.
    let items = structure::extract_structure(text, Path::new(&file.relative));
    let mut structural_hits: Vec<(&StructureItem, String)> = Vec::new();
    for item in &items {
        if !plan.kind_ok(item.kind) {
            continue;
        }
        let lowered_label = item.label.to_ascii_lowercase();
        if let Some(term) = plan.longest_term_in(&lowered_label) {
            structural_hits.push((item, term.to_string()));
        }
    }
    if !structural_hits.is_empty() {
        let points = (structural_hits.len() as f32 * 30.0).min(60.0);
        score.structure += points;
        let labels: Vec<&str> = structural_hits
            .iter()
            .take(3)
            .map(|(item, _)| item.label.as_str())
            .collect();
        score.reasons.push(format!(
            "symbols match: {} (+{points:.0})",
            labels.join(", ")
        ));
    }

    // Content signal: how much of the file is about the subject.
    let mut matched_lines: Vec<usize> = Vec::new();
    let mut hit_terms: BTreeSet<String> = BTreeSet::new();
    let mut usage_evidence = false;
    for (index, line) in lowered.iter().enumerate() {
        let Some(term) = plan.longest_term_in(line) else {
            continue;
        };
        hit_terms.insert(term.to_string());
        if matched_lines.len() < MAX_LINE_HITS_PER_FILE {
            matched_lines.push(index + 1);
        }
        if line.contains('(') || line.contains('.') || line.contains("::") || line.contains('=') {
            usage_evidence = true;
        }
    }
    // User-facing evidence is read from the neighbourhood of a match: a subject
    // rendered with `format!` or `println!` sits a line away from the string
    // literal that carries it, so the exact hit line is too narrow a window.
    let user_facing_evidence = matched_lines.iter().take(40).any(|hit| {
        let begin = hit.saturating_sub(SNIPPET_RADIUS).max(1);
        let finish = (hit + SNIPPET_RADIUS).min(lowered.len());
        (begin..=finish).any(|line_number| {
            lowered
                .get(line_number.saturating_sub(1))
                .is_some_and(|line| line.contains('"') || line.contains('\'') || line.contains('`'))
        })
    });
    if !matched_lines.is_empty() {
        let density = (matched_lines.len() as f32 * 0.6).min(18.0);
        let points = hit_terms.len() as f32 * 10.0 + density;
        score.content += points;
        score.reasons.push(format!(
            "{} subject term(s) on {} line(s) (+{points:.0})",
            hit_terms.len(),
            matched_lines.len()
        ));
    }

    // Subjects are the contract: a file must carry at least one subject signal
    // somewhere (name, symbol, or content) before supports may boost it.
    if score.name == 0.0 && score.structure == 0.0 && score.content == 0.0 {
        return None;
    }

    let support_matches: Vec<&str> = plan
        .support_terms
        .iter()
        .filter(|term| lowered.iter().any(|line| term_in_line(line, term.as_str())))
        .map(String::as_str)
        .collect();
    if !support_matches.is_empty() {
        let points = support_matches.len() as f32 * 8.0;
        score.support += points;
        score.reasons.push(format!(
            "support terms present: {:?} (+{points:.0})",
            support_matches
        ));
    }

    if !plan.weights.is_empty() {
        let mut points = 0.0f32;
        let mut evidence: Vec<&str> = Vec::new();
        if plan.weights.value(RelationBucket::Definition) > 0.0 && !structural_hits.is_empty() {
            points += plan.weights.value(RelationBucket::Definition);
            evidence.push("definition");
        }
        if plan.weights.value(RelationBucket::Usage) > 0.0 && usage_evidence {
            points += plan.weights.value(RelationBucket::Usage);
            evidence.push("usage");
        }
        if plan.weights.value(RelationBucket::UserFacing) > 0.0 && user_facing_evidence {
            points += plan.weights.value(RelationBucket::UserFacing);
            evidence.push("user-facing text");
        }
        if plan.weights.value(RelationBucket::Tests) > 0.0 && is_test_path(&file.relative) {
            points += plan.weights.value(RelationBucket::Tests);
            evidence.push("test path");
        }
        if plan.weights.value(RelationBucket::Config) > 0.0 && is_config_path(&file.relative) {
            points += plan.weights.value(RelationBucket::Config);
            evidence.push("config path");
        }
        if points > 0.0 {
            score.relation += points;
            score.reasons.push(format!(
                "relation evidence: {} (+{points:.0})",
                evidence.join(", ")
            ));
        }
    }

    if let Some(context) = context {
        let focus = context.focus_boost(&file.relative);
        if focus > 0.0 {
            score.context += focus;
            score
                .reasons
                .push(format!("session focus file (+{focus:.0})"));
        }
        let known = context.known_file_boost(&file.relative);
        if known > 0.0 {
            score.context += known;
            score
                .reasons
                .push(format!("already read this session (+{known:.0})"));
        }
        let symbol = items
            .iter()
            .map(|item| context.symbol_boost(&file.relative, &item.label))
            .fold(0.0f32, |best, value| best.max(value));
        if symbol > 0.0 {
            score.context += symbol;
            score
                .reasons
                .push(format!("symbol already in context (+{symbol:.0})"));
        }
    }

    let total = score.total();
    if total <= 0.0 {
        return None;
    }

    let regions = if args.paths_only {
        Vec::new()
    } else {
        build_regions(&lines, &items, &matched_lines, plan, args.full_region)
    };
    let structure_items = select_structure(&items, plan);

    Some(Candidate {
        absolute: file.absolute.clone(),
        path: file.relative.clone(),
        score: total,
        reasons: score.reasons.into_iter().take(8).collect(),
        structure: structure_items,
        regions,
    })
}

fn build_regions(
    lines: &[&str],
    items: &[StructureItem],
    matched_lines: &[usize],
    plan: &Plan,
    mode: FullRegionMode,
) -> Vec<SmartRegion> {
    let mut regions: Vec<SmartRegion> = Vec::new();

    for item in items {
        if !plan.kind_ok(item.kind) {
            continue;
        }
        let lowered_label = item.label.to_ascii_lowercase();
        let label_term = plan.longest_term_in(&lowered_label);
        let hits: Vec<usize> = matched_lines
            .iter()
            .copied()
            .filter(|line| *line >= item.start_line && *line <= item.end_line)
            .collect();
        if label_term.is_none() && hits.is_empty() {
            continue;
        }
        let points = 8.0 + hits.len() as f32 * 3.0 + if label_term.is_some() { 30.0 } else { 0.0 };
        let mut region = SmartRegion {
            label: structure::safe_label(&item.label),
            kind: Some(item.kind.to_string()),
            start_line: item.start_line,
            end_line: item.end_line,
            matched_lines: hits.clone(),
            body: snippet_body(lines, &hits, item.start_line, item.end_line),
            full: false,
            score: points,
        };
        region.full = wanted_full(&region, mode);
        regions.push(region);
    }

    regions.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.start_line.cmp(&right.start_line))
    });
    regions.truncate(MAX_STRUCTURAL_REGIONS);

    let claimed: Vec<(usize, usize)> = regions
        .iter()
        .map(|region| (region.start_line, region.end_line))
        .collect();

    // Matches outside every structure region still deserve a snippet, so they
    // are grouped into regions of their own.
    let mut unclaimed: Vec<usize> = matched_lines
        .iter()
        .copied()
        .filter(|line| {
            !claimed
                .iter()
                .any(|(start, end)| line >= start && line <= end)
        })
        .collect();
    unclaimed.sort_unstable();

    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for line in unclaimed {
        let merged = match clusters.last_mut() {
            Some(cluster) => match cluster.last().copied() {
                Some(previous) if line.saturating_sub(previous) <= MAX_CLUSTER_GAP => {
                    cluster.push(line);
                    true
                }
                _ => false,
            },
            None => false,
        };
        if !merged {
            clusters.push(vec![line]);
        }
    }

    for cluster in clusters {
        let start = cluster[0];
        let end = cluster.last().copied().unwrap_or(start);
        let term = cluster.iter().find_map(|line| {
            let text = lines.get(line.saturating_sub(1)).copied().unwrap_or("");
            let lowered = text.to_ascii_lowercase();
            plan.longest_term_in(&lowered).map(str::to_string)
        });
        let label = match &term {
            Some(term) => format!("match {term}"),
            None => "match".to_string(),
        };
        let mut region = SmartRegion {
            label: structure::safe_label(&label),
            kind: Some("match".to_string()),
            start_line: start,
            end_line: end,
            matched_lines: cluster.clone(),
            body: snippet_body(lines, &cluster, start, end),
            full: false,
            score: 8.0 + cluster.len() as f32 * 2.0,
        };
        region.full = wanted_full(&region, mode);
        regions.push(region);
    }

    regions
}

fn wanted_full(region: &SmartRegion, mode: FullRegionMode) -> bool {
    match mode {
        FullRegionMode::Always => true,
        FullRegionMode::Never => false,
        FullRegionMode::Auto => {
            region.kind.as_deref() != Some("match")
                && region.end_line.saturating_sub(region.start_line) < AUTO_FULL_REGION_LINES
        }
    }
}

fn snippet_body(
    lines: &[&str],
    hits: &[usize],
    start_line: usize,
    end_line: usize,
) -> Vec<RegionLine> {
    if hits.is_empty() {
        return Vec::new();
    }
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for line in hits {
        let begin = line.saturating_sub(SNIPPET_RADIUS).max(start_line);
        let finish = (line + SNIPPET_RADIUS).min(end_line);
        if let Some(window) = windows.last_mut()
            && begin <= window.1 + 1
        {
            window.1 = window.1.max(finish);
            continue;
        }
        if windows.len() < MAX_SNIPPET_WINDOWS {
            windows.push((begin, finish));
        }
    }

    let mut body: Vec<RegionLine> = Vec::new();
    for (index, (begin, finish)) in windows.iter().enumerate() {
        if index > 0 {
            body.push(RegionLine {
                line: 0,
                text: "...".to_string(),
            });
        }
        for line_number in *begin..=*finish {
            if body.len() >= MAX_SNIPPET_LINES {
                body.push(RegionLine {
                    line: 0,
                    text: "... (snippet clipped)".to_string(),
                });
                return body;
            }
            let text = lines
                .get(line_number.saturating_sub(1))
                .copied()
                .unwrap_or("");
            body.push(RegionLine {
                line: line_number,
                text: clip(text.trim_end()),
            });
        }
    }
    body
}

fn expand_full_regions(absolute: &Path, regions: &mut [SmartRegion]) {
    if !regions.iter().any(|region| region.full) {
        return;
    }
    let Some(text) = fsutil::read_text(absolute) else {
        for region in regions.iter_mut() {
            region.full = false;
        }
        return;
    };
    let lines: Vec<&str> = text.lines().collect();
    for region in regions.iter_mut() {
        if !region.full {
            continue;
        }
        let length = region.end_line.saturating_sub(region.start_line) + 1;
        if length > MAX_FULL_REGION_LINES {
            region.full = false;
            region.body = snippet_body(
                &lines,
                &region.matched_lines,
                region.start_line,
                region.end_line,
            );
            continue;
        }
        let mut body = Vec::new();
        for line_number in region.start_line..=region.end_line {
            let text = lines
                .get(line_number.saturating_sub(1))
                .copied()
                .unwrap_or("");
            body.push(RegionLine {
                line: line_number,
                text: clip(text.trim_end()),
            });
        }
        region.body = body;
    }
}

fn select_structure(items: &[StructureItem], plan: &Plan) -> Vec<StructureItem> {
    let mut matched: Vec<StructureItem> = Vec::new();
    let mut rest: Vec<StructureItem> = Vec::new();
    for item in items {
        if !plan.kind_ok(item.kind) {
            continue;
        }
        let lowered = item.label.to_ascii_lowercase();
        if plan.longest_term_in(&lowered).is_some() {
            matched.push(item.clone());
        } else {
            rest.push(item.clone());
        }
    }
    matched.extend(rest);
    matched.truncate(MAX_STRUCTURE_ITEMS_PER_FILE);
    matched.sort_by_key(|item| item.start_line);
    matched
}

fn describe_query(query: &SmartQuery) -> String {
    let positives = query.positive_terms();
    if positives.is_empty() {
        query.describe()
    } else {
        positives.join(" ")
    }
}

fn normalise_path_filter(path: &str) -> String {
    let lowered = path.trim().to_ascii_lowercase().replace('\\', "/");
    let trimmed = lowered.strip_prefix("./").unwrap_or(&lowered).to_string();
    trimmed.trim_end_matches('/').to_string()
}

/// Whether a lowercased line contains a term, with whole-word matching for
/// very short terms so that `subject:ui` does not match `build`.
fn term_in_line(line_lower: &str, term: &str) -> bool {
    if term.chars().count() >= 3 {
        return line_lower.contains(term);
    }
    line_lower
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|word| word == term)
}

fn name_matches_token(name: &str, words: &[String], term: &str) -> bool {
    let short = term.chars().count() < 3;
    words.iter().any(|word| word == term) || (!short && name.contains(term))
}

/// Split an identifier-ish string into lowercase words.
fn name_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_lowercase = false;
    for character in text.chars() {
        if character.is_alphanumeric() {
            let is_upper = character.is_uppercase();
            if is_upper && previous_lowercase && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(character.to_ascii_lowercase());
            previous_lowercase = character.is_lowercase() || character.is_numeric();
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
            previous_lowercase = false;
        } else {
            previous_lowercase = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_test_path(relative: &str) -> bool {
    let lowered = relative.to_ascii_lowercase();
    lowered.starts_with("tests/")
        || lowered.contains("/tests/")
        || lowered.contains("test")
        || lowered.contains("spec")
}

fn is_config_path(relative: &str) -> bool {
    let lowered = relative.to_ascii_lowercase();
    let name = lowered.rsplit('/').next().unwrap_or(&lowered);
    if name.contains("config") || name.contains("settings") || name.starts_with(".env") {
        return true;
    }
    matches!(
        lowered.rsplit_once('.').map(|(_, extension)| extension),
        Some("toml" | "yaml" | "yml" | "json" | "ini" | "cfg" | "conf" | "env" | "properties")
    )
}

/// Clip a body line to a readable width.
fn clip(text: &str) -> String {
    crate::search::clip_to(text, MAX_BODY_LINE_LENGTH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart_dsl::parse_smart_query;
    use std::fs;

    fn args() -> SmartArgs {
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
            no_ignore: true,
            context_json: None,
        }
    }

    fn query(terms: &[&str]) -> SmartQuery {
        let owned: Vec<String> = terms.iter().map(|term| term.to_string()).collect();
        parse_smart_query(&owned).expect("query")
    }

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(
            dir.path().join("src/auth.rs"),
            concat!(
                "pub struct AuthStatus {\n",
                "    signed_in: bool,\n",
                "}\n",
                "\n",
                "pub fn render_status(status: &AuthStatus) -> String {\n",
                "    format!(\"signed in: {}\", status.signed_in)\n",
                "}\n",
                "\n",
                "// signed_in is cached for the session\n",
            ),
        )
        .expect("write");
        fs::write(dir.path().join("src/unrelated.rs"), "pub fn other() {}\n").expect("write");
        dir
    }

    #[test]
    fn the_defining_file_ranks_first_and_reports_its_symbol() {
        let dir = fixture();
        let result =
            run_smart(dir.path(), &query(&["subject:render_status"]), &args()).expect("trace");
        assert_eq!(result.files[0].path, "src/auth.rs");
        assert!(result.summary.best_file.as_deref() == Some("src/auth.rs"));
        let labels: Vec<&str> = result.files[0]
            .structure
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert!(labels.contains(&"render_status"));
        let region = result.files[0]
            .regions
            .iter()
            .find(|region| region.label == "render_status")
            .expect("region");
        assert_eq!((region.start_line, region.end_line), (5, 7));
        assert!(region.full, "auto mode prints small symbol bodies whole");
        assert_eq!(region.body.len(), 3);
    }

    #[test]
    fn bare_matches_get_a_grouped_region_with_a_snippet() {
        let dir = fixture();
        let result = run_smart(dir.path(), &query(&["subject:signed_in"]), &args()).expect("trace");
        assert_eq!(result.files[0].path, "src/auth.rs");
        // Two hits sit inside symbols, so they are reported as symbol regions;
        // the trailing comment is outside every symbol and clusters into a
        // `match` region that carries a snippet rather than a whole body.
        assert!(
            result.files[0]
                .regions
                .iter()
                .any(|region| region.label == "AuthStatus")
        );
        let clustered = result.files[0]
            .regions
            .iter()
            .find(|region| region.kind.as_deref() == Some("match"))
            .expect("a grouped match region for the line outside any symbol");
        assert_eq!(clustered.label, "match signed_in");
        assert!(!clustered.full);
    }

    #[test]
    fn relations_prefer_files_with_user_facing_text() {
        let dir = fixture();
        let result = run_smart(
            dir.path(),
            &query(&["subject:AuthStatus", "relation:rendered"]),
            &args(),
        )
        .expect("trace");
        let reasons = result.files[0].reasons.join(" | ");
        assert!(reasons.contains("relation evidence"), "{reasons}");
    }

    #[test]
    fn paths_only_returns_no_regions() {
        let dir = fixture();
        let mut paths_args = args();
        paths_args.paths_only = true;
        let result = run_smart(dir.path(), &query(&["subject:auth"]), &paths_args).expect("trace");
        assert!(result.files.iter().all(|file| file.regions.is_empty()));
    }

    #[test]
    fn absent_subjects_return_an_empty_result_with_a_note() {
        let dir = fixture();
        let result =
            run_smart(dir.path(), &query(&["subject:quaternion"]), &args()).expect("trace");
        assert!(result.files.is_empty());
        assert!(
            result
                .summary
                .note
                .expect("note")
                .contains("no file matched")
        );
    }

    #[test]
    fn kind_filters_restrict_the_structure_section() {
        let dir = fixture();
        let result = run_smart(
            dir.path(),
            &query(&["subject:auth", "kind:struct"]),
            &args(),
        )
        .expect("trace");
        assert!(
            result.files[0]
                .structure
                .iter()
                .all(|item| item.kind == "struct")
        );
    }
}
