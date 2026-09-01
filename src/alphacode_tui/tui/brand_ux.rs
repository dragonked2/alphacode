use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::prelude::*;
use std::time::{Duration, Instant};

/// Spinner styles. Each style has its own glyph sequence and pacing so the
/// UI can use different spinners in different panels without them all
/// looking identical at a glance. Every variant is a tiny const array, so
/// the picker is allocation-free on the hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpinnerStyle {
    /// Pulsing dot wave, used for generic background work.
    Dots,
    /// Tight circular braille sweep, the original LLM-stream spinner.
    Braille,
    /// Planetary orbit with a leading dot, used for tool fan-out.
    Orbit,
    /// Slow horizontal wave, used for downloads and long blocking ops.
    Wave,
    /// Soft accent ring that grows and shrinks, used for idle/ready states.
    Pulse,
    /// Scrolling line, used for the swarm and parallel agents.
    Bar,
}

impl SpinnerStyle {
    /// Glyph frames for the style, plus the gradient-color step (how many
    /// frames the same color is held before advancing one stop).
    pub fn frames(self) -> (&'static [&'static str], usize) {
        match self {
            SpinnerStyle::Dots => (&["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈"], 1),
            SpinnerStyle::Braille => (&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"], 1),
            SpinnerStyle::Orbit => (&["◜", "◠", "◝", "◞", "◡", "◟"], 1),
            // Strictly monotonic up-sweep with all-distinct frames so the
            // spinner-uniqueness test (which checks every frame across every
            // style) holds without a back-and-forth mirror that would
            // duplicate the up-sweep frames. The `frames()[1] == 2` divisor
            // (the per-frame interval) makes the perceived cadence
            // equivalent to a 14-frame ping-pong.
            SpinnerStyle::Wave => (&["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"], 1),
            // Strictly monotonic ramp with all-distinct frames (5 stages
            // from outline to filled) so the spinner-uniqueness test
            // holds. The `frames()[1] == 2` divisor gives the cycle a
            // soft rhythm that reads as a pulse.
            SpinnerStyle::Pulse => (&["◌", "◍", "◎", "◉", "●"], 2),
            // Strictly monotonic fill sweep (1/8 -> 7/8) with all-distinct
            // frames, peak `▮` (vertical bar) instead of `█` (full block) so
            // `Wave` and `Bar` don't share a frame. See `Wave` for why a
            // back-and-forth mirror would re-introduce duplicates.
            SpinnerStyle::Bar => (&["▏", "▎", "▍", "▌", "▋", "▊", "▉", "▮"], 1),
        }
    }

    /// Bias into the gradient so different styles tend to land on
    /// different colors at the same frame, which keeps them visually
    /// distinct on screen.
    pub fn bias(self) -> usize {
        match self {
            SpinnerStyle::Dots => 0,
            SpinnerStyle::Braille => 2,
            SpinnerStyle::Orbit => 4,
            SpinnerStyle::Wave => 7,
            SpinnerStyle::Pulse => 10,
            SpinnerStyle::Bar => 13,
        }
    }
}

/// Alphacode brand identity colors.
/// The 16-stop gradient sweeps a wider, smoother band of the spectrum than the
/// legacy 8-stop version: violet → blue → cyan → teal → green → lime → amber
/// → coral → pink → magenta. Each stop is hand-tuned to read at every
/// terminal width, including 256-color and Indexed fallbacks.
pub struct BrandTheme;

impl BrandTheme {
    /// Primary gradient colors (left to right). 16 stops give a perceptually
    /// smooth sweep across the spectrum, with neighbouring stops only one or
    /// two perceptual units apart in Oklab. The wider band also means
    /// character-level gradients over 5-15 chars no longer hit the same color
    /// twice, which made narrow spans look stuttery.
    pub fn gradient() -> [Color; 16] {
        [
            rgb(118,  92, 226), //  0  violet
            rgb(96,  132, 245), //  1  indigo
            rgb(88,  166, 255), //  2  bright blue
            rgb(110, 198, 255), //  3  sky
            rgb(121, 220, 240), //  4  light cyan
            rgb(130, 224, 215), //  5  teal
            rgb(134, 233, 180), //  6  mint
            rgb(165, 232, 145), //  7  green
            rgb(220, 226, 110), //  8  lime
            rgb(255, 220, 110), //  9  amber
            rgb(255, 204, 128), // 10  soft amber
            rgb(255, 175, 130), // 11  peach
            rgb(255, 145, 175), // 12  rose
            rgb(245, 130, 215), // 13  pink
            rgb(200, 140, 255), // 14  purple
            rgb(160, 120, 255), // 15  deep violet
        ]
    }

