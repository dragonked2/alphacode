# Windows READ and release corrections for 1.0.94

READ executes in the native AlphaCode process, using the Windows account and
filesystem visible to that process. It does not start an isolated filesystem
sandbox. Remote sessions execute on the server host; mapped drives must be
visible to that server account/session. Windows ACLs still apply.

The reported directory failure had two code paths: directories were opened as
text files (Windows reports access denied), and read-only tools shared a
protected-target policy intended for destructive writes. READ now enumerates
local directories, including hidden entries, and READ/LS resolve native paths
without applying destructive-write policy. Writes retain their existing guards.
No configuration change is needed for local directory reads.

Directory results are bounded to 1,000 entries per request. Offset/limit page
through native enumeration order; each returned page is sorted with folders
first. Pagination is best effort if directory contents change. Symlinks are
marked with @ and children are never recursively opened. Missing paths and
actual permission failures remain errors. Regression coverage includes the
current Windows drive root, hidden folders, pagination, empty directories, and
preservation of write protection. The reporter's L: mapping is unavailable here.

The TUI keeps the primary Enter action visible as a compact glyph in very narrow
layouts. The desktop composer supports Alt+Enter as well as Shift+Enter for a
newline. The desktop header sizes to wrapped actions so the title remains
visible at minimum window width. CI fixes the Clippy failure in the import browser and allows time for
macOS test/cache cleanup, with test debug symbols disabled to reduce artifacts.

Release publication remains gated on successful CI for the exact source commit,
all eight platform builds, packaged version checks, and combined checksums.

Follow-up corrections: AlphaX automatic routes use a product label instead of
upstream URLs, including stale remote picker catalogs. Remote effort selection
is forwarded only to the server, avoiding an unsupported-effort error from the
local stand-in. Todo accepts active/started/in-progress aliases as in_progress.
