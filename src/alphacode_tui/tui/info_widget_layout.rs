use super::info_widget::{
    InfoWidgetData, Side, WidgetKind, WidgetPlacement, calculate_widget_height,
    is_overview_mergeable,
};
use ratatui::layout::Rect;
use std::cmp::Reverse;
use std::collections::HashSet;

/// Minimum width needed to show a widget.
const MIN_WIDGET_WIDTH: u16 = 35;
/// Maximum width a widget can take.
const MAX_WIDGET_WIDTH: u16 = 65;
/// Minimum height needed to show a widget.
const MIN_WIDGET_HEIGHT: u16 = 5;
/// Blank rows kept between two widgets stacked in the same pocket.
const WIDGET_GAP: u16 = 1;
/// Frames a widget may stay hidden-in-place before its anchor is abandoned and
/// Phase 2 may re-home it. Residents are bound to transcript lines, so hiding
/// only happens while the content beneath re-renders (streaming, collapse,
/// markdown refresh). This is a backstop for content that permanently grew
/// over the slot.
const MAX_HIDDEN_FRAMES: u16 = 120;
/// Frames a wider slot must be continuously available before a previously
/// shrunken widget grows back toward its preferred width. Shrinking is
/// immediate; growing is delayed so the left edge never jitters.
const GROW_BACK_FRAMES: u16 = 45;

/// Persistent memory of where a widget lives. Widgets are *residents*: bound to
/// a transcript line, they ride with their content while scrolling and while
/// new lines append at the bottom.
#[derive(Debug, Clone)]
pub(crate) struct WidgetAnchor {
    pub placement: WidgetPlacement,
    /// Consecutive frames this anchor has been retained but not rendered.
    pub hidden_frames: u16,
    /// Absolute transcript line the widget's top row is pinned to.
    pub content_top: usize,
    /// Widest width this widget has had since it was placed; the target for
    /// grow-back after a transient squeeze.
    pub preferred_width: u16,
    /// Consecutive frames a wider-than-current slot has been available.
    pub wider_frames: u16,
}

impl WidgetAnchor {
    fn fresh(placement: WidgetPlacement, content_top: usize) -> Self {
        let preferred_width = placement.rect.width;
        Self {
            placement,
            hidden_frames: 0,
            content_top,
            preferred_width,
            wider_frames: 0,
        }
    }
}

/// Result of a placement pass: what to render now, plus the anchor memory to
/// feed back in next frame.
pub(crate) struct PlacementOutcome {
    pub visible: Vec<WidgetPlacement>,
    pub anchors: Vec<WidgetAnchor>,
}

impl PlacementOutcome {
    fn empty() -> Self {
        Self {
            visible: Vec::new(),
            anchors: Vec::new(),
        }
    }
}

/// Margin information for layout calculation.
#[derive(Debug, Clone, Default)]
pub struct Margins {
    /// Free widths on the right side for each row.
    pub right_widths: Vec<u16>,
    /// Free widths on the left side for each row (only populated in centered mode).
    pub left_widths: Vec<u16>,
    /// Whether we're in centered mode.
    pub centered: bool,
    /// Look-ahead "reliable" free widths: per row, the width that stays free
    /// across a band of upcoming/recent scroll lines. Gates where *new* widgets
    /// may dock (Phase 2). Pinned widgets (Phase 1) size to the instantaneous
    /// widths. Empty = fall back to the instantaneous widths.
    pub right_reliable: Vec<u16>,
    pub left_reliable: Vec<u16>,
    /// Absolute transcript line shown on the first visible row this frame.
    pub scroll_top: usize,
}

impl Margins {
    fn right_dock_widths(&self) -> &[u16] {
        if self.right_reliable.is_empty() {
            &self.right_widths
        } else {
            &self.right_reliable
        }
    }

    fn left_dock_widths(&self) -> &[u16] {
        if self.left_reliable.is_empty() {
            &self.left_widths
        } else {
            &self.left_reliable
        }
    }

    fn instant_widths(&self, side: Side) -> &[u16] {
        match side {
            Side::Right => &self.right_widths,
            Side::Left => &self.left_widths,
        }
    }
}

// ---------------------------------------------------------------------------
// Margin spaces and pockets
// ---------------------------------------------------------------------------

/// Available margin space on one side.
#[derive(Debug, Clone)]
struct MarginSpace {
    side: Side,
    /// Free width per row (index = row from top of the messages area),
    /// already truncated to the area height.
    widths: Vec<u16>,
    /// X offset where this margin starts (right edge for Right, left for Left).
    x_offset: u16,
}