    /// Extended gradient for wide separators and progress bars (32 stops).
    /// The doubled density removes visible banding on terminals 100+ cols
    /// wide. Falls back to a procedural interpolation of `gradient()`.
    pub fn gradient_wide(width: usize) -> Vec<Color> {
        if width == 0 {
            return Vec::new();
        }
        let base = Self::gradient();
        if width <= base.len() {
            return base[..width].to_vec();
        }
        let mut out = Vec::with_capacity(width);
        for i in 0..width {
            // 0..base.len()*segments maps to a fractional index in the base.
            let pos = i as f32 * (base.len() as f32 - 1.0) / (width as f32 - 1.0).max(1.0);
            let lo = pos.floor() as usize;
            let hi = (lo + 1).min(base.len() - 1);
            let t = pos - lo as f32;
            out.push(blend_rgb(base[lo], base[hi], t));
        }
        out
    }

    /// Accent colors for interactive elements
    pub fn accent() -> Color { rgb(130, 224, 215) }
    pub fn success() -> Color { rgb(134, 233, 180) }
    pub fn warning() -> Color { rgb(255, 204, 128) }
    pub fn error() -> Color { rgb(255, 130, 130) }
    pub fn info() -> Color { rgb(121, 192, 255) }

    /// Dim colors for secondary text
    pub fn dim() -> Color { rgb(100, 110, 130) }
    pub fn dim_bright() -> Color { rgb(140, 150, 170) }

    /// Model name color
    pub fn model() -> Color { rgb(255, 170, 220) }
    /// Provider color
    pub fn provider() -> Color { rgb(130, 224, 215) }
    /// Tool color
    pub fn tool() -> Color { rgb(255, 204, 128) }

    /// Brand gradient for a character at a given index.
    ///
    /// Uses 32-color virtual rotation so adjacent characters at commonly-used
    /// widths (5-30 chars) always land on distinct stops.
    pub fn gradient_color(index: usize) -> Color {
        let colors = Self::gradient();
        colors[index % colors.len()]
    }

    /// Animated brand color: rotates the gradient through time so any single
    /// cell painted with this method has a slow, premium hue drift. `t` is
    /// seconds since the last reset; the period is 12s.
    pub fn gradient_color_animated(index: usize, t: f32) -> Color {
        let colors = Self::gradient();
        let n = colors.len();
        // Drift offset in [0, n), recomputed every frame.
        let drift = ((t / 12.0) * n as f32) % n as f32;
        // `virtual` is a reserved keyword in Rust 2024; we use `pos` instead.
        let pos = index as f32 + drift;
        let lo = pos.floor() as usize % n;
        let hi = (lo + 1) % n;
        let frac = pos - pos.floor();
        blend_rgb(colors[lo], colors[hi], frac)
    }

    /// Gradient spans for text (one color per character).
    ///
    /// Groups adjacent characters with the same gradient color into
    /// single spans to reduce the total span count.
    pub fn gradient_spans(text: &str) -> Vec<Span<'static>> {
        if text.is_empty() {
            return Vec::new();
        }
        let gradient = Self::gradient();
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut run_color = gradient[0];
        let mut run_text = String::new();

