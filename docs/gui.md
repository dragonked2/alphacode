# AlphaCode desktop GUI

On Windows, run `alphacode --gui` (or `alphacode gui`) to open the desktop workspace. The desktop connects to a local AlphaCode server and starts one if needed. The desktop host and its icon are embedded in the Windows executable at build time.

Enter sends a prompt; Shift+Enter adds a new line. Use **Hide sidebar** / **Show sidebar** or Ctrl+B to toggle the sidebar, and Ctrl+K to search chats. The sidebar preference is saved. The desktop uses standard Windows window controls. `alphacode gui --preview` opens the interface without connecting to a server, and `alphacode gui --once` prints a status snapshot.

The desktop connects through the local AlphaCode server. It supports its own chat, live-session prompts, model selection, saved chats, and workspace selection. The Terminal button opens the terminal interface for commands and workflows that the desktop does not yet support. Native saved sessions that are not live show their recent history read-only; resume them in the terminal to continue. Imported sessions still need the terminal for history and resumption. GUI availability is currently limited to Windows; the terminal interface remains available on all supported platforms.

Unsent drafts stay with their chat while the desktop is open. Switching chats restores the corresponding draft; successful delivery clears only the submitted draft. Drafts are held in memory. Command passwords are never stored as chat drafts. The Terminal button resumes the selected native chat when it has a valid session ID; otherwise it opens a terminal in the selected workspace.

Use **Jump to latest** after scrolling back through a response. Ctrl+End also returns to the latest response while the conversation has focus. Ctrl+N starts a new chat, Ctrl+L focuses the composer, Escape clears chat search, and Ctrl+Shift+C copies the response. Increase or decrease reading text with Ctrl+plus / Ctrl+minus, or reset with Ctrl+0; the size is saved and can also be changed from the More menu. The initial window fits the available desktop area and starts with a collapsed sidebar on smaller screens.

Windows builds require the .NET Framework C# compiler, Windows PowerShell, and the Windows resource compiler. Cross-compiled Windows builds do not include the desktop host and show an error if GUI launch is requested.
