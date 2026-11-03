# Interface style refresh — 2026-11-03

## Shared visual language

The built-in terminal palette now shares the desktop slate-and-mint design:
mint for actions and focus, blue for information and links, neutral body text,
and separate success, warning, and error colors. CLI output includes equivalent
light-background colors. Existing custom palettes and presets remain available.
Brand gradients stay between the accent and information colors. Prompt recency
fades through emphasis rather than assigning a different hue to each message;
tool activity subtly pulses within the current theme.

## Terminal surfaces

- Composer focus has an accent border, with separate busy and inactive tones.
- Help uses a quick-start row, section headings, command names and descriptions
  on separate rows, and quieter category colors.
- Onboarding reserves less space for animation and measures the wrapped body
  before allocating decorative space. Small terminals use a compact layout
  with a persistent help or escape hint.
- Login import columns use display-cell widths for wide and combining text.
- Telemetry copy links to privacy controls without assuming the active setting.

## Desktop surfaces

- Labeled navigation and selected-edge markers.
- Searchable command palette with keyboard navigation, backdrop dismissal,
  focus restoration, and actions connected to existing controls.
- Focus mode restores the previous panel layout and preserves saved preferences.
- Wrapping toolbar, compact composer gutters, and bounded conversation width.
- Descriptive quick-start cards, themed tooltips, grouped work cards, role markers,
  code typography, settings navigation descriptions, and keyboard keycaps.

## Verification scope

An earlier GUI smoke run passed and its desktop and minimum-window captures were
visually reviewed before the broader redesign. An intermediate workspace check
passed. These results do not validate the final changes. The subsequent Cargo
verification run was stopped when the user requested no further checks or tests.
Final redesign changes have not been compiled or tested. No release or installer
build was produced for this refresh.