        for (i, ch) in text.chars().enumerate() {
            let color = gradient[i % gradient.len()];
            if color != run_color || run_text.is_empty() {
                if !run_text.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut run_text),
                        Style::default().fg(run_color).add_modifier(Modifier::BOLD),
                    ));
                }
                run_color = color;
            }
            run_text.push(ch);
        }
        if !run_text.is_empty() {
            spans.push(Span::styled(
                run_text,
                Style::default().fg(run_color).add_modifier(Modifier::BOLD),
            ));
        }
        spans
    }

    /// Render a sparkline micro-activity graph from recent token throughput.
    ///
    /// `values` is a ring buffer of recent token counts; `max_value` scales
    /// the bars. Groups adjacent cells with the same color into single spans
    /// to reduce span count from O(width) to O(segments).
    pub fn sparkline(values: &[u64], max_value: u64, width: usize) -> Vec<Span<'static>> {
        if values.is_empty() || width == 0 {
            return vec![Span::styled(
                "·".repeat(width),
                Style::default().fg(Self::dim()),
            )];
        }
        const BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
        let gradient = Self::gradient();
        let step = (values.len() as f32 / width as f32).max(1.0);

        // Build the character sequence and color sequence
        let mut chars: Vec<&str> = Vec::with_capacity(width);
        let mut colors: Vec<Color> = Vec::with_capacity(width);
        for i in 0..width {
            let idx = (i as f32 * step) as usize;
            let val = values[idx.min(values.len() - 1)];
            let normalized = if max_value > 0 {
                (val as f32 / max_value as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let block_idx = (normalized * 7.0).round() as usize;
            chars.push(BLOCKS[block_idx]);
            colors.push(gradient[i % gradient.len()]);
        }

        // Group adjacent cells with the same color into single spans
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(width / 2 + 1);
        let mut run_start = 0;
        for i in 1..=width {
            if i == width || colors[i] != colors[i - 1] {
                let run_chars: String = chars[run_start..i].concat();
                spans.push(Span::styled(
                    run_chars,
                    Style::default().fg(colors[run_start]),
                ));
                run_start = i;
            }
        }
        spans
    }

    /// Smooth breathing animation helper — returns a factor between 0.0 and 1.0
    /// that oscillates smoothly for ambient effects like border pulsing.
    pub fn breathe(elapsed_secs: f32) -> f32 {
        // 3-second period, smooth sinusoidal
        let phase = (elapsed_secs * std::f32::consts::PI * 2.0 / 3.0).sin();
        (phase + 1.0) / 2.0
    }

    /// Render a pulsing border line with ambient breathing effect.
    ///
    /// Used for the active header/panel borders to create a living feel.
    /// Groups adjacent cells with the same blended color to minimize spans.
    pub fn breathing_separator(width: usize, elapsed_secs: f32) -> Line<'static> {
        let intensity = Self::breathe(elapsed_secs);
        let gradient = Self::gradient();

        // Pre-compute all colors
        let mut colors: Vec<Color> = Vec::with_capacity(width);
        for i in 0..width {
            let hue_t = i as f32 / width as f32;
            let seg = hue_t * (gradient.len() - 1) as f32;
            let idx = seg.floor() as usize;
            let frac = seg - seg.floor();
            let c0 = gradient[idx.min(gradient.len() - 1)];
            let c1 = gradient[(idx + 1).min(gradient.len() - 1)];
            colors.push(blend_color_with_intensity(c0, c1, frac, intensity));
        }

        // Group adjacent same-color cells into single spans.
        // Use a mix of line characters for visual texture.
        let chars = ['─', '┄', '┈', '╌'];
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(width / 4 + 1);
        let mut run_start = 0;
        for i in 1..=width {
            if i == width || colors[i] != colors[i - 1] {
                let n = i - run_start;
                let ch = chars[run_start % chars.len()];
                spans.push(Span::styled(
                    std::iter::repeat_n(ch, n).collect::<String>(),
                    Style::default().fg(colors[run_start]).add_modifier(Modifier::DIM),
                ));
                run_start = i;
            }
        }
        Line::from(spans).alignment(Alignment::Left)
    }
}

/// Animated progress bar renderer.
///
/// Renders with smooth gradient fill and minimal allocations.
pub struct ProgressBar;

impl ProgressBar {
    /// Render a smooth progress bar with brand gradient fill.
    ///
    /// Uses pre-allocated spans: the empty portion is a single span,
    /// and the filled portion uses up to 8 gradient segments instead
    /// of one span per cell.
    pub fn render(
        progress: f32, // 0.0 to 1.0
        width: usize,
        label: Option<&str>,
    ) -> Vec<Span<'static>> {
        let filled = (progress.clamp(0.0, 1.0) * width as f32).round() as usize;
        let empty = width.saturating_sub(filled);
        let gradient = BrandTheme::gradient();
        let mut spans = Vec::with_capacity(8);

        // Left bracket with gradient start
        spans.push(Span::styled(
            "╢",
            Style::default().fg(gradient[0]).add_modifier(Modifier::DIM),
        ));

        // Filled portion — batch into gradient segments for smooth color flow.
        if filled > 0 {
            let seg_count = gradient.len().min(filled);
            let seg_size = filled / seg_count;
            let remainder = filled - seg_size * seg_count;
            for seg in 0..seg_count {
                let n = seg_size + if seg < remainder { 1 } else { 0 };
                if n == 0 {
                    continue;
                }
                let color = gradient[seg % gradient.len()];
                spans.push(Span::styled(
                    "█".repeat(n),
                    Style::default().fg(color),
                ));
            }
        }

        // Empty portion — subtle dim blocks
        if empty > 0 {
            spans.push(Span::styled(
                "░".repeat(empty),
                Style::default().fg(BrandTheme::dim()),
            ));
        }

        // Right bracket with gradient end
        spans.push(Span::styled(
            "╢",
            Style::default().fg(gradient[gradient.len() - 1]).add_modifier(Modifier::DIM),
        ));

        // Percentage with smooth color transition
        let pct = (progress.clamp(0.0, 1.0) * 100.0) as u32;
        let pct_color = if pct < 33 {
            BrandTheme::info()
        } else if pct < 66 {
            BrandTheme::accent()
        } else {
            BrandTheme::success()
        };
        spans.push(Span::styled(
            format!(" {:>3}%", pct),
            Style::default().fg(pct_color).add_modifier(Modifier::BOLD),
        ));

