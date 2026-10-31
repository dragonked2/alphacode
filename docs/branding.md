# Terminal branding

`src/alphacode_brand.rs` is the single source of truth for the product name,
the brand mark and the wordmark tiers. Every surface reads from it:

| Surface | Entry point |
|---|---|
| CLI `run` banner, interactive REPL | `alphacode_core::output_style::startup_banner` |
| TUI splash / welcome screen | `SplashScreen::wordmark_art` |
| TUI persistent header | `build_alpha_banner` |
| Window title, tab bars, menubar | `process_title::terminal_window_title` |
| Widget titles | `alphacode_brand::widget_title` / `section_title` |

No surface may keep a private copy of the art. That is not a style preference:
three hand-maintained copies had already drifted, and one of them was rendering
`╔═╗╔═╗╔╦╗` garbage because its rows were 33, 32 and 41 cells wide and spelled
nothing.

## Wordmark tiers

`wordmark_for(width)` returns the widest tier whose every row fits, measured in
display cells (not bytes or `char`s, so double-width glyphs cannot shear the
art). Every surface that can host the logo — the startup header, the splash and
the CLI banner — uses this one selector.

`compact_wordmark_for(width)` applies the same rule but only over the single-row
text tiers. It is for chrome with a hard one-row budget (a section rule, a picker
title, a status strip), **not** for the header. Routing the header here collapsed
the six-row wordmark into `A L P H A C O D E` on every terminal wide enough to
show the art, which looked like the banner had vanished.

| Tier | Rows | Cells | Used by |
|---|---|---|---|
| `WORDMARK_BLOCK` | 6 | 73 | startup header, splash, CLI banner, REPL |
| `WORDMARK_COMPACT` | 5 | 63 | startup header, splash, CLI banner, REPL |
| `WORDMARK_MONOSPACE` (`A L P H A C O D E`) | 1 | 17 | startup header, splash |
| `WORDMARK_MINIMAL` (`‹ AlphaCode ›`) | 1 | 13 | startup header, splash |

Below 13 columns both selectors return `None` and the caller degrades to a plain
text label. They never return a tier that would have to wrap.

Wordmark rows are **plain text — never embedded ANSI/SGR escapes**. Colour is
applied by the renderer (`BrandTheme::gradient_rows` for the TUI surfaces,
`styled_startup_banner` for the CLI), because two independent things break when
art carries its own escapes:

- the tier selector measures rows with `unicode-width`, which counts escape
  bytes as printable cells, so the art appears far wider than it renders and is
  almost never selected (and is centred from a bogus width when it is);
- ratatui renders `Span` text as literal cells, so the escapes are painted as
  glyphs instead of being interpreted.

`alphacode_brand::tests::wordmark_art_carries_no_ansi_escapes` pins this.

The splash gates each accessory row on *its own* measured width
(`full_tagline_width`, `compact_tagline_width`, `elite_feature_row_width`)
rather than a round number, so no accessory row can be wider than the terminal
that displays it. Below the short tagline's width the splash drops the tagline
entirely and shows only the wordmark.

## Unicode and colour fallbacks

When Unicode is unavailable, the CLI banner uses spaced ASCII lettering
(`A L P H A C O D E`), then the plain product name, then nothing. Colour is
always optional: the art, the alignment and the centering are computed from
display width alone, so they work identically with `NO_COLOR` set or output
piped. JSON, NDJSON, quiet mode and redirected output never receive a banner.

The wordmark gradient comes from `BrandTheme::gradient_rows`, which produces a
continuous ramp across the brand hues. Sampling the fixed 16-stop gradient array
per row instead would give a six-row logo only three distinct colours and draw
hard horizontal bands; the header and the splash both use the ramp so the same
art reads the same way on both.

## What the tests pin

- Every tier's rows share one display width; the ladder is ordered widest-first.
- `wordmark_for` and `compact_wordmark_for` never return a tier wider than the
  requested width, swept over every width from 0 to 200.
- The persistent selector never returns multi-row art, and never selects a tier
  wider than the splash tier for the same width.
- `SplashScreen::banner_with_features` emits no row wider than the terminal, for
  every width from 0 to 200, and emits nothing below the minimal wordmark.
- The CLI banner centers its rows and stays within the terminal width across the
  same sweep, in both Unicode and ASCII mode.
- No wordmark constant is a raw string containing `\u`, and no wordmark spells
  the product name in the lowercase slug.
- Window titles name the product exactly once, in branded case
  (`AlphaCode · Fox`, never `AlphaCode · alphacode Fox`).
