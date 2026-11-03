# AlphaCode desktop GUI

On Windows, run `alphacode --gui` (or `alphacode gui`) to open the desktop workspace. The desktop connects to a local AlphaCode server and starts one if needed. The desktop host and its icon are embedded in the Windows executable at build time.

Enter sends a prompt; Shift+Enter adds a new line. Use **Hide sidebar** / **Show sidebar** or Ctrl+B to toggle the sidebar, and Ctrl+K to search chats. The sidebar preference is saved. The desktop uses standard Windows window controls. `alphacode gui --preview` opens the interface without connecting to a server, and `alphacode gui --once` prints a status snapshot.

The desktop connects through the local AlphaCode server. It supports its own chat, live-session prompts, model selection, saved chats, and workspace selection. The Terminal button opens the terminal interface for commands and workflows that the desktop does not yet support. Native saved sessions that are not live show their recent history read-only; resume them in the terminal to continue. Imported sessions still need the terminal for history and resumption. GUI availability is currently limited to Windows; the terminal interface remains available on all supported platforms.

Unsent drafts stay with their chat while the desktop is open. Switching chats restores the corresponding draft; successful delivery clears only the submitted draft. Drafts are held in memory. Command passwords are never stored as chat drafts. The Terminal button resumes the selected native chat when it has a valid session ID; otherwise it opens a terminal in the selected workspace.

Use **Jump to latest** after scrolling back through a response. Ctrl+End also returns to the latest response while the conversation has focus. Ctrl+N starts a new chat, Ctrl+L focuses the composer, Escape clears chat search, and Ctrl+Shift+C copies the response. Increase or decrease reading text with Ctrl+plus / Ctrl+minus, or reset with Ctrl+0; the size is saved and can also be changed from the More menu. The initial window fits the available desktop area and starts with a collapsed sidebar on smaller screens.

Windows builds require the .NET Framework C# compiler, Windows PowerShell, and the Windows resource compiler. Cross-compiled Windows builds do not include the desktop host and show an error if GUI launch is requested.

## Appearance and layout

The desktop uses a consistent slate and mint palette with dark and light themes. Switch themes using the half-circle button at the bottom of the navigation rail. The theme, window dimensions, reading size, sidebar preference, and window position are restored on the next launch. Initial layout no longer saves over preferences before they are read.

Chat actions sit below the title rather than competing with it for horizontal space. The composer gives workspace context its own row, keeps the Send action separate from model selection, wraps selectors when needed, and shows the prompt character limit. Blank prompts cannot be sent. Connection failures and command-input requests appear above the conversation, including when history is visible. Reading-size controls apply to normal and secure command inputs as well as replies and prompts.

Resizing below 1,000 logical pixels temporarily collapses the sidebar; expanding to at least 1,080 restores it unless you explicitly changed its visibility. Search and manual sidebar controls remain available in the compact layout. Model names receive more room on wide windows. Keyboard focus has a visible outline, and theme-aware status badges use text as well as color.

