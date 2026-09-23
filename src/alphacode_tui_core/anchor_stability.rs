//! Anchor-stability analysis for rendered transcript frames.
//!
//! Quantifies the most jarring kinds of visual motion in the chat transcript:
//! content that *repositions* relative to where it was anchored, content pushed
//! down by insertions above it, large blocks popping in within a single frame,
//! rows blinking out and back, and whole-screen reflows. Expected motion (user
//! scrolling, resizes, the uniform upward scroll while following the live
//! tail) is excluded so the report isolates surprises.
//!
//! The input is cheap: a per-row content hash of the messages area for each
//! rendered frame. Consecutive frames are aligned by voting on the vertical
//! offset of rows whose hash is unique in both frames (duplicate hashes, e.g.
//! blank or repeated lines, do not get to vote). The winning offset is the
//! "dominant shift"; rows that match at other offsets are *displaced* and rows
//! that vanished/appeared feed the pop/blink/reflow metrics.

use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Hash value used for a visually blank row. Blank rows are ignored for
/// alignment and change accounting (they carry no anchor information).
pub const BLANK_ROW_HASH: u64 = 0;

/// Maximum jarring events retained in the report log.
const EVENT_LOG_CAP: usize = 256;

/// Appeared-block size at or above which a single-frame insertion is flagged
/// as a "big pop" (rows appearing at once feel sharp past a few lines).
const BIG_POP_ROWS: usize = 5;

/// Maximum per-frame deviation from the dominant shift that still reads as
/// smooth motion. Collapse/expand animations move nearby content a row or two
/// per frame, which the eye tracks as a slide; anything farther is a jump.
const SLIDE_TOLERANCE: i32 = 2;

/// Fraction of previously visible rows that must survive a frame for it not to
/// be considered a mass reflow.
const MASS_CHANGE_SURVIVOR_FRACTION: f64 = 0.5;

/// Minimum non-blank rows in the previous frame before mass-reflow checking is
/// meaningful.
const MASS_CHANGE_MIN_ROWS: usize = 8;

/// One captured frame of the transcript viewport.
#[derive(Debug, Clone)]
pub struct AnchorFrame {
    /// Content hash per visible row, top to bottom. [`BLANK_ROW_HASH`] = blank.
    pub rows: Vec<u64>,
    /// Viewport width in cells (width changes imply rewrap; diffs are skipped).
    pub width: u16,
    /// App scroll offset (changes imply user scrolling; diffs are skipped).
    pub scroll_offset: usize,
    /// Whether the view is following the live tail (auto-scroll).
    pub following_tail: bool,
    /// Capture time.
    pub at: Instant,
}

/// Classified jarring event kinds, ordered roughly by how disruptive they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JarringKind {
    /// Rows moved to a different position out of step with the dominant shift:
    /// content reordered relative to its anchor (e.g. a block jumping from the
    /// middle of the transcript back to the bottom).
    Reposition,
    /// The dominant shift was downward: something was inserted *above* the
    /// content the user was reading, pushing everything down.
    InsertionAbove,
    /// A large contiguous block of rows appeared in a single frame.
    BigPop,
    /// Rows disappeared for one frame and came back (flicker/blink).
    Blink,
    /// Most of the screen changed at once without a resize or scroll.
    MassReflow,
}

/// One logged jarring event.
#[derive(Debug, Clone, Serialize)]
pub struct JarringEvent {
    pub kind: JarringKind,
    /// Milliseconds since the recorder was created/reset.
    pub at_ms: u64,
    /// Rows involved (displaced rows, popped block size, blinked rows, ...).
    pub rows: usize,
    /// Dominant shift at the time (negative = content moved up).
    pub dominant_shift: i32,
}

