# Alphacode project instructions

## Build and verification

This repository is one Cargo package, `alphacode`, with its manifest at the repository root and library entry at `src/lib.rs`. Directories named `src/alphacode_*` are modules, not packages; do not pass them to `cargo -p`.

Run the relevant checks with:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo check --workspace --tests
cargo test --lib
cargo clippy --lib -- -D warnings
```

### Do not rebuild the world to check warnings

`RUSTFLAGS` is part of Cargo's fingerprint. If you run
`RUSTFLAGS="-D warnings" cargo clippy …` in the normal target directory, Cargo
recompiles the entire dependency graph and this crate, and the next ordinary
`cargo check` recompiles it *again* on the way back. On this tree that is
minutes in each direction, and it is the single most common way to turn a
fifteen-second check into a ten-minute one.

Run the strict gates in their own target directory instead:

```text
make check-strict      # cargo check --workspace --tests with -D warnings
make clippy-strict     # CI's exact clippy invocation
make test-one FILTER=the_test_name    # filtered run, no relink per check
make test-fast         # cargo nextest when installed, else the plain harness
```

These keep strict and dev artifacts warm independently, so both stay fast.

The lib test binary is ~150 MB and links on every source change. Iterating by
name (`make test-one FILTER=…`) reuses it; running the whole suite to check one
function pays the full link cost for nothing. Reserve the full suite for the
final gate.

On Windows, build outside OneDrive. Keep `CARGO_TARGET_DIR` at `C:\Users\or0to\AppData\Local\Temp\alphacode-target`. Load the VS 2022 MSVC environment in the same PowerShell process before building:

```powershell
$vc = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cmd /c "`"$vc`" >nul 2>&1 && set" | ForEach-Object {
  if ($_ -match '^([^=]+)=(.*)$') {
    [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
  }
}
$env:CARGO_TARGET_DIR = "C:\Users\or0to\AppData\Local\Temp\alphacode-target"
```

When working in a shared checkout, inspect diffs before reverting or stashing; preserve other in-flight changes.

## TUI regression checks

- Keep Windows console reads independent of rendering and daemon work. Preserve the bounded event queue and event order. Never make ordinary typing wait for another key or a paste timer.
- Measure input latency from the console reader through the frame flush so the result includes time waiting in the app's queue. Exercise Windows Terminal while idle, after selecting a model, and during streaming. Check tail latency as well as the median.
- When changing visible TUI wording, update the buffer-level assertions in the same change and run the affected render tests. Verify CI on the exact tagged revision before publishing.

## Security and challenge work

Use security tools only within the target and actions authorized by the user, challenge, or program scope. Read supplied challenge files and scope first, keep testing within that boundary, avoid unrelated data, and report evidence and limitations accurately.

Alphacode includes native recon tools: `subfinder`, `httpx`, `waybackurls`, `gau`, `katana`, `ffuf`, and `dnsx`. They invoke separately installed binaries. Typical Go installs:

```text
go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
go install github.com/projectdiscovery/httpx/cmd/httpx@latest
go install github.com/tomnomnom/waybackurls@latest
go install github.com/lc/gau/v2/cmd/gau@latest
go install github.com/projectdiscovery/katana/cmd/katana@latest
go install github.com/ffuf/ffuf/v2@latest
go install github.com/projectdiscovery/dnsx/cmd/dnsx@latest
```

Use `read`, `write`, and `bash` for authorized file and code analysis; `webfetch`, `websearch`, `httpflow`, and `jwt` for permitted network or token analysis; and `memory`, `alphagrep`, and `session_search` for reusable findings. For complex analysis, automate repeatable steps and preserve concise evidence and notes.

Built-in security workflows cover reconnaissance, web and API testing, authentication and authorization checks, injection, configuration review, business logic, binary analysis, verification, reporting, and recovery. Choose focused tools, validate findings, and avoid presenting unverified hypotheses as confirmed issues.
