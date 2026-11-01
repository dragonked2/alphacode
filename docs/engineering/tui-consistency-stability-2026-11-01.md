# TUI consistency and stability follow-up — 2026-11-01

This pass preserves the existing in-flight composer, console, session and provider work.

Changes made in this pass:

- Both header wordmark renderers use the active brand accent, replacing independent three-shade gold arrays. Search hits, saved-session labels, and swarm tree markers also use the palette accent; current search hits retain stronger emphasis through their blend strength.
- Session-label truncation slices at Unicode grapheme boundaries without collecting the full text into a temporary grapheme vector or boxing an iterator. It preserves prefix/suffix behavior, wide characters, combining marks, and joined emoji.
- Preview wheel momentum stops when a rendered preview fits without scrolling. Previously a zero maximum was mistaken for an unknown maximum, repeatedly accepting scroll steps that the next render discarded.
- Long preview offsets saturate at the existing u16 scroll limit instead of wrapping. This does not remove the 65,535-row offset limit.
- Onboarding prompt geometry measures full cell widths and accumulates wrapped rows with saturating usize arithmetic before clamping to the viewport.
- Corrected existing build blockers: missing AgentsConfig plan_max_nodes default, an unclosed communication-input test, test access to private fields/methods, and unused import/variable warnings. The communication-input test now lives in its owning module.

Regression coverage includes theme-colored wordmarks, joined emoji and narrow truncation, and preview scroll boundaries. Verification uses the external temporary Cargo target and VS 2022 MSVC environment required by AGENTS.md. Live providers and terminal-emulator font/shaping behavior are outside this pass; no general model-accuracy or measured runtime-speed improvement is claimed.

## Live work presentation

The follow-up request adds a work checkpoint before assistant messages following tool work. It summarizes completed command runs, reads, successful edits, other tool calls, and failed actions. Batch calls are matched to their individual result sections, so a failed edit cannot inflate successful file totals. Existing detailed tool rows and diffs remain available.

The status/footer row also shows current-turn edit totals while streaming or running tools (subject to available width), and after completion. The next user prompt resets these totals. Unique paths count once; line totals count edit operations, not net Git changes. Overwriting a file without a returned diff does not invent removed-line totals. These counters cover structured tools with available metadata; edits performed inside arbitrary shell commands are not inferred. Totals are cached by session, display version and message count so decorative redraws do not recalculate diffs.

The system prompt now asks for a short initial next-step message, concise findings and next checks between meaningful tool blocks, periodic updates during sustained work when control returns from tools, and an evidence-based final result. Update wording and cadence depend on the serving model; a blocking tool cannot emit model commentary while it owns control.

Additional tests cover batch partial failures, deduplicated edit paths, omitted unknown overwrite statistics, checkpoint boundaries and narrow widths, complete transcript rendering, and resetting current-turn totals on a new prompt.

The full verification run also exposed and corrected two existing regression-fixture problems (the hidden-tool test now uses its own live registry; the Deep-mode cap fixture counts the automatic root gate). Invalid priority-label errors now identify the priority field. The growth guard itself now includes a gate only in modes that insert one, allowing a Light-mode expansion that reaches the configured cap exactly; arithmetic saturates instead of overflowing. An executable regression covers that boundary.

Focused renderer verification passed 595 tests with 1 ignored. An initial 256-color run exposed two pre-existing color-capability assumptions: a shimmer test expected distinct RGB shades after quantization, and the animation reference comparison could observe another test pinning truecolor between its two render paths. Both tests now pin their required color capability before sampling.

## Final verification

Formatting, both workspace checks, strict library Clippy, and whitespace checks passed. The final complete library suite passed 6,401 tests with 0 failures and 11 ignored (338.63 seconds). First-party test compilation used the temporary process override CARGO_PROFILE_TEST_OPT_LEVEL=0, with the required VS 2022 MSVC environment and external Cargo target. Source build profiles were preserved. The effort follow-up and its lifecycle/regression details are recorded in effort-controls-audit-2026-11-01.md.