/// Per-frame alignment result between two consecutive frames.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AnchorDiff {
    /// Winning vertical offset for surviving rows (negative = moved up).
    pub dominant_shift: i32,
    /// Non-blank rows that matched at the dominant shift.
    pub matched_rows: usize,
    /// Non-blank rows that matched at a *different* offset (repositioned).
    pub displaced_rows: usize,
    /// Rows that moved within [`SLIDE_TOLERANCE`] of the dominant shift:
    /// smooth animation motion (collapse/expand), tracked but not jarring.
    pub sliding_rows: usize,
    /// Rows that stayed at the same screen position while the dominant shift
    /// was nonzero: viewport-pinned UI (footers, status rows). Tracked
    /// separately because staying still is usually intentional, unlike
    /// `displaced_rows` which truly jumped.
    pub stationary_rows: usize,
    /// Non-blank rows newly visible this frame.
    pub appeared_rows: usize,
    /// Largest contiguous block of appeared rows.
    pub largest_appeared_block: usize,
    /// Non-blank rows from the previous frame no longer visible.
    pub removed_rows: usize,
    /// Rows present two frames ago, gone last frame, back this frame.
    pub blinked_rows: usize,
    /// True when the diff was skipped (resize, scroll, first frame).
    pub skipped: bool,
}

/// Aggregated report across all observed frames.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorStabilityReport {
    pub frames_observed: u64,
    pub frames_compared: u64,
    pub frames_skipped_scroll: u64,
    pub frames_skipped_resize: u64,
    /// Frames in which any row changed at all (activity level).
    pub frames_with_changes: u64,
    pub reposition_events: u64,
    pub reposition_rows_total: u64,
    /// Rows that stayed screen-pinned while content scrolled (footers, status
    /// rows). Expected UI behavior; tracked to confirm they are excluded from
    /// reposition events.
    pub stationary_rows_total: u64,
    /// Rows that slid within tolerance of the dominant shift (animations).
    pub sliding_rows_total: u64,
    pub insertion_above_events: u64,
    pub big_pop_events: u64,
    pub blink_events: u64,
    pub mass_reflow_events: u64,
    /// Appeared-rows-per-changed-frame statistics: how big insertions are.
    pub appeared_rows_mean: f64,
    pub appeared_rows_p95: usize,
    pub appeared_rows_max: usize,
    /// Largest single appeared block seen.
    pub largest_appeared_block: usize,
    /// Observation span in milliseconds.
    pub span_ms: u64,
    /// Jarring events per minute of observed span (0 when span is tiny).
    pub jarring_events_per_minute: f64,
    /// Most recent jarring events, oldest first.
    pub recent_events: Vec<JarringEvent>,
}

/// Streaming recorder: feed it one [`AnchorFrame`] per rendered frame.
pub struct AnchorStabilityRecorder {
    started: Instant,
    prev: Option<AnchorFrame>,
    /// Unique non-blank hashes of the frame before `prev` (for blink checks).
    prev_prev_hashes: Option<HashMap<u64, usize>>,
    frames_observed: u64,
    frames_compared: u64,
    frames_skipped_scroll: u64,
    frames_skipped_resize: u64,
    frames_with_changes: u64,
    reposition_events: u64,
    reposition_rows_total: u64,
    stationary_rows_total: u64,
    sliding_rows_total: u64,
    insertion_above_events: u64,
    big_pop_events: u64,
    blink_events: u64,
    mass_reflow_events: u64,
    appeared_sizes: Vec<usize>,
    largest_appeared_block: usize,
    events: VecDeque<JarringEvent>,
}