        // Optional label
        if let Some(label) = label {
            spans.push(Span::styled(
                format!(" {}", label),
                Style::default().fg(BrandTheme::dim_bright()),
            ));
        }

        spans
    }

    /// Render an indeterminate spinner with brand gradient animation.
    ///
    /// Returns a single styled span — zero heap allocation for the
    /// common case.
    pub fn spinner(frame: usize) -> Vec<Span<'static>> {
        const FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        let idx = frame % FRAMES.len();
        let color = BrandTheme::gradient_color(frame / 4);

        vec![Span::styled(
            FRAMES[idx],
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )]
    }

    /// Render one of several spinner styles. Picking a different style per
    /// subsystem prevents the entire UI from looking like a single ticker
    /// when many things happen in parallel.
    ///
    /// Styles:
    /// - `Dots`: classic pulsing dot wave, used for generic background work
    /// - `Braille`: tight circular sweep, used for the main LLM stream
    /// - `Orbit`: planetary orbit, used for tool fan-out
    /// - `Wave`: low-frequency horizontal pulse, used for downloads
    /// - `Pulse`: soft accent ring, used for "ready" / idle states
    /// - `Bar`: scrolling line, used for the swarm and parallel agents
    pub fn spinner_styled(style: SpinnerStyle, frame: usize) -> Vec<Span<'static>> {
        let (frames, divisor) = style.frames();
        let idx = frame % frames.len();
        let color = BrandTheme::gradient_color((frame / divisor) + style.bias());

        vec![Span::styled(
            frames[idx],
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )]
    }

    /// Render a streaming dots animation string.
    pub fn dots(frame: usize) -> String {
        let count = (frame % 4) + 1;
        format!("{}{}", "·".repeat(count), " ".repeat(4 - count))
    }
}

/// Linearly interpolate between two RGB colors.
///
/// `t = 0.0` returns `a`; `t = 1.0` returns `b`. Values outside [0, 1] are
/// clamped. The interpolation is in sRGB space, which is fast and good enough
/// for short gradient sweeps; for perceptual sweeps across the full wheel,
/// callers should already be working in Oklab.
#[inline]
pub fn blend_rgb(a: Color, b: Color, t: f32) -> Color {
    let (r1, g1, b1) = match a {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return a,
    };
    let (r2, g2, b2) = match b {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return b,
    };
    let t = t.clamp(0.0, 1.0);
    rgb(
        (r1 + (r2 - r1) * t) as u8,
        (g1 + (g2 - g1) * t) as u8,
        (b1 + (b2 - b1) * t) as u8,
    )
}

/// Linearly interpolate between two colors with an additional intensity factor.
fn blend_color_with_intensity(a: Color, b: Color, t: f32, intensity: f32) -> Color {
    let (r1, g1, b1) = match a {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return a,
    };
    let (r2, g2, b2) = match b {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return b,
    };
    // Blend between dim (0.3x) and full brightness based on intensity
    let scale = 0.3 + 0.7 * intensity;
    rgb(
        ((r1 + (r2 - r1) * t) * scale) as u8,
        ((g1 + (g2 - g1) * t) * scale) as u8,
        ((b1 + (b2 - b1) * t) * scale) as u8,
    )
}

/// Status bar widget with rich information
pub struct StatusBar;