Run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/test-gui-smoke.ps1` for the isolated WPF interaction fixture. It uses a local fake bridge, validates restored settings, theme propagation, compact layout, sending, drafts, history, search, scrolling, model and effort controls, and emits dark-desktop and light-compact PNG renders under its temporary directory. It does not send prompts to a model or change your real GUI settings.

## Work panel

Use **Show work** beside the sidebar control to inspect the selected live chat's todos, plan, and swarm. On narrow layouts it uses the conversation area; the composer remains available and **Hide work** returns to the transcript. The panel uses read-only, selectable text and follows reading-size settings. It clears snapshots when switching chats and explains when work data is unavailable. Opening it requests session-scoped snapshots; closing it stops those requests. It does not start agents, edit plans, or mark tasks complete.

Core fixes accompanying the panel preserve explicitly blocked plan items, remove already-completed dependencies from the todo projection, include failed items in plan notices, and prevent blocked swarms from being reported as entirely finished. Todo writes now reject duplicate or blank IDs and invalid statuses; generated IDs for repeated or non-Latin titles are deterministic and unique. The task scheduler rejects a blank worker instead of leaving a running task without an owner.

## Conversation workspace

Responses now render as native selectable text, headings, lists, fenced code and plan cards. Inline bold and code are formatted, tool-call lists can be expanded, and code/plan cards have dedicated copy buttons. Response contents are treated as text, never evaluated as XAML or PowerShell. While streaming, unchanged sections retain their existing controls; only the changed tail is rebuilt.

Ctrl+F opens conversation search. Enter and Shift+Enter move between matching sections, with inline highlights and a visible match counter. Escape closes search. Ctrl+J toggles the work panel and Ctrl+M opens model selection. Drag the divider next to the work panel to resize it; its width and visibility are saved. On compact windows the work panel still uses the conversation area instead of leaving two cramped columns.

## Workspace selection

The + workspace control now uses the native Explorer-style Windows common item dialog directly, including on the bundled .NET Framework host. It opens at the selected workspace when that directory exists, uses the GUI window as its modal owner, accepts one existing filesystem folder, handles cancellation without starting a chat, and preserves Unicode paths. The legacy tree picker and optional Windows API Code Pack dependency have been removed. See Microsoft's [common file dialog documentation](https://learn.microsoft.com/windows/win32/shell/common-file-dialog).

## Sidebar and chat selection

The home icon toggles the sidebar without changing the active chat or navigation filter. Its visual state, tooltip, and accessibility name follow the sidebar state. All, Live, and Saved filters narrow the list; typing a search does not open a different chat or move your draft. Click a result or press Enter to open it; Down/Up moves into the results. Clear search restores the active row. Polling preserves the list scroll position and no longer reopens the companion chat during a refresh. Workspace shortcuts contain actual workspace folders, and chat rows distinguish working, attention, offline, and read-only history states.

## Settings form

The settings button opens a dedicated form with Appearance, Model & reasoning, Workspace, and Keyboard sections. Theme cards, a reading-size slider with a live preview, and workspace switches apply immediately and save automatically. Workspace selection uses the native folder picker. Model search displays available model identities; select a model and choose Use selected model to request the change. Model and reasoning controls reflect availability, pending requests, and read-only chats, with visible feedback. Escape or Done closes the form.

## Connected model chooser and response activity

The composer model button (Ctrl+M) opens a searchable chooser with keyboard navigation, an explicit Use model action, and a no-results state. Both it and Settings use the same desktop catalog: when route metadata is present, only models with at least one available provider route are offered. Route-less single-provider runtimes retain their own catalog, including local endpoints such as LM Studio. A missing current model is not added back, and an empty catalog cannot turn placeholder text into a model choice. The terminal catalog is unchanged.

Sending a prompt immediately shows a response activity strip above the composer. Animated dots, elapsed time, the selected chat's activity detail, and a Stop action remain visible while that chat is working, including while viewing earlier history. The indicator stops for ready, saved, offline, and input-request states; it respects Windows' client-area animation preference. Reading width is bounded on wide windows and the prompt grows with input up to a height limit, with a smaller footprint on compact windows.


## Copy the whole session

Run `/copy` in the composer to copy the current session as readable Markdown.
It uses the original retained messages and tool calls/results, including history
hidden by context compaction, collapsed tool output, or `/cls`. It does not copy
terminal borders, line wrapping, or just the visible viewport. Available live
text is labeled as an in-progress snapshot. Images are represented by attachment
labels; binary image data and opaque provider replay state are omitted.

Connected clients request full history from the attached server session and copy
on the client computer. Clipboard access runs in the background. The status row
reports preparation, success, an empty session, or failure. A session switch
invalidates a pending server response. Native clipboard access or terminal
clipboard forwarding is required; forwarding support and size limits depend on
the terminal. Client and server both need the `/copy` protocol support.


## Command palette and focus mode

Use **Commands** in the conversation toolbar or **Ctrl+Shift+P** to search
available desktop actions. Search accepts multiple words and ignores case.
Use the arrow keys to choose an action, Enter to run it, and Esc or the backdrop
to close. Unavailable actions are omitted. Closing the palette restores the
previous keyboard focus.

Use **Focus** or **Ctrl+Shift+F** to give the conversation more space. Focus mode
hides the chat sidebar and work panel; exiting restores their previous states.
The temporary layout does not replace your saved panel preferences.

Navigation buttons now include visible labels and selected-edge markers.
Toolbar actions wrap in narrow windows, while the composer adapts its gutters.
Quick-start cards explain what each action does. The work panel groups tasks,
plans, and agents into separate cards. Settings shows keyboard shortcuts as
aligned keycaps, with descriptions for each navigation section.
