# Interface follow-up — 2026-10-30

The existing plans are inventories, not evidence that their targets have been
met. The most consequential open interface work identified in the current
documents is:

- Capture and review TUI layouts at 40, 80, 120 and 200 columns, including
  keyboard navigation, resize and streaming (improvement backlog D1–D5).
- Consolidate theme tokens and check narrow terminals and streaming behavior
  (UI/UX modernization checklist). Verify color and non-color focus cues.
- Measure frame times on large transcripts and during resizing (backlog B4).
- Verify shared branding, ASCII fallback and machine-readable CLI output across
  TUI and console surfaces (backlog E).
- Complete the full test and lint gates and document platform limitations
  (backlog G). The previous TUI audit explicitly did not establish a green full
  suite or interactive coverage across terminal emulators.
- Desktop saved chats are currently read-only, and desktop availability is
  Windows-only (`docs/gui.md`). These remain product limitations, not finished
  cross-platform or session-resumption features.

This pass fixes desktop search updates, selection of filtered results, project
list retention on unchanged refreshes, and the search shortcut with a hidden
sidebar. The sidebar now explains empty search results while the current chat
remains visible. Keyboard focus and accessible names were improved for desktop
navigation. The GUI smoke test covers filtering and refresh with a local fake
bridge; it does not verify a real server or screen reader.

Shared TUI chrome now clips drawing to the frame during resize. Local client
event reads use cancellation-safe bounded framing, with a 64 MiB limit on a
single event; unusually large history responses may need pagination instead.
These corrections do not close the broader work above or establish a speedup.