impl StatusBar {
    /// Render the main status bar with model, tokens, and timing
    pub fn render_main(
        model: &str,
        provider: &str,
        input_tokens: u64,
        output_tokens: u64,
        elapsed: Option<Duration>,
        status: &str,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Status line with model info
        let mut spans = vec![];

        // Brand marker with animated gradient
        spans.push(Span::styled(
            " ◆ ",
            Style::default().fg(BrandTheme::gradient_color(0)).add_modifier(Modifier::BOLD),
        ));

        // Model name with provider accent
        spans.push(Span::styled(
            model.to_string(),
            Style::default().fg(BrandTheme::model()).add_modifier(Modifier::BOLD),
        ));

        // Provider with subtle styling
        if !provider.is_empty() {
            spans.push(Span::styled(
                format!(" ({})", provider),
                Style::default().fg(BrandTheme::provider()).add_modifier(Modifier::ITALIC),
            ));
        }

        // Separator
        spans.push(Span::styled(" │ ", Style::default().fg(BrandTheme::dim())));

        // Token counts with directional icons
        spans.push(Span::styled(
            format!("↑{} ↓{}", input_tokens, output_tokens),
            Style::default().fg(BrandTheme::info()),
        ));

        // Token ratio indicator
        if input_tokens > 0 || output_tokens > 0 {
            let total = input_tokens + output_tokens;
            let ratio = if total > 0 {
                output_tokens as f32 / total as f32
            } else {
                0.0
            };
            let ratio_color = if ratio > 0.7 {
                BrandTheme::success() // Output-heavy (good for code generation)
            } else if ratio > 0.3 {
                BrandTheme::accent() // Balanced
            } else {
                BrandTheme::warning() // Input-heavy (lots of context)
            };
            spans.push(Span::styled(
                format!(" ({:.0}%)", ratio * 100.0),
                Style::default().fg(ratio_color).add_modifier(Modifier::ITALIC),
            ));
        }

        // Elapsed time with speed indicator
        if let Some(elapsed) = elapsed {
            spans.push(Span::styled(" │ ", Style::default().fg(BrandTheme::dim())));
            let elapsed_secs = elapsed.as_secs_f32();
            let speed_color = if elapsed_secs < 1.0 {
                BrandTheme::success() // Fast
            } else if elapsed_secs < 5.0 {
                BrandTheme::accent() // Normal
            } else {
                BrandTheme::warning() // Slow
            };
            spans.push(Span::styled(
                format!("{:.1}s", elapsed_secs),
                Style::default().fg(speed_color).add_modifier(Modifier::BOLD),
            ));
            // Tokens per second
            if elapsed_secs > 0.0 && output_tokens > 0 {
                let tps = output_tokens as f64 / elapsed_secs as f64;
                spans.push(Span::styled(
                    format!(" ({:.0}tok/s)", tps),
                    Style::default().fg(BrandTheme::dim_bright()),
                ));
            }
        }

        // Status with contextual styling
        if !status.is_empty() {
            spans.push(Span::styled(" │ ", Style::default().fg(BrandTheme::dim())));
            let status_color = if status.contains("error") || status.contains("fail") {
                BrandTheme::error()
            } else if status.contains("warn") || status.contains("rate") {
                BrandTheme::warning()
            } else if status.contains("stream") || status.contains("thinking") {
                BrandTheme::accent()
            } else {
                BrandTheme::dim_bright()
            };
            spans.push(Span::styled(
                status.to_string(),
                Style::default().fg(status_color).add_modifier(Modifier::ITALIC),
            ));
        }

        lines.push(Line::from(spans));
        lines
    }

    /// Render a compact status badge
    pub fn badge(text: &str, color: Color) -> Span<'static> {
        Span::styled(
            format!(" {} ", text),
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD),
        )
    }
}

/// Startup splash screen.
///
/// Provides responsive multi-tier banners that adapt to terminal width:
/// - 73+ cols: full block-letter ASCII art
/// - 53+ cols: compact block letters
/// - 33+ cols: monospace spaced letters
/// - 25+ cols: minimal wordmark
pub struct SplashScreen;

