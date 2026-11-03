# Windows release: nested-executable removal (AV verdict root cause)

## The report

VirusTotal, `alphacode-windows-x86_64(2).zip`, sha256
`aa30c118d118aaae18d8cf885d534a77daf626aec007aa79024aefe6f279c1ca`:

- `malicious 8`, `undetected 57`, `type-unsupported 8`, `timeout 1`, `failure 1`.
- Popular label `trojan.barys`, family `barys`.

The eight "malicious" entries are **one detection family, not eight independent
verdicts**:

| Engine | Result |
|---|---|
| BitDefender | `Gen:Variant.Barys.381847` |
| MicroWorld-eScan | `Gen:Variant.Barys.381847` |
| ALYac | `Gen:Variant.Barys.381847` |
| VIPRE | `Gen:Variant.Barys.381847` |
| GData | `Gen:Variant.Barys.381847` |
| Emsisoft | `Gen:Variant.Barys.381847 (B)` |
| Arcabit | `Trojan.Barys.D5D397` |
| CTX | `zip.unknown.generic` |

ALYac, Arcabit, eScan, Emsisoft, GData, MicroWorld and VIPRE license the
BitDefender engine; "Barys" is that engine's name for the family. Engines that
did **not** flag it on the same scan include Microsoft, CrowdStrike, SentinelOne,
Kaspersky, ESET-NOD32, Sophos, Fortinet, Trend Micro, Avast, AVG, Avira, Google,
Ikarus, Malwarebytes and Xcitium.

So the correct description is: *seven reseller echoes of one heuristic verdict,
plus one generic "unknown zip" note.* The engine count was misleading; the
underlying signal was a single behavioural heuristic.

## Root cause

`alphacode.exe` contained a **second complete PE**:

1. `build.rs::configure_windows_desktop` compiled a .NET/WPF host
   (`src/cli/gui/GuiHost.cs` with `gui.ps1` embedded as a managed resource) to
   `OUT_DIR/AlphaCode.Desktop.exe`.
2. `src/cli/gui.rs` pulled those bytes in with
   `include_bytes!(env!("ALPHACODE_DESKTOP_HOST_BYTES"))`.
3. On the first `alphacode gui`, it **wrote** that embedded executable to
   `%USERPROFILE%\.alphacode\gui\AlphaCode.Desktop.exe` and **launched** it.

Embed → drop → execute, in one unsigned 94 MB file, is the canonical
packer/dropper shape. Nothing about the program's actual behaviour was
malicious; the *layout* was the signal.

Measured, not assumed — scanning the shipped debug build for `MZ` + valid
`e_lfanew` + `PE\0\0`:

```
before: PE images at offsets [0, 60102752]  (file 92,138,496 bytes)
after:  PE images at offsets [0]            (file 92,358,656 bytes)
```

The second image began at byte 60,102,752 — i.e. 64% of the file was the nested
host.

## What changed

- The host is built either way but is **no longer embedded by default**. It is
  staged beside the binary (`build.rs` copies it to the cargo profile directory)
  and shipped inside the release archive as a sibling.
- `resolve_desktop_host()` looks for it in this order: explicit
  `ALPHACODE_DESKTOP_HOST` → beside the running executable → the legacy
  `~/.alphacode/gui/` location (so an in-place upgrade still finds the copy a
  previous version extracted). Only the opt-in `embedded-gui-host` feature can
  write an extracted copy, and it says so in the error path.
- The release workflow assembles the Windows archive explicitly
  (`zip -j alphacode.exe AlphaCode.Desktop.exe`) instead of letting the generic
  action zip one binary, and fails if either input is missing.
- `scripts/verify-pe-single-image.py` counts PE images and fails above one. It is
  wired into the release job, so this cannot regress silently.
- The Windows VERSIONINFO resource (added earlier) keeps the executable
  attributed; an unnamed, version-less PE scores worse.

## What still needs a human

1. **Code signing.** The binaries are unsigned (`Get-AuthenticodeSignature`
   reports `NotSigned`). This is the single strongest remaining input to
   reputation-based detection and to SmartScreen. An OV/EV certificate deployed
   via Azure Trusted Signing or a hardware token in CI would remove it. No
   amount of source change substitutes for this.
2. **Reputation reset.** BitDefender caches by file hash, so a rebuild produces a
   different verdict path, but the *family* association persists in their
   database. Submit the 1.0.93 binary to
   <https://www.bitdefender.com/submit/> as a false positive and ask for the
   `Gen:Variant.Barys` rule to be reviewed against the new layout. The same form
   exists for the resellers that reuse the engine.
3. **Re-scan.** Upload the 1.0.93 archive to VirusTotal and confirm the count
   drops. Verify relationships (bundled files, dropped files) on the *zip* entry
   — VirusTotal flags the container and the inner exe separately, and the zip
   score is what users see first.