impl Default for AnchorStabilityRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl AnchorStabilityRecorder {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            prev: None,
            prev_prev_hashes: None,
            frames_observed: 0,
            frames_compared: 0,
            frames_skipped_scroll: 0,
            frames_skipped_resize: 0,
            frames_with_changes: 0,
            reposition_events: 0,
            reposition_rows_total: 0,
            stationary_rows_total: 0,
            sliding_rows_total: 0,
            insertion_above_events: 0,
            big_pop_events: 0,
            blink_events: 0,
            mass_reflow_events: 0,
            appeared_sizes: Vec::new(),
            largest_appeared_block: 0,
            events: VecDeque::new(),
        }
    }

    /// Observe the next rendered frame. Returns the diff against the previous
    /// frame when one was comparable.
    pub fn observe(&mut self, frame: AnchorFrame) -> Option<AnchorDiff> {
        self.frames_observed += 1;
        let prev = match self.prev.take() {
            Some(prev) => prev,
            None => {
                self.prev = Some(frame);
                return None;
            }
        };

        // Expected-motion exclusions: resizes rewrap everything; scrolls move
        // everything deliberately. Track them but do not classify them.
        let mut diff = AnchorDiff::default();
        let comparable = if frame.width != prev.width {
            self.frames_skipped_resize += 1;
            false
        } else if frame.scroll_offset != prev.scroll_offset {
            self.frames_skipped_scroll += 1;
            false
        } else {
            true
        };

        if !comparable {
            diff.skipped = true;
            self.prev_prev_hashes = Some(unique_nonblank_hashes(&prev.rows));
            self.prev = Some(frame);
            return Some(diff);
        }

        self.frames_compared += 1;
        diff = align_frames(&prev.rows, &frame.rows);

        let changed = diff.appeared_rows > 0
            || diff.removed_rows > 0
            || diff.displaced_rows > 0
            || diff.dominant_shift != 0;
        if changed {
            self.frames_with_changes += 1;
        }

        let at_ms = self.started.elapsed().as_millis() as u64;

        // Blink: rows present two frames ago, missing last frame, back now.
        if let Some(pp) = &self.prev_prev_hashes {
            let prev_unique = unique_nonblank_hashes(&prev.rows);
            let cur_unique = unique_nonblank_hashes(&frame.rows);
            let blinked = cur_unique
                .keys()
                .filter(|h| pp.contains_key(*h) && !prev_unique.contains_key(*h))
                .count();
            diff.blinked_rows = blinked;
            if blinked > 0 {
                self.blink_events += 1;
                self.push_event(JarringEvent {
                    kind: JarringKind::Blink,
                    at_ms,
                    rows: blinked,
                    dominant_shift: diff.dominant_shift,
                });
            }
        }

        if diff.displaced_rows > 0 {
            self.reposition_events += 1;
            self.reposition_rows_total += diff.displaced_rows as u64;
            self.push_event(JarringEvent {
                kind: JarringKind::Reposition,
                at_ms,
                rows: diff.displaced_rows,
                dominant_shift: diff.dominant_shift,
            });
        }
        self.stationary_rows_total += diff.stationary_rows as u64;
        self.sliding_rows_total += diff.sliding_rows as u64;

        if diff.dominant_shift > 0 && diff.matched_rows > 0 {
            self.insertion_above_events += 1;
            self.push_event(JarringEvent {
                kind: JarringKind::InsertionAbove,
                at_ms,
                rows: diff.dominant_shift.unsigned_abs() as usize,
                dominant_shift: diff.dominant_shift,
            });
        }

        if diff.appeared_rows > 0 {
            self.appeared_sizes.push(diff.appeared_rows);
            self.largest_appeared_block =
                self.largest_appeared_block.max(diff.largest_appeared_block);
            if diff.largest_appeared_block >= BIG_POP_ROWS {
                self.big_pop_events += 1;
                self.push_event(JarringEvent {
                    kind: JarringKind::BigPop,
                    at_ms,
                    rows: diff.largest_appeared_block,
                    dominant_shift: diff.dominant_shift,
                });
            }
        }

        let prev_nonblank = prev.rows.iter().filter(|h| **h != BLANK_ROW_HASH).count();
        if prev_nonblank >= MASS_CHANGE_MIN_ROWS {
            let survivors = diff.matched_rows + diff.displaced_rows;
            if (survivors as f64) < (prev_nonblank as f64) * MASS_CHANGE_SURVIVOR_FRACTION {
                self.mass_reflow_events += 1;
                self.push_event(JarringEvent {
                    kind: JarringKind::MassReflow,
                    at_ms,
                    rows: prev_nonblank - survivors,
                    dominant_shift: diff.dominant_shift,
                });
            }
        }

        self.prev_prev_hashes = Some(unique_nonblank_hashes(&prev.rows));
        self.prev = Some(frame);
        Some(diff)
    }

    fn push_event(&mut self, event: JarringEvent) {
        if self.events.len() >= EVENT_LOG_CAP {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn report(&self) -> AnchorStabilityReport {
        let span_ms = self.started.elapsed().as_millis() as u64;
        let jarring_total = self.reposition_events
            + self.insertion_above_events
            + self.big_pop_events
            + self.blink_events
            + self.mass_reflow_events;
        let per_minute = if span_ms >= 1_000 {
            jarring_total as f64 / (span_ms as f64 / 60_000.0)
        } else {
            0.0
        };
        let mut sorted = self.appeared_sizes.clone();
        sorted.sort_unstable();
        let p95 = if sorted.is_empty() {
            0
        } else {
            sorted[((sorted.len() as f64 - 1.0) * 0.95).round() as usize]
        };
        AnchorStabilityReport {
            frames_observed: self.frames_observed,
            frames_compared: self.frames_compared,
            frames_skipped_scroll: self.frames_skipped_scroll,
            frames_skipped_resize: self.frames_skipped_resize,
            frames_with_changes: self.frames_with_changes,
            reposition_events: self.reposition_events,
            reposition_rows_total: self.reposition_rows_total,
            stationary_rows_total: self.stationary_rows_total,
            sliding_rows_total: self.sliding_rows_total,
            insertion_above_events: self.insertion_above_events,
            big_pop_events: self.big_pop_events,
            blink_events: self.blink_events,
            mass_reflow_events: self.mass_reflow_events,
            appeared_rows_mean: if self.appeared_sizes.is_empty() {
                0.0
            } else {
                self.appeared_sizes.iter().sum::<usize>() as f64 / self.appeared_sizes.len() as f64
            },
            appeared_rows_p95: p95,
            appeared_rows_max: sorted.last().copied().unwrap_or(0),
            largest_appeared_block: self.largest_appeared_block,
            span_ms,
            jarring_events_per_minute: per_minute,
            recent_events: self.events.iter().cloned().collect(),
        }
    }
}

/// Map of hash -> row index for hashes appearing exactly once (non-blank).
fn unique_nonblank_hashes(rows: &[u64]) -> HashMap<u64, usize> {
    let mut counts: HashMap<u64, (usize, usize)> = HashMap::new();
    for (idx, h) in rows.iter().enumerate() {
        if *h == BLANK_ROW_HASH {
            continue;
        }
        let entry = counts.entry(*h).or_insert((0, idx));
        entry.0 += 1;
    }
    counts
        .into_iter()
        .filter(|(_, (count, _))| *count == 1)
        .map(|(h, (_, idx))| (h, idx))
        .collect()
}

/// Align two row-hash frames: vote on the dominant vertical offset using rows
/// whose hash is unique in both frames, then classify every previous non-blank
/// row as matched (dominant offset), displaced (other offset), or removed, and
/// every new non-blank row as appeared.
fn align_frames(prev: &[u64], cur: &[u64]) -> AnchorDiff {
    let prev_unique = unique_nonblank_hashes(prev);
    let cur_unique = unique_nonblank_hashes(cur);

    // Offset votes from rows unique in both frames.
    let mut votes: HashMap<i32, usize> = HashMap::new();
    for (h, prev_idx) in &prev_unique {
        if let Some(cur_idx) = cur_unique.get(h) {
            let offset = *cur_idx as i32 - *prev_idx as i32;
            *votes.entry(offset).or_insert(0) += 1;
        }
    }
    // Dominant offset: most votes, ties broken toward zero (no motion).
    let dominant_shift = votes
        .iter()
        .max_by_key(|(offset, count)| (**count, std::cmp::Reverse(offset.unsigned_abs())))
        .map(|(offset, _)| *offset)
        .unwrap_or(0);

    let mut diff = AnchorDiff {
        dominant_shift,
        ..Default::default()
    };

    // Classify previous rows. Rows with duplicate hashes are checked only
    // against the dominant offset (ambiguous matches must not count as
    // displacement).
    let mut matched_cur_rows = vec![false; cur.len()];
    for (prev_idx, h) in prev.iter().enumerate() {
        if *h == BLANK_ROW_HASH {
            continue;
        }
        let dominant_target = prev_idx as i32 + dominant_shift;
        let at_dominant = dominant_target >= 0
            && (dominant_target as usize) < cur.len()
            && cur[dominant_target as usize] == *h;
        if at_dominant {
            diff.matched_rows += 1;
            matched_cur_rows[dominant_target as usize] = true;
            continue;
        }
        // Unique-in-both rows found elsewhere are classified by how far they
        // moved relative to the dominant shift; everything else (changed
        // content, duplicates) counts as removed. Rows that stayed at the
        // *same screen position* while everything else shifted are
        // viewport-pinned UI (footers, status rows). Rows within
        // SLIDE_TOLERANCE of the dominant shift are smooth animation motion.
        // Only rows beyond that are true jumps (displaced).
        if prev_unique.contains_key(h)
            && let Some(cur_idx) = cur_unique.get(h)
        {
            let offset = *cur_idx as i32 - prev_idx as i32;
            if offset == 0 && dominant_shift != 0 {
                diff.stationary_rows += 1;
            } else if (offset - dominant_shift).abs() <= SLIDE_TOLERANCE {
                diff.sliding_rows += 1;
            } else {
                diff.displaced_rows += 1;
            }
            matched_cur_rows[*cur_idx] = true;
            continue;
        }
        diff.removed_rows += 1;
    }

    // Appeared rows and the largest contiguous appeared block.
    let mut run = 0usize;
    for (idx, h) in cur.iter().enumerate() {
        if *h != BLANK_ROW_HASH && !matched_cur_rows[idx] {
            diff.appeared_rows += 1;
            run += 1;
            diff.largest_appeared_block = diff.largest_appeared_block.max(run);
        } else {
            run = 0;
        }
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rows: Vec<u64>) -> AnchorFrame {
        AnchorFrame {
            rows,
            width: 80,
            scroll_offset: 0,
            following_tail: true,
            at: Instant::now(),
        }
    }

    fn hashes(range: std::ops::Range<u64>) -> Vec<u64> {
        range.collect()
    }

    #[test]
    fn first_frame_yields_no_diff() {
        let mut rec = AnchorStabilityRecorder::new();
        assert!(rec.observe(frame(hashes(1..10))).is_none());
    }

    #[test]
    fn identical_frames_report_no_motion() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..10)));
        let diff = rec.observe(frame(hashes(1..10))).unwrap();
        assert_eq!(diff.dominant_shift, 0);
        assert_eq!(diff.displaced_rows, 0);
        assert_eq!(diff.appeared_rows, 0);
        assert_eq!(diff.removed_rows, 0);
        let report = rec.report();
        assert_eq!(report.reposition_events, 0);
        assert_eq!(report.frames_with_changes, 0);
    }

    #[test]
    fn tail_append_scrolls_up_without_jarring_events() {
        // Following the tail: rows shift up by 2, two new rows at the bottom.
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..11))); // rows 1..=10
        let diff = rec.observe(frame(hashes(3..13))).unwrap(); // rows 3..=12
        assert_eq!(diff.dominant_shift, -2);
        assert_eq!(diff.displaced_rows, 0);
        assert_eq!(diff.appeared_rows, 2);
        let report = rec.report();
        assert_eq!(report.reposition_events, 0);
        assert_eq!(report.insertion_above_events, 0);
    }

    #[test]
    fn block_jumping_to_bottom_is_a_reposition() {
        // Rows 1..8 stay anchored; block [100,101] moves from the middle to the
        // bottom (like a reasoning block jumping below new output). Rows 4/5/6
        // shift up by only 2 (within slide tolerance), but the block jumps +3.
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(vec![1, 2, 3, 100, 101, 4, 5, 6]));
        let diff = rec
            .observe(frame(vec![1, 2, 3, 4, 5, 6, 100, 101]))
            .unwrap();
        assert_eq!(diff.dominant_shift, 0, "anchored rows must win the vote");
        assert_eq!(diff.displaced_rows, 2, "100/101 jump beyond tolerance");
        assert_eq!(diff.sliding_rows, 3, "4/5/6 slide up within tolerance");
        let report = rec.report();
        assert_eq!(report.reposition_events, 1);
    }

    #[test]
    fn screen_pinned_footer_rows_are_stationary_not_repositioned() {
        // While the transcript scrolls up (tail-follow), bottom-pinned UI rows
        // (status footer, TPS line) stay at the same screen position. That is
        // intentional viewport-pinned behavior and must not count as a
        // reposition event.
        let mut rec = AnchorStabilityRecorder::new();
        // 8 transcript rows + 2 pinned footer rows (900, 901) at the bottom.
        let mut rows = hashes(1..9);
        rows.extend([900, 901]);
        rec.observe(frame(rows));
        // Transcript scrolls up by 2 (rows 3..=10 now visible); footer stays.
        let mut rows = hashes(3..11);
        rows.extend([900, 901]);
        let diff = rec.observe(frame(rows)).unwrap();
        assert_eq!(diff.dominant_shift, -2);
        assert_eq!(diff.stationary_rows, 2, "footer rows are stationary");
        assert_eq!(diff.displaced_rows, 0, "no true repositions");
        let report = rec.report();
        assert_eq!(report.reposition_events, 0);
        assert_eq!(report.stationary_rows_total, 2);
    }

    #[test]
    fn insertion_above_pushes_content_down() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..11)));
        // Three new rows appear on top; everything else pushed down.
        let mut rows = vec![100, 101, 102];
        rows.extend(hashes(1..8));
        let diff = rec.observe(frame(rows)).unwrap();
        assert_eq!(diff.dominant_shift, 3);
        let report = rec.report();
        assert_eq!(report.insertion_above_events, 1);
    }

    #[test]
    fn large_single_frame_block_is_a_big_pop() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..5)));
        let mut rows = hashes(1..5);
        rows.extend(hashes(100..108)); // 8 new contiguous rows at once
        let diff = rec.observe(frame(rows)).unwrap();
        assert_eq!(diff.largest_appeared_block, 8);
        let report = rec.report();
        assert_eq!(report.big_pop_events, 1);
    }

    #[test]
    fn small_incremental_appends_are_not_big_pops() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..5)));
        let mut rows = hashes(1..5);
        rows.push(100);
        rows.push(101);
        rec.observe(frame(rows)).unwrap();
        let report = rec.report();
        assert_eq!(report.big_pop_events, 0);
    }

    #[test]
    fn blink_detects_row_disappearing_and_returning() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(vec![1, 2, 3, 4]));
        rec.observe(frame(vec![1, 2, 4, 0])); // row 3 vanishes
        let diff = rec.observe(frame(vec![1, 2, 3, 4])).unwrap(); // row 3 returns
        assert_eq!(diff.blinked_rows, 1);
        let report = rec.report();
        assert_eq!(report.blink_events, 1);
    }

    #[test]
    fn scroll_change_skips_classification() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..11)));
        let mut scrolled = frame(hashes(50..60));
        scrolled.scroll_offset = 20;
        let diff = rec.observe(scrolled).unwrap();
        assert!(diff.skipped);
        let report = rec.report();
        assert_eq!(report.frames_skipped_scroll, 1);
        assert_eq!(report.reposition_events, 0);
        assert_eq!(report.mass_reflow_events, 0);
    }

    #[test]
    fn resize_skips_classification() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..11)));
        let mut resized = frame(hashes(100..110));
        resized.width = 120;
        let diff = rec.observe(resized).unwrap();
        assert!(diff.skipped);
        let report = rec.report();
        assert_eq!(report.frames_skipped_resize, 1);
        assert_eq!(report.mass_reflow_events, 0);
    }

    #[test]
    fn mass_reflow_detected_when_most_rows_change() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..21))); // 20 rows
        let diff = rec.observe(frame(hashes(100..120))).unwrap(); // all different
        assert_eq!(diff.matched_rows, 0);
        let report = rec.report();
        assert_eq!(report.mass_reflow_events, 1);
    }

    #[test]
    fn duplicate_hashes_do_not_vote_or_count_as_displaced() {
        // Blank-like duplicate rows (same hash) must not produce false
        // reposition events.
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(vec![7, 7, 1, 2, 7, 7]));
        let diff = rec.observe(frame(vec![7, 7, 1, 2, 7, 7])).unwrap();
        assert_eq!(diff.displaced_rows, 0);
        assert_eq!(diff.dominant_shift, 0);
    }

    #[test]
    fn blank_rows_are_ignored_entirely() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(vec![BLANK_ROW_HASH, 1, BLANK_ROW_HASH, 2]));
        let diff = rec
            .observe(frame(vec![1, BLANK_ROW_HASH, 2, BLANK_ROW_HASH]))
            .unwrap();
        // Both rows shifted up by one (blank movement does not matter).
        assert_eq!(diff.dominant_shift, -1);
        assert_eq!(diff.displaced_rows, 0);
    }

    #[test]
    fn report_counts_appeared_stats() {
        let mut rec = AnchorStabilityRecorder::new();
        rec.observe(frame(hashes(1..5)));
        let mut rows = hashes(1..5);
        rows.extend([100, 101, 102]);
        rec.observe(frame(rows.clone()));
        rows.extend([103]);
        rec.observe(frame(rows));
        let report = rec.report();
        assert_eq!(report.appeared_rows_max, 3);
        assert!(report.appeared_rows_mean > 0.0);
    }
}