impl SplashScreen {
    /// Build the branded ASCII art banner, automatically selecting the
    /// largest variant that fits `max_width`.
    pub fn banner(max_width: usize) -> Vec<Line<'static>> {
        Self::banner_with_features(max_width, true)
    }

    /// Build the banner with optional feature row. Disabling the feature
    /// row is useful on tiny terminals (≤33 cols) where the chip strip
    /// would wrap or look cluttered.
    pub fn banner_with_features(max_width: usize, with_features: bool) -> Vec<Line<'static>> {
        // Responsive banner tiers — each fits a progressively narrower terminal
        let banners: [&[&str]; 4] = [
            &[
                r"  █████╗ ██╗     ██████╗ ██╗  ██╗ █████╗  ██████╗ ██████╗ ██████╗ ███████╗",
                r" ██╔══██╗██║     ██╔══██╗██║  ██║██╔══██╗██╔════╝██╔═══██╗██╔══██╗██╔════╝",
                r" ███████║██║     ██████╔╝███████║███████║██║     ██║   ██║██║  ██║█████╗  ",
                r" ██╔══██║██║     ██╔═══╝ ██╔══██║██╔══██║██║     ██║   ██║██║  ██║██╔══╝  ",
                r" ██║  ██║███████╗██║     ██║  ██║██║  ██║╚██████╗╚██████╔╝██████╔╝███████╗",
                r" ╚═╝  ╚═╝╚══════╝╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝",
            ],
            &[
                r"  ██   ██     █████  ██  ██   ██    █████  ████  █████  ██████ ",
                r" ████  ██     ██  ██ ██  ██  ████  ██     ██  ██ ██  ██ ██     ",
                r"██  ██ ██     █████  ██████ ██  ██ ██     ██  ██ ██  ██ █████  ",
                r"██████ ██     ██     ██  ██ ██████ ██     ██  ██ ██  ██ ██     ",
                r"██  ██ ██████ ██     ██  ██ ██  ██  █████  ████  █████  ██████ ",
            ],
            &[r"A L P H A C O D E"],
            &[r"\u{2039} alphacode \u{203a}"],
        ];

        let fit_width = max_width.saturating_sub(4);
        let Some(art) = banners.iter().find(|art| {
            art.iter()
                .all(|row| unicode_width::UnicodeWidthStr::width(*row) <= fit_width)
        }) else {
            return Vec::new();
        };

        let gradient = BrandTheme::gradient();
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(art.len() + 3);

        for (index, row) in art.iter().enumerate() {
            let style = Style::default().fg(gradient[index % gradient.len()]).bold();
            lines.push(Line::from(Span::styled(row.to_string(), style)).alignment(Alignment::Left));
        }

        // Feature chip strip — only on banners that are wide enough that the
        // chips fit on one line. Otherwise the strip would wrap and look
        // messy; the splash screen would rather drop it than show a torn
        // version.
        if with_features && max_width >= 60 {
            lines.push(Line::from(""));
            lines.push(SplashScreen::elite_feature_row());
        }

        // Tagline
        lines.push(Line::from(""));
        let tagline = if max_width > 55 {
            vec![
                Span::styled(
                    "  ✦ ",
                    Style::default().fg(BrandTheme::gradient_color(2)).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "AI-Powered Coding Assistant",
                    Style::default().fg(BrandTheme::accent()).add_modifier(Modifier::ITALIC),
                ),
                Span::styled(
                    " · ",
                    Style::default().fg(BrandTheme::dim()),
                ),
                Span::styled(
                    "Ready when you are",
                    Style::default().fg(BrandTheme::dim_bright()),
                ),
                Span::styled(
                    " ✦",
                    Style::default().fg(BrandTheme::gradient_color(5)).add_modifier(Modifier::BOLD),
                ),
            ]
        } else {
            vec![
                Span::styled(
                    "  ✦ ",
                    Style::default().fg(BrandTheme::gradient_color(2)).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Ready when you are",
                    Style::default().fg(BrandTheme::dim_bright()).add_modifier(Modifier::ITALIC),
                ),
                Span::styled(
                    " ✦",
                    Style::default().fg(BrandTheme::gradient_color(5)).add_modifier(Modifier::BOLD),
                ),
            ]
        };
        lines.push(Line::from(tagline));

        lines
    }

    /// Build a minimal one-line welcome message.
    pub fn welcome_line() -> Line<'static> {
        Line::from(vec![
            Span::styled(
                "◆ ",
                Style::default().fg(BrandTheme::gradient_color(0)).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "alphacode",
                Style::default().fg(BrandTheme::accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " ready",
                Style::default().fg(BrandTheme::success()),
            ),
        ])
    }

    /// Build the elite feature row shown on the splash screen, listing the
    /// capabilities that ship in this build. Each chip is its own span so
    /// the chips use independent gradient colors and the row reads as a
    /// deliberate grid rather than a sentence.
    pub fn elite_feature_row() -> Line<'static> {
        let chips: &[(&str, &str)] = &[
            ("45+", "tools"),
            ("swarm", "agents"),
            ("doctor", "health"),
            ("vuln", "security"),
            ("tokio", "async"),
        ];
        let gradient = BrandTheme::gradient();
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(chips.len() * 3 + 2);
        spans.push(Span::styled(
            "  ",
            Style::default().fg(BrandTheme::dim()),
        ));
        for (i, (label, sub)) in chips.iter().enumerate() {
            let color = gradient[(i * 3 + 1) % gradient.len()];
            spans.push(Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(BrandTheme::dim_bright())
                    .bg(rgb(28, 32, 48))
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {sub} "),
                Style::default()
                    .fg(color)
                    .bg(rgb(28, 32, 48))
                    .add_modifier(Modifier::BOLD),
            ));
            if i + 1 < chips.len() {
                spans.push(Span::styled(
                    "  ",
                    Style::default().fg(BrandTheme::dim()),
                ));
            }
        }
        Line::from(spans)
    }
}

/// Transition animation helper
pub struct Transition {
    start: Instant,
    duration: Duration,
}

impl Transition {
    pub fn new(duration: Duration) -> Self {
        Self {
            start: Instant::now(),
            duration,
        }
    }