impl MarginSpace {
    /// Phase 2 docks on the *reliable* widths so a new widget lands only where
    /// space stays free for a few scroll lines. If that profile admits no
    /// placeable region at all, fall back to the instantaneous widths so we
    /// still show something. Both are truncated to the viewport first, so a
    /// stale/oversized profile can never let a widget escape the messages area.
    fn new(side: Side, reliable: &[u16], instant: &[u16], area: Rect) -> Self {
        let h = area.height as usize;
        let reliable = &reliable[..reliable.len().min(h)];
        let instant = &instant[..instant.len().min(h)];
        let widths = if !reliable.is_empty() && !find_candidate_rows(reliable).is_empty() {
            reliable.to_vec()
        } else {
            instant.to_vec()
        };
        let x_offset = match side {
            Side::Right => area.x.saturating_add(area.width),
            Side::Left => area.x,
        };
        Self {
            side,
            widths,
            x_offset,
        }
    }
}

/// A free rectangle in a margin, candidate for docking a new widget.
#[derive(Debug, Clone, Copy)]
struct Pocket {
    side: Side,
    top: u16,
    height: u16,
    width: u16,
    x: u16,
    margin_idx: usize,
}

impl Pocket {
    /// Build a pocket over rows `[top, end)`, deriving width and x from the
    /// actual margin profile. Returns `None` if the rows can't host a widget.
    fn fit(
        spaces: &[MarginSpace],
        margin_idx: usize,
        top: usize,
        end: usize,
        area_width: u16,
    ) -> Option<Self> {
        let space = spaces.get(margin_idx)?;
        let end = end.min(space.widths.len());
        if top >= end || end - top < MIN_WIDGET_HEIGHT as usize {
            return None;
        }
        let width = space.widths[top..end]
            .iter()
            .copied()
            .min()?
            .min(MAX_WIDGET_WIDTH)
            .min(area_width);
        if width < MIN_WIDGET_WIDTH {
            return None;
        }
        let x = match space.side {
            Side::Right => space.x_offset.saturating_sub(width),
            Side::Left => space.x_offset,
        };
        Some(Self {
            side: space.side,
            top: top as u16,
            height: (end - top) as u16,
            width,
            x,
            margin_idx,
        })
    }
}

fn push_unique(pockets: &mut Vec<Pocket>, p: Pocket) {
    let dup = pockets.iter().any(|q| {
        q.side == p.side && q.margin_idx == p.margin_idx && q.top == p.top && q.height == p.height
    });
    if !dup {
        pockets.push(p);
    }
}

fn build_pockets(spaces: &[MarginSpace], area_width: u16) -> Vec<Pocket> {
    let mut pockets = Vec::new();
    for (idx, space) in spaces.iter().enumerate() {
        for (top, len) in find_candidate_rows(&space.widths) {
            if let Some(p) = Pocket::fit(spaces, idx, top, top + len, area_width) {
                push_unique(&mut pockets, p);
            }
        }
    }
    pockets
}

/// Carve rows `[start, end)` on `side` out of every pocket that overlaps them.
/// Unlike a simple trim, a reservation in the middle of a pocket keeps *both*
/// the part above and the part below, each re-fitted to its true free width.
fn reserve_rows(
    pockets: &mut Vec<Pocket>,
    spaces: &[MarginSpace],
    area_width: u16,
    side: Side,
    start: usize,
    end: usize,
) {
    let mut next = Vec::with_capacity(pockets.len() + 1);
    for p in pockets.drain(..) {
        let p_start = p.top as usize;
        let p_end = p_start + p.height as usize;
        if p.side != side || start >= p_end || end <= p_start {
            push_unique(&mut next, p);
            continue;
        }
        if p_start < start {
            if let Some(piece) = Pocket::fit(spaces, p.margin_idx, p_start, start, area_width) {
                push_unique(&mut next, piece);
            }
        }
        if end < p_end {
            if let Some(piece) = Pocket::fit(spaces, p.margin_idx, end, p_end, area_width) {
                push_unique(&mut next, piece);
            }
        }
    }
    *pockets = next;
}