## What was checked and deliberately *not* changed

- **No self-update download-and-execute.** There is no in-binary updater; the
  installer is a separate script the user runs.
- **Browser bridge.** It downloads a native binary from GitHub releases on first
  use. That is a real dropper *if* abused, but it is user-initiated, reads a
  versioned release asset, and is not part of the scanned archive. It stays, but
  it is the next thing to harden if AV noise persists: pin a checksum for the
  asset and verify it before execution.
- **Server-lifetime commands** (`alphacode auto-poke`, `cargo watch`, `tail -f`)
  are exempt from the new auto-backgrounding so their behaviour does not change.

## Verification performed

Two pre-existing lib tests were failing before this work and were fixed as part
of it. Both asserted a mechanism instead of the invariant, so neither indicated
a product bug:

- `take_buffered_line_keeps_capacity_while_backlog_remains` required
  `capacity == capacity_before`, but the large-backlog path calls `split_off`
  (rebuilding the tail buffer) and then shrinks toward
  `READ_BUFFER_RETAIN_CAPACITY`. Capacity legitimately drops from 4 MiB to 2 MiB
  with a backlog still buffered. The test now asserts the actual contract: never
  below the retain floor (no reallocation per line) and never regrown past the
  original.
- `brand_line_shows_wordmark_only_when_banner_is_absent` expected the literal
  string `alphacode` at width 5. The brand line only has a 4-cell budget there,
  so `truncate_header_text` correctly emits `Alp...`; the assertion was
  unsatisfiable. It now checks that the line is non-blank and that its leading
  alphanumeric run is a genuine prefix of the brand.

| Check | Result |
|---|---|
| `cargo check --workspace --all-targets --locked` | exit 0 |
| `cargo clippy --locked --lib --all-targets -- -D warnings` | exit 0 |
| Targeted `cargo test --lib` (auto-background, compaction) | 5 passed |
| `scripts/tests/install-path.tests.ps1` | 46 passed |
| `install.ps1` / `uninstall.ps1` PowerShell parse | OK |
| PE images in `alphacode.exe` | 1 (was 2) |
| `verify-pe-single-image.py` negative control | rejects a synthetic nested binary |
| GUI host contains AppUserModelID | yes |

## Limits

### Version reporting bug found while verifying the artifact

Running the freshly built release binary reported:

```
alphacode v1.0.91 (b7f3ac1)      <- --version
FileVersion 1.0.93               <- Windows VERSIONINFO resource
```

`build.rs::extract_version_from_release_tag` prefers a git tag at HEAD, then
fell back to `git describe --tags --abbrev=0`, which returns the *nearest* tag
— not necessarily one at HEAD. The comment claimed the result was verified
against HEAD; it was not. Two tags (`v1.0.90`, `v1.0.91`) share the commit
b7f3ac1, so every release build of an untagged tree advertised 1.0.91 no matter
what `Cargo.toml` said. The fallback now uses `--exact-match` and returns
nothing without an exact tag, so it falls through to the crate version. Verified
after the fix: the same release binary reports `v1.0.93`.

### Concurrent modification during this work

Three `alphacode` processes were live for this repository during the session
(one `serve`, one `--no-update`, one TUI) and a second agent was actively
editing the same working tree. Consequences to be aware of when reading the
numbers below:

- One intermediate build failed on a transient syntax error in
  `src/alphacode_security_core/chain.rs`, a file this work never touched; it was
  repaired by the other session seconds later and the tree compiled again.
- A full-suite run on the shared tree reported **9 failures**, all in
  `alphacode_tui_style::{palette,harmony}`, `alphacode_tui::ui_help` and
  `tui::ui::tests::onboarding` — files owned by the concurrent session, mid-edit
  (e.g. the onboarding copy now reads "Privacy controls - /telemetry" while its
  test still expects the old footer). The total test count moved from 6456 to
  6482 across runs for the same reason.
- Everything in this document's changeset was verified in isolation: the 12
  tests covering auto-backgrounding, compaction bounding, the GUI-host name
  contract, the read-buffer contract and the two env-override tests all pass.
  `cargo fmt --check` and `cargo clippy -D warnings` pass on the shared tree.

Before tagging 1.0.93, run the gates once with no other writer active.

- Only the Windows x86_64 artifact was analysed; arm64 was not.
- VirusTotal relationship data (bundled files, sandbox behaviours) was
  rate-limited (HTTP 429) during this investigation, so no sandbox report was
  reviewed. The verdict data itself came from the authenticated report API.
- The effectiveness of the change on vendor verdicts is unmeasured until 1.0.93
  is uploaded and re-analysed. Do not claim the detection is "fixed" before that
  scan; the *cause* is fixed and structurally prevented from returning.
