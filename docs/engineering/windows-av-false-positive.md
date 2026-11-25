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

## 1.1.21: plaintext documentation corpus (second root cause)

The nested-PE fix above removed the *dropper* shape. It did not remove the
*stealer* shape, because a second, independent root cause remained.

### The report

Two files were submitted. `alphacode.exe` (sha256
`13be563d...`) was reported by 4 engines — `UDS:Trojan-Spy.Win32.Stealer.fxdp`
(Kaspersky), `Stealer.Agent!8.C2` (Rising), `W64/ABRisk.OUTT-5081` (Varist),
`Detected` (Google) — with the popular label `trojan.stealer/abrisk`. Zenbox
sandbox graded it 64% malicious. `AlphaCode.Desktop.exe` (sha256
`26d070ce...`) was reported by 5 more (APEX, Bkav, Cylance, Kingsoft,
Trapmine).

### Root cause

Thirteen crowdsourced YARA rules matched the main binary. Every one matched
**inert documentation text**, not code:

| Rule | What it matched in our docs |
|---|---|
| `SQLQuery_ConfidentialDataStore` | browser credential-store queries in the CTF forensics bundle (`... from logins`, `... from cookies`, `password_value`) |
| `NoneWindowsUA` | macOS and X11 browser user-agent literals |
| `References_PublicServiceInterface` | `webhook.site/` out-of-band callback examples |
| `ClearWinLogs` | `wevtutil cl Application` / `cl Security` |
| `VM_Evasion_MACAddrComb` | hypervisor MAC prefixes (`00:0C:29`, `00:50:56`, `08:00:27`) |
| `Mimikatz_Strings`, `Impacket` | tool-name lists |

The bundled skill corpus shipped as ~290 `include_str!` calls, so 4.38 MB of
security-methodology prose landed in `.rdata` as contiguous plaintext. An
assessment agent documents exactly these techniques; an infostealer scanner
cannot tell the difference between the documentation and the tool.

Two further contributors:

- **Build-machine paths.** `strip = true` removes the symbol table but not the
  `file!()` string literals every dependency emits, so the binary carried the
  builder's account name 2,121 times. That is a privacy leak and a reputation
  signal (an unsigned PE full of one home directory reads as a dropped payload).
- **Non-Windows user agents** in `webfetch`/`websearch`, which the `NoneWindowsUA`
  rule matched directly.

### What changed

- The corpus is packed by `scripts/bundled_skills_manifest.py` and deflated by
  `build.rs::configure_bundled_skills`. The manifest moved to
  `src/alphacode_base/bundled_skills.json`; `bundled_skills.rs` decodes the blob
  on first use and keeps serving identical `&'static str` bodies.
  **Measured:** 4.38 MB plaintext -> 1.50 MB deflate (66% smaller), and the
  trigger literals vanish (`from logins` x2 -> 0, `webhook.site/` x10 -> 0,
  `mimikatz` x8 -> 0, and so on).
- The Windows release build passes `--remap-path-prefix` for the workspace and
  the cargo registry.
- `webfetch`/`websearch` use Windows/neutral user agents only.
- `scripts/verify-no-av-triggers.py` scans the shipped bytes (ASCII and
  UTF-16LE) for all of those literals and fails the release. Wired into
  `release.yml` and `scripts/release-build.sh`.
- Three orphaned duplicate `SKILL.md` files under `bundled_skills/bugbounty/`
  (`methodology`, `code-security-audit`, `vuln-hunter`) were removed.

### Negative control

`verify-no-av-triggers.py` run against the shipped 1.1.20 binary reproduces
16 trigger matches across 7 rule families — i.e. the guard detects the exact
artifact VirusTotal flagged. A guard that passes everything would be worthless.

### Still needs a human

Code signing remains the single strongest remaining input to reputation-based
detection and SmartScreen (see item 1 above). No source change substitutes for it.

## 1.1.22: the GUI host was its own root cause (third)

The 1.1.21 fixes above targeted `alphacode.exe`. The file the user actually
submitted to VirusTotal was the **other** binary — `AlphaCode.Desktop.exe`, the
313 KB GUI host — and it carried a defect the previous work introduced.

### The report

`AlphaCode.Desktop.exe`, sha256
`f61b747a1961fc96159a02607b7b851c8c025eaaad8e28bcf4e4eaffe272a054`, 313,004
bytes. VirusTotal, 11 malicious / 74 engines:

| Engine | Verdict |
|---|---|
| Arctic Wolf | `Unsafe`, `TR/Dropper.Gen`, `Static AI - Suspicious PE` |
| WithSecure | `Trojan.TR/Dropper.Gen` |
| Bkav Pro | `W32.Malware.DC4032F` |
| CrowdStrike Falcon | `Win/malicious_confidence_70% (D)` |
| Elastic | `Malicious (moderate Confidence)` |
| Malwarebytes | `MachineLearning/Anomalous.100%` |
| SecureAge | `Malicious` |
| Trapmine | `Malicious.moderate.ml.score` |
| TrendMicro-HouseCall | `Trojan.Win32.VSX.PE04CAN` |

File tags: `peexe`, `assembly`, **`overlay`**, **`detect-debug-environment`**,
`calls-wmi`, `obfuscated`. Dropped file recorded by the sandbox:
`__psscriptpolicytest_<random>.ps1` — i.e. the sandbox saw the embedded script
being written out and run by PowerShell.

### Root cause: a self-inflicted PE overlay

`build.rs::append_version_resource` compiled a VERSIONINFO `.res` with `rc.exe`
and then did this:

```rust
let mut data = std::fs::read(host)?;
data.extend_from_slice(&res);      // <- bytes appended after the last section
std::fs::write(host, data)?;
```

Two consequences, both measured rather than assumed:

- **Overlay.** Sections ended at byte 312,320 of a 313,004-byte file: 684 bytes
  of trailing data. That is the shape packers leave, and `overlay` is literally
  one of the VirusTotal tags on the file.
- **The resource was never registered.** Appending bytes does not add a
  resource-directory entry, so Windows ignored them. `Get-Item
  AlphaCode.Desktop.exe` on 1.1.21 reported:

  ```
  FileDescription: AlphaCode Desktop
  ProductName:                     <- empty
  CompanyName:                     <- empty
  LegalCopyright:                  <- empty
  ```

  So the fix intended to make the host "read as a product" produced an
  unsigned, partially unattributed .NET launcher that embeds and executes a
  PowerShell script — which is precisely the dropper profile nine engines
  scored.

### What changed

- The host declares `AssemblyProduct`, `AssemblyCompany`,
  `AssemblyCopyright` and `AssemblyVersion` in `GuiBuildInfo.cs`, so `csc.exe`
  synthesises VERSIONINFO and links it into `.rsrc`. `append_version_resource`
  and its call site were deleted.
- `alphacode.exe` gained the one VERSIONINFO field it never set,
  `LegalCopyright`.
- `scripts/verify-pe-metadata.py` fails the release if either shipped PE has a
  non-zero overlay, or if VERSIONINFO is not readable from the *mapped image*
  (only bytes up to the end of the last section are mapped, which is why the
  appended block was invisible).
- The AV-trigger guard now also runs against `AlphaCode.Desktop.exe` in both
  `release.yml` and `scripts/release-build.sh`; previously only the CLI was
  scanned even though the host embeds `gui.ps1`.

### Measured results

| Check | 1.1.21 host | 1.1.22 host |
|---|---|---|
| PE overlay | 684 bytes | **0 bytes** |
| `ProductName` (Windows) | *(empty)* | `AlphaCode` |
| `CompanyName` (Windows) | *(empty)* | `AlphaCode` |
| `LegalCopyright` (Windows) | *(empty)* | `Copyright (c) Ali Essam` |
| Embedded script identity | `934702dd…` | `934702dd…` (unchanged) |
| `verify-pe-metadata.py` | FAIL | OK |
| `verify-no-av-triggers.py` | OK | OK |
| `verify-pe-single-image.py` | OK (1) | OK (1) |
| Functional launch | window, responding | window, responding |

The GUI was re-launched after the change: `MainWindowHandle != 0`,
`MainWindowTitle = AlphaCode`, `Responding = True`, and the embedded script's
`gui_sha256` matched the source byte-for-byte, so the metadata change did not
alter behaviour.

### Related gap closed: local release builds leaked the builder's paths

`scripts/release-build.sh` is the documented way to build a release locally, but it
did not pass `--remap-path-prefix`, while `.github/workflows/release.yml` did. Building
the 1.1.22 release with the script therefore produced an `alphacode.exe` carrying
`C:\Users\<account>\` **2,099 times** — the same defect the 1.1.21 notes
above describe for CI builds, just on a different path.

That matters beyond privacy: an unsigned PE whose string table is full of a
single home directory is a recognised reputation signal, and it is one of the
inputs that keeps the *family* association alive in vendor databases even
after the specific verdict is fixed. The script now applies the same two remaps
as CI (`workspace -> /alphacode`, `registry -> /cargo/registry`), and
`verify-pe-metadata.py` fails the release if any `C:\Users\` occurrence
survives.


### Why `--remap-path-prefix` alone was not enough

After wiring the remap flags into `scripts/release-build.sh`, the local release binary
still carried the builder's path twice. Those two occurrences were not `file!()`
literals: they came from `env!("CARGO_MANIFEST_DIR")` in
`alphacode_build_support::paths::get_repo_dir` and
`alphacode_telemetry_core::state_support::telemetry_alphacode_repo_dir`, which use the
compile-time checkout path to locate the repo for self-dev mode. `--remap-path-prefix`
rewrites `file!()` and `#[track_caller]` locations; it does not rewrite an
`env!()` string the crate chooses to embed.

Both sites now consult `alphacode_build_meta::is_release_build()` and skip the
compile-time directory in a release build, where the executable-relative and
working-directory fallbacks already cover the same lookup. Measured on the
rebuilt 1.1.22 binary: `or0to` 2,097 -> **0**, `OneDrive` -> **0**, while
`/alphacode` and `/cargo/registry` remain present, so panic messages still point
at the repository layout.
### What is still true and not fixed by source

- **Unsigned binaries.** Still the strongest single input to reputation and
  SmartScreen. This is the remaining human step.
- **`detect-debug-environment`.** The .NET/WPF runtime reaches
  `UnhandledExceptionFilter` through its own exception machinery; the tag is
  runtime behaviour, not a dropper indicator, and it is not removable without
  abandoning the managed host.
- **`calls-wmi` and `obfuscated`** are sandbox behavioural tags inferred from
  the managed runtime and the large embedded script resource. They are not
  matched by the crowdsourced YARA corpus in `tmp/av/is.yar`.
- **Verdicts are per-hash.** A rebuild produces a new hash and therefore new
  verdicts; the effectiveness of this change on the 1.1.22 binary is unmeasured
  until it is uploaded and re-analysed. Do not claim the detection is "fixed"
  on VirusTotal before that scan — the *cause* is fixed and structurally
  guarded.
