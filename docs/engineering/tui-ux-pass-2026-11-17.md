# TUI/UX improvement pass — 2026-11-17

Scope: TUI rendering, input, layout, and release CI. Both changes were
verified on the exact failing paths; nothing here is a redesign.

## 1. Release blocker: 1.1.10 could never publish

`61fbfff4` ("Fix flaky session_watchdog test assertion") edited
`test_tui_cerebras_paste_key_lifecycle_has_no_degraded_success_messages`
and left an unbalanced `}` (a duplicate close after `else if ...`). That is a
parse error, so `cargo test --lib` could not compile on any platform:

```
error: unexpected closing delimiter: `}`
  --> src/alphacode_tui/tui/app/tests/state_model_poke_03.rs:949:1
```

Consequences observed on CI runs `44094511225` / `44094510987`:

- `ci` / `test (ubuntu-24.04)`, `test (macos-15-intel)`, `test
  (windows-latest)` all failed.
- `release` / `publish release` failed at "Require green CI for this source
  revision", so the v1.1.10 tag was left with build artifacts but a **draft**
  release and a failing run.

Fixed by removing the stray brace (commit `79a89d0e`).

The release workflow only runs on `v*.*.*` tag pushes, and its
`publish release` job blocks on "Require green CI for this source revision".
Because the tag pointed at the uncompilable commit, no amount of retrying
could ever publish it. The lightweight tag was therefore re-pointed from
`61fbfff4` to `cd05ad78`, a commit whose `ci` run is fully green; the release
run `44142105576` then completed with `publish release` succeeded.
No tag message was lost: `v1.1.10` was a lightweight tag.

## 2. Cross-test leak that made the Cerebras test flaky

`test_tui_login_providers_have_real_tui_handlers` iterates every TUI login
provider. Alphax Free skips the interactive prompt (bundled key), so
`start_login_provider` reaches `start_openai_compatible_post_login_activation`
and, when `provider.set_model` fails against the test double, publishes
`LoginCompleted { provider: "Alphax Free", success: false }` on the
**process-global** bus (`auth.rs`). The test did not hold
`Bus::test_bus_lock()`, so under `--test-threads=4` that event was delivered
to any concurrently running subscriber — in practice the Cerebras paste-key
test, which asserts "no late failed login event after successful auth".

Fixed by holding `Bus::test_bus_lock()` for that test body, taken after
`ensure_test_alphacode_home_if_unset()` so the established env-then-bus lock
order is preserved (the env helper uses a bounded try-lock, so neither order
can block forever).

## 3. `↑N` / `↓N` overflow affordance: byte length used as cell width

`draw_messages` computed the affordance geometry from `String::len()`:

```rust
let indicator = format!("↑{}", scroll);      // e.g. "↑14"
x:     render_area.width.saturating_sub(indicator.len() as u16 + 2),
width: indicator.len() as u16,
```

`↑14` is 5 UTF-8 bytes but 3 cells, so the glyphs were drawn two columns to
the left of where they were measured — and they were painted *after* the
transcript paragraph, so they overwrote content. Offscreen render evidence at
40 columns:

```
0| /model               Choose a mo↑14
```

The affordance now reserves its own cells before the transcript is painted:

- it is planned after the row margins are known but before
  `frame.render_widget(Paragraph::new(visible_lines), ...)`;
- if a host row has less free right margin than the affordance is wide, the
  row is trimmed with a dim ellipsis (`truncate_line_for_copy_badge`) to make
  exactly the room it needs, so content is never silently covered;
- geometry is measured with `UnicodeWidthStr::width`, and the rect is flush
  against `content_area.right()`.

## 4. Composer footer repeated the newline hint

`actions()` in `ui_composer_chrome.rs` listed both `Alt+Enter newline` and
`\+Enter newline`. Shift+Enter (kitty keyboard protocol), Alt/Option+Enter,
and a trailing backslash then Enter all reach
`newline::enter_inserts_newline`, so this spent footer width on one action
twice. On a 120-column terminal the row read
`... Ctrl+F find · \+Enter newline────╯`, with the repetition pushing the
rounded corner off the line.

A single `Alt+Enter newline` entry now covers the action, and
`composer_review_scroll_feedback_and_shortcuts_share_the_existing_border`
asserts the duplicate hint is absent. Rendered frames at 80/120/200 columns
show the footer ending at `Ctrl+F find` with the border closed.
## Verification

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo check --lib --tests` | pass |
| `cargo clippy --lib --all-targets -- -D warnings` | pass (CI's exact clippy invocation) |
| `cargo test --lib overflow_indicator_uses_cell_width_and_never_overwrites_transcript_text` | pass |
| `cargo test --lib -- --test-threads=4 test_tui_cerebras_paste_key_lifecycle test_tui_login_providers_have_real_tui_handlers` | 2 passed |
| CI `44128066266` (commit `c9e48156`) | all 13 jobs pass |
| CI `44133829088` (commit `cd05ad78`, tagged) | all 13 jobs pass |
| release `44142105576` (tag `v1.1.10`) | all 9 jobs pass, including `publish release` |

Release `v1.1.10` is published (`draft=false`) with 17 assets: eight platform
archives, their per-asset `.sha256` files, and the combined `SHA256SUMS`.

## Limitations

- The regression test drives the offscreen `TestBackend`; it does not
  exercise a live terminal, so font shaping and reader-to-flush latency remain
  unmeasured for this change.
- Only the non-native-scrollbar affordance path was corrected. The native
  scrollbar's thumb geometry was already cell-based and is unchanged.