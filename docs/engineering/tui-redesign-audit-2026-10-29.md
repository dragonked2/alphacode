# TUI redesign audit — 2026-10-29

This records the surfaces inspected and the changes made in this pass. It is not a claim that every terminal, theme, or application workflow has been exhaustively tested.

- Conversation: made the persistent wordmark compact and kept it stable during streaming; replaced the heavy separator with a lighter rule. Reviewed the main frame, composer and status paths; model labels retain their actual identity.
- Session picker: added responsive single-pane navigation for narrow terminals, explicit empty/search feedback, theme-based focus and status colors, grapheme-safe truncation, a readable crash banner, and safe handling of short windows. Search no longer clears an unsuccessful query on Enter. Standalone terminal cleanup runs after setup, input, drawing errors, and panics.
- Login and accounts: stacked list and details on narrow screens; removed conflicting letter shortcuts so provider searches accept ordinary text. Arrows, Page Up/Down, Home/End and Escape remain available. Backspace removes a full grapheme and Enter does not dismiss an empty result set.
- Help and settings: wrapped help descriptions to the actual width and aligned scroll limits with wrapped rows; made end-of-help navigation reversible. Settings use a visible focus marker and shared border styling.
- Search, selection and feedback: kept the transcript-search counter visible beside long queries, used grapheme boundaries for selection, picker labels and toast previews, and bounded model-picker highlights in very narrow columns.
- Onboarding and themes: aligned onboarding accents and session-preview colors with theme roles, and invalidated cached previews when the palette changes.

Regression tests were added for grapheme boundaries, picker search and navigation, and terminal-size rendering. Full workspace checks and interactive checks across terminal emulators are separate verification steps; record their actual results in the task report.


## Verification for this pass

- `cargo check --workspace` and `cargo check --workspace --tests` passed with the required MSVC environment and external target directory.
- Focused standalone tests passed for terminal setup, display-width selection, query highlights, picker truncation, and search/toast text fitting. Two full library test builds were stopped when concurrent builds brought available memory close to exhaustion; the full suite remains unverified.
- `cargo fmt --all -- --check` reports unrelated formatting differences in other in-flight files. The edited TUI files pass their individual formatting checks.
- `cargo clippy --lib -- -D warnings` reports an existing `needless_return` in `src/alphacode_build_support/paths.rs`; that shared in-flight file was not changed for this audit.