/// Find candidate placement row ranges `(top, len)` in a free-width profile.
///
/// For every distinct width level, emits each maximal run of rows at least that
/// wide (and at least `MIN_WIDGET_HEIGHT` tall). A ragged region therefore
/// offers both its full-height narrow pocket and any wider, shorter sub-pocket,
/// instead of collapsing everything to the single narrowest row.
fn find_candidate_rows(free_widths: &[u16]) -> Vec<(usize, usize)> {
    let capped: Vec<u16> = free_widths
        .iter()
        .map(|&w| w.min(MAX_WIDGET_WIDTH))
        .collect();
    let mut levels: Vec<u16> = capped
        .iter()
        .copied()
        .filter(|&w| w >= MIN_WIDGET_WIDTH)
        .collect();
    levels.sort_unstable_by(|a, b| b.cmp(a));
    levels.dedup();

    let mut out: Vec<(usize, usize)> = Vec::new();
    for level in levels {
        let mut start: Option<usize> = None;
        // `len` is the sentinel index that closes a trailing run.
        for i in 0..=capped.len() {
            let ok = capped.get(i).is_some_and(|&w| w >= level);
            match (ok, start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    start = None;
                    let len = i - s;
                    if len >= MIN_WIDGET_HEIGHT as usize && !out.contains(&(s, len)) {
                        out.push((s, len));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Width policy for a held widget. Shrinks immediately to clear wide content;
/// grows back toward `preferred` only after a wider slot has been available for
/// `GROW_BACK_FRAMES` consecutive frames. Returns `(width, wider_frames)`.
fn resolve_width(current: u16, preferred: u16, fit: u16, wider_frames: u16) -> (u16, u16) {
    if fit < current {
        (fit, 0)
    } else if fit > current && current < preferred {
        let frames = wider_frames.saturating_add(1);
        if frames >= GROW_BACK_FRAMES {
            (fit.min(preferred), 0)
        } else {
            (current, frames)
        }
    } else {
        (current, 0)
    }
}

// ---------------------------------------------------------------------------
// Placement engine
// ---------------------------------------------------------------------------

struct Placer<'a> {
    area: Rect,
    margins: &'a Margins,
    spaces: Vec<MarginSpace>,
    pockets: Vec<Pocket>,
    placements: Vec<WidgetPlacement>,
    anchors: Vec<WidgetAnchor>,
    /// Widgets with a live anchor (visible or hidden-in-place); Phase 2 must
    /// not re-home these.
    anchored: HashSet<WidgetKind>,
    /// Overview is shown *or* only transiently hidden: its mergeable widgets
    /// (model, context, ...) stay suppressed so nothing pops in elsewhere.
    overview_active: bool,
}

impl<'a> Placer<'a> {
    fn new(area: Rect, margins: &'a Margins) -> Self {
        let mut spaces = Vec::new();
        if !margins.right_widths.is_empty() {
            spaces.push(MarginSpace::new(
                Side::Right,
                margins.right_dock_widths(),
                &margins.right_widths,
                area,
            ));
        }
        if margins.centered && !margins.left_widths.is_empty() {
            spaces.push(MarginSpace::new(
                Side::Left,
                margins.left_dock_widths(),
                &margins.left_widths,
                area,
            ));
        }
        let pockets = build_pockets(&spaces, area.width);
        Self {
            area,
            margins,
            spaces,
            pockets,
            placements: Vec::new(),
            anchors: Vec::new(),
            anchored: HashSet::new(),
            overview_active: false,
        }
    }

    fn reserve(&mut self, side: Side, start: usize, end: usize) {
        reserve_rows(
            &mut self.pockets,
            &self.spaces,
            self.area.width,
            side,
            start,
            end,
        );
    }

    /// Phase 1: hold an anchored widget on the transcript line it is bound to.
    ///
    /// Because the covered rows always map back to the same content lines, the
    /// free-width profile under a resident is stable frame to frame. If its
    /// line scrolled above the viewport (or below it after a resize/scroll-up)
    /// the resident retires and Phase 2 may house a fresh instance.
    fn hold(&mut self, available: &[WidgetKind], anchor: &WidgetAnchor) {
        let prev = &anchor.placement;
        let kind = prev.kind;
        if !available.contains(&kind) || prev.rect.height <= 2 {
            return;
        }
        if self.anchored.contains(&kind)
            || (self.overview_active && is_overview_mergeable(kind))
        {
            return;
        }

        let Some(row_start) = anchor.content_top.checked_sub(self.margins.scroll_top) else {
            return;
        };
        let row_end = row_start + prev.rect.height as usize;
        let widths = self.margins.instant_widths(prev.side);
        if row_end > widths.len() || row_end > self.area.height as usize {
            return;
        }
        let target_y = self.area.y.saturating_add(row_start as u16);

        let fit_width = widths[row_start..row_end]
            .iter()
            .copied()
            .min()
            .unwrap_or(0)
            .min(MAX_WIDGET_WIDTH)
            .min(self.area.width);

        if fit_width < MIN_WIDGET_WIDTH {
            // A wide line is under the slot: hide in place so the widget returns
            // to the same spot, unless it has been hidden too long.
            let hidden_frames = anchor.hidden_frames.saturating_add(1);
            if hidden_frames <= MAX_HIDDEN_FRAMES {
                self.anchored.insert(kind);
                self.anchors.push(WidgetAnchor {
                    placement: prev.clone(),
                    hidden_frames,
                    content_top: anchor.content_top,
                    preferred_width: anchor.preferred_width,
                    wider_frames: 0,
                });
                if kind == WidgetKind::Overview {
                    self.overview_active = true;
                }
                // Keep Phase 2 out of the slot the widget will reclaim.
                self.reserve(prev.side, row_start, row_end);
            }
            return;
        }

        let (width, wider_frames) = resolve_width(
            prev.rect.width,
            anchor.preferred_width,
            fit_width,
            anchor.wider_frames,
        );
        let x = match prev.side {
            Side::Right => self
                .area
                .x
                .saturating_add(self.area.width)
                .saturating_sub(width),
            Side::Left => self.area.x,
        };
        let placement = WidgetPlacement {
            kind,
            rect: Rect::new(x, target_y, width, prev.rect.height),
            side: prev.side,
        };
        self.placements.push(placement.clone());
        self.anchors.push(WidgetAnchor {
            placement,
            hidden_frames: 0,
            content_top: anchor.content_top,
            preferred_width: anchor.preferred_width.max(width),
            wider_frames,
        });
        self.anchored.insert(kind);
        if kind == WidgetKind::Overview {
            self.overview_active = true;
        }
        self.reserve(prev.side, row_start, row_end);
    }

    /// Phase 2: place a widget that has no live anchor into the best pocket.
    fn place_new(&mut self, data: &InfoWidgetData, kind: WidgetKind) {
        if self.anchored.contains(&kind) || (self.overview_active && is_overview_mergeable(kind)) {
            return;
        }

        let min_h = kind.min_height() + 2;
        let preferred = kind.preferred_side();

        // Score: preferred side first; then the most content shown; then the
        // tightest fit (keeps big pockets free for later widgets); then width.
        type Score = (bool, u16, Reverse<u16>, u16);
        let mut best: Option<(usize, u16, Score)> = None;

        for (idx, p) in self.pockets.iter().enumerate() {
            if p.height < min_h || p.width < MIN_WIDGET_WIDTH {
                continue;
            }
            // A pocket's nominal minimum can still be too small for the actual
            // content; skip it rather than pick it and abandon the widget.
            let h = calculate_widget_height(kind, data, p.width, p.height).min(p.height);
            if h <= 2 {
                continue;
            }
            let score: Score = (p.side == preferred, h, Reverse(p.height - h), p.width);
            if best.as_ref().is_none_or(|(_, _, s)| score > *s) {
                best = Some((idx, h, score));
            }
        }

        let Some((idx, widget_height, _)) = best else {
            return;
        };
        let pocket = self.pockets[idx];

        // Seat at the *bottom* of the pocket for maximum runway to ride upward
        // with the transcript before scrolling off the top.
        let placed_top = pocket.top + (pocket.height - widget_height);
        let placement = WidgetPlacement {
            kind,
            rect: Rect::new(
                pocket.x,
                self.area.y.saturating_add(placed_top),
                pocket.width,
                widget_height,
            ),
            side: pocket.side,
        };
        self.placements.push(placement.clone());
        // Bind the widget to the transcript line under its top row.
        self.anchors.push(WidgetAnchor::fresh(
            placement,
            self.margins.scroll_top + placed_top as usize,
        ));
        self.anchored.insert(kind);

        // Claim the rows (plus a breathing gap above) in every overlapping pocket.
        let start = placed_top.saturating_sub(WIDGET_GAP) as usize;
        let end = placed_top as usize + widget_height as usize;
        self.reserve(pocket.side, start, end);

        if kind == WidgetKind::Overview {
            self.overview_active = true;
            // Overview supersedes any small widgets that survived Phase 1,
            // otherwise the same info would be shown twice.
            self.placements.retain(|p| !is_overview_mergeable(p.kind));
            self.anchors
                .retain(|a| !is_overview_mergeable(a.placement.kind));
            self.anchored.retain(|k| !is_overview_mergeable(*k));
        }
    }

    fn finish(self) -> PlacementOutcome {
        PlacementOutcome {
            visible: self.placements,
            anchors: self.anchors,
        }
    }
}

/// Anchor-aware placement. Widgets behave like residents of the transcript:
/// each holds the slot bound to its content line, shrinks immediately when a
/// wide line scrolls under it, grows back with hysteresis, hides in place when
/// the slot is unusable, and only re-homes via Phase 2 after being unusable for
/// [`MAX_HIDDEN_FRAMES`] or after scrolling out of view.
pub(crate) fn calculate_placements_anchored(
    messages_area: Rect,
    margins: &Margins,
    data: &InfoWidgetData,
    enabled: bool,
    prev_anchors: &[WidgetAnchor],
) -> PlacementOutcome {
    if !enabled || messages_area.height == 0 || messages_area.width == 0 {
        return PlacementOutcome::empty();
    }
    let available = data.available_widgets();
    if available.is_empty() {
        return PlacementOutcome::empty();
    }
    let available: Vec<WidgetKind> = available.iter().copied().collect();

    let mut placer = Placer::new(messages_area, margins);

    // Phase 1: resolve Overview first so its mergeable widgets are known to be
    // suppressed before they are considered.
    let mut ordered: Vec<&WidgetAnchor> = prev_anchors.iter().collect();
    ordered.sort_by_key(|a| a.placement.kind != WidgetKind::Overview);
    for anchor in ordered {
        placer.hold(&available, anchor);
    }

    // Phase 2: new widgets, Overview first so smaller widgets are not placed
    // and then immediately retracted.
    let mut order = available;
    order.sort_by_key(|k| *k != WidgetKind::Overview);
    for kind in order {
        placer.place_new(data, kind);
    }

    placer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn right_space(widths: Vec<u16>) -> MarginSpace {
        MarginSpace {
            side: Side::Right,
            widths,
            x_offset: 100,
        }
    }

    #[test]
    fn candidate_rows_single_run() {
        let free = [0, 0, 40, 40, 40, 40, 40, 0];
        assert_eq!(find_candidate_rows(&free), vec![(2, 5)]);
    }

    #[test]
    fn candidate_rows_ragged_offers_wide_and_tall() {
        let free = [60, 60, 60, 60, 60, 36, 36, 36, 36, 36];
        let rows = find_candidate_rows(&free);
        assert!(rows.contains(&(0, 5)), "wide sub-pocket missing: {rows:?}");
        assert!(rows.contains(&(0, 10)), "full pocket missing: {rows:?}");
    }

    #[test]
    fn candidate_rows_too_short_or_narrow() {
        assert!(find_candidate_rows(&[40, 40, 40]).is_empty());
        assert!(find_candidate_rows(&[10; 20]).is_empty());
        assert!(find_candidate_rows(&[]).is_empty());
    }

    #[test]
    fn reserve_in_middle_keeps_both_halves() {
        let spaces = vec![right_space(vec![50; 20])];
        let mut pockets = vec![Pocket::fit(&spaces, 0, 0, 20, 100).unwrap()];
        reserve_rows(&mut pockets, &spaces, 100, Side::Right, 8, 12);
        let mut spans: Vec<(u16, u16)> = pockets.iter().map(|p| (p.top, p.height)).collect();
        spans.sort();
        assert_eq!(spans, vec![(0, 8), (12, 8)]);
    }

    #[test]
    fn reserve_ignores_other_side_and_drops_small_remnants() {
        let spaces = vec![right_space(vec![50; 8])];
        let mut pockets = vec![Pocket::fit(&spaces, 0, 0, 8, 100).unwrap()];
        reserve_rows(&mut pockets, &spaces, 100, Side::Left, 0, 8);
        assert_eq!(pockets.len(), 1);
        reserve_rows(&mut pockets, &spaces, 100, Side::Right, 2, 8);
        assert!(pockets.is_empty(), "2-row remnant should be dropped");
    }

    #[test]
    fn width_shrinks_immediately_and_grows_back_slowly() {
        assert_eq!(resolve_width(60, 60, 40, 7), (40, 0));
        let mut w = (40, 0);
        for _ in 0..GROW_BACK_FRAMES - 1 {
            w = resolve_width(w.0, 60, 55, w.1);
            assert_eq!(w.0, 40);
        }
        w = resolve_width(w.0, 60, 55, w.1);
        assert_eq!(w, (55, 0));
    }

    #[test]
    fn width_never_exceeds_preferred_and_resets_streak() {
        assert_eq!(resolve_width(60, 60, 65, 10), (60, 0));
        assert_eq!(resolve_width(40, 50, 40, 20), (40, 0));
    }
}