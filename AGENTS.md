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

Use `read`, `write`, and `bash` for authorized file and code analysis; `webfetch`, `websearch`, `httpflow`, and `jwt` for permitted network or token analysis; and `memory`, `agentgrep`, and `session_search` for reusable findings. For complex analysis, automate repeatable steps and preserve concise evidence and notes.

Built-in security workflows cover reconnaissance, web and API testing, authentication and authorization checks, injection, configuration review, business logic, binary analysis, verification, reporting, and recovery. Choose focused tools, validate findings, and avoid presenting unverified hypotheses as confirmed issues.