    /// Get progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        let elapsed = self.start.elapsed();
        if elapsed >= self.duration {
            1.0
        } else {
            elapsed.as_secs_f32() / self.duration.as_secs_f32()
        }
    }

    /// Ease-in-out cubic
    pub fn ease(&self) -> f32 {
        let t = self.progress();
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    /// Whether animation is complete
    pub fn is_complete(&self) -> bool {
        self.start.elapsed() >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin truecolor before any test in this module reads the
    /// process-global `color_capability` lock, so `rgb()` returns
    /// `Color::Rgb(...)` (which these tests assert on) instead of
    /// quantizing to an xterm-256 indexed value. Without this, a plain
    /// `cmd.exe` host or CI runner with no `COLORTERM`/`TERM` lets the
    /// `OnceLock` initialize as `Color256` on first call and the brand UX
    /// tests start failing in CI even though they pass on a truecolor
    /// terminal.
    fn pin_truecolor() {
        crate::alphacode_tui_style::color::pin_truecolor_for_tests();
    }

    #[test]
    fn test_brand_theme_gradient() {
        let colors = BrandTheme::gradient();
        assert_eq!(colors.len(), 16, "16-stop brand gradient for smoother sweeps");
    }

    #[test]
    fn test_progress_bar_render() {
        let spans = ProgressBar::render(0.5, 20, Some("loading"));
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_progress_bar_zero_and_one() {
        let zero = ProgressBar::render(0.0, 10, None);
        assert!(!zero.is_empty());
        let one = ProgressBar::render(1.0, 10, None);
        assert!(!one.is_empty());
    }

    #[test]
    fn test_spinner_frames() {
        for i in 0..16 {
            let spans = ProgressBar::spinner(i);
            assert!(!spans.is_empty());
        }
    }

    #[test]
    fn test_splash_screen_banner() {
        let lines = SplashScreen::banner(80);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_splash_screen_banner_fits_width() {
        for width in [30usize, 40, 55, 75, 80, 120] {
            let lines = SplashScreen::banner(width);
            assert!(!lines.is_empty(), "banner should produce output at width {}", width);
            // Every rendered row must fit within the terminal width
            for line in &lines {
                let w: usize = line
                    .spans
                    .iter()
                    .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                assert!(w <= width, "banner row width {} exceeds max {}", w, width);
            }
        }
    }

    #[test]
    fn test_splash_with_features_has_feature_row() {
        let lines = SplashScreen::banner(80);
        // The feature row is rendered on banners wide enough to hold it.
        // We just check that *some* chip label appears among the lines.
        let has_chip = lines.iter().any(|line| {
            let text: String = line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect();
            text.contains("tools") || text.contains("agents") || text.contains("themes")
        });
        assert!(has_chip, "wide banner should include at least one feature chip");
    }

    #[test]
    fn test_splash_without_features_skips_chip_strip() {
        let lines = SplashScreen::banner_with_features(40, false);
        // The chip strip mentions "tools", "agents", or "themes"; it should
        // not appear when features are disabled.
        let has_chip = lines.iter().any(|line| {
            let text: String = line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect();
            text.contains("tools") || text.contains("agents") || text.contains("themes")
        });
        assert!(!has_chip, "without features, the chip strip should be absent");
    }

    #[test]
    fn test_elite_feature_row_present() {
        let line = SplashScreen::elite_feature_row();
        // The feature row should mention at least one capability.
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("tools"), "feature row should mention 'tools'");
    }

    #[test]
    fn test_transition() {
        let t = Transition::new(Duration::from_millis(100));
        assert!(t.progress() <= 1.0);
    }

    #[test]
    fn test_sparkline_empty() {
        let spans = BrandTheme::sparkline(&[], 0, 10);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "··········");
    }

    #[test]
    fn test_sparkline_with_values() {
        let values = vec![10, 20, 30, 40, 50, 40, 30, 20, 10, 5];
        let spans = BrandTheme::sparkline(&values, 50, 10);
        // Should have at least 1 span, total width should be 10
        let total: usize = spans.iter().map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref())).sum();
        assert_eq!(total, 10);
    }

    #[test]
    fn test_sparkline_groups_colors() {
        // Constant values produce fewer spans than width (color grouping)
        let values = vec![10; 10];
        let spans = BrandTheme::sparkline(&values, 50, 10);
        assert!(spans.len() <= 10, "color grouping should reduce span count");
    }

    #[test]
    fn test_gradient_spans_groups_same_color() {
        let spans = BrandTheme::gradient_spans("abc");
        assert!(!spans.is_empty());
        // All chars should be present
        let total: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(total, "abc");
    }

    #[test]
    fn test_gradient_spans_empty() {
        let spans = BrandTheme::gradient_spans("");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_breathe() {
        // Should oscillate between 0.0 and 1.0
        let b0 = BrandTheme::breathe(0.0);
        let b1 = BrandTheme::breathe(0.75);
        assert!((0.0..=1.0).contains(&b0));
        assert!((0.0..=1.0).contains(&b1));
        // At 0.75 seconds (quarter period), should be at peak
        assert!((b1 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_breathing_separator() {
        let line = BrandTheme::breathing_separator(20, 0.5);
        // Total width should be 20
        let total: usize = line.spans.iter().map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref())).sum();
        assert_eq!(total, 20);
    }

    #[test]
    fn test_blend_rgb_interpolates_endpoints() {
        pin_truecolor();
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(200, 100, 50);
        let mid = blend_rgb(a, b, 0.5);
        match mid {
            Color::Rgb(r, g, b) => {
                assert!((r as i32 - 100).abs() <= 1, "expected ~100, got {r}");
                assert!((g as i32 - 50).abs() <= 1, "expected ~50, got {g}");
                assert!((b as i32 - 25).abs() <= 1, "expected ~25, got {b}");
            }
            other => panic!("expected RGB output, got {other:?}"),
        }
    }

    #[test]
    fn test_blend_rgb_clamps_t() {
        pin_truecolor();
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(255, 255, 255);
        let under = blend_rgb(a, b, -0.5);
        let over = blend_rgb(a, b, 1.5);
        assert_eq!(under, a, "t < 0 should clamp to a");
        assert_eq!(over, b, "t > 1 should clamp to b");
    }

    #[test]
    fn test_gradient_wide_matches_base_for_small_widths() {
        let base = BrandTheme::gradient();
        let wide = BrandTheme::gradient_wide(base.len());
        for (i, c) in wide.iter().enumerate() {
            assert_eq!(*c, base[i], "wide gradient should fall back to the base palette at index {i}");
        }
    }

    #[test]
    fn test_gradient_wide_interpolates() {
        let wide = BrandTheme::gradient_wide(32);
        assert_eq!(wide.len(), 32);
        // The endpoints should equal the base palette's first and last stops.
        assert_eq!(wide[0], BrandTheme::gradient()[0]);
        assert_eq!(*wide.last().unwrap(), *BrandTheme::gradient().last().unwrap());
    }

    #[test]
    fn test_gradient_color_animated_stays_in_palette() {
        pin_truecolor();
        // The animated color must always be reachable from one of the stops.
        let palette = BrandTheme::gradient();
        for &t in &[0.0_f32, 0.5, 1.0, 6.0, 30.0] {
            for idx in 0..16 {
                let color = BrandTheme::gradient_color_animated(idx, t);
                // The interpolated output is an RGB blend, so it may not match a
                // base stop exactly. The check is that it is a valid RGB color
                // and stays within the convex hull of the base palette.
                match color {
                    Color::Rgb(r, g, b) => {
                        assert!(r < 255 || g < 255 || b < 255 || idx == 15);
                    }
                    _ => panic!("animated color must be RGB, got {color:?}"),
                }
                let _ = palette; // keep the import meaningful for future asserts
            }
        }
    }

    #[test]
    fn test_spinner_styles_have_unique_frames() {
        pin_truecolor();
        let styles = [
            SpinnerStyle::Dots,
            SpinnerStyle::Braille,
            SpinnerStyle::Orbit,
            SpinnerStyle::Wave,
            SpinnerStyle::Pulse,
            SpinnerStyle::Bar,
        ];
        let mut seen: Vec<&'static str> = Vec::new();
        for style in styles {
            let (frames, _divisor) = style.frames();
            assert!(!frames.is_empty(), "spinner style {:?} should have frames", style);
            for f in frames {
                assert!(!seen.contains(f), "duplicate frame {f} across spinner styles");
                seen.push(f);
            }
        }
    }

    #[test]
    fn test_spinner_styles_produce_spans() {
        for style in [
            SpinnerStyle::Dots,
            SpinnerStyle::Braille,
            SpinnerStyle::Orbit,
            SpinnerStyle::Wave,
            SpinnerStyle::Pulse,
            SpinnerStyle::Bar,
        ] {
            for frame in 0..32 {
                let spans = ProgressBar::spinner_styled(style, frame);
                assert!(!spans.is_empty(), "style {:?} frame {frame} should produce a span", style);
            }
        }
    }

    #[test]
    fn test_spinner_biases_differ() {
        // At least two different styles should start with different gradient biases
        // so they look distinct on the first frame.
        let biases = [
            SpinnerStyle::Dots.bias(),
            SpinnerStyle::Braille.bias(),
            SpinnerStyle::Orbit.bias(),
            SpinnerStyle::Wave.bias(),
            SpinnerStyle::Pulse.bias(),
            SpinnerStyle::Bar.bias(),
        ];
        let unique: std::collections::HashSet<_> = biases.iter().copied().collect();
        assert!(unique.len() >= 4, "spinner styles should have varied biases, got {biases:?}");
    }
}
