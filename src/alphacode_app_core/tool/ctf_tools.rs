//! CTF-specific tools for binary analysis, crypto solving, and forensics.
//!
//! These tools wrap common CTF utilities (pwntools, binwalk, steghide,
//! radare2, z3, etc.) with the same bounded-execution and output-capping
//! patterns used by the recon tools.

use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

/// Default timeout for CTF tool invocations.
const CTF_TOOL_TIMEOUT: Duration = Duration::from_secs(120);

/// Bytes retained between chunks when streaming a file looking for flags, so a
/// match spanning a chunk boundary is not missed.
const MAX_CARRY: usize = 4096;

/// Locate a working Python interpreter.
///
/// Hardcoding `python3` fails outright on Windows (where the launcher is
/// `py` or `python`) and on many macOS/virtualenv setups, so the tool would
/// report "failed to run" even when Python is installed. Mirrors the lookup
/// already used by `scrapling`.
async fn find_python() -> Result<String> {
    super::python::find_python()
        .await
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow::anyhow!("no usable Python interpreter found (checked PYTHON, PATH, and standard Windows install locations)"))
}

/// Write `script` to a unique temp file and run it with the resolved Python
/// interpreter, cleaning the file up on every exit path.
///
/// `label` is used for the temp-file prefix and error messages only. The
/// previous per-tool copies named the file after `std::process::id()`, so two
/// concurrent invocations inside one agent process overwrote each other's
/// script and the second run silently executed the first one's exploit.
async fn run_python_script(
    label: &str,
    script: &str,
    workdir: Option<&str>,
    timeout: Option<Duration>,
) -> Result<std::process::Output> {
    let python = find_python().await?;
    let timeout = timeout.unwrap_or(CTF_TOOL_TIMEOUT);

    let dir = tempfile::Builder::new()
        .prefix(&format!("alphacode_{label}_"))
        .tempdir()?;
    let script_path = dir.path().join(format!("{label}.py"));
    std::fs::write(&script_path, script)?;

    // `TempDir`'s Drop removes the directory, so the file is cleaned up on the
    // timeout path too, not just on the success path.
    let args = vec![script_path.to_string_lossy().into_owned()];
    let working_dir = workdir.map(std::path::Path::new);
    let output = super::recon_common::run_bounded_in(&python, &args, working_dir, timeout)
        .await
        .map_err(|error| anyhow::anyhow!("Failed to run {label} script: {error}"))?;

    Ok(output)
}

/// Merge stdout and stderr into one report.
///
/// Reporting only stderr loses the payload: pwntools, GDB harnesses and
/// `print()`-heavy solvers routinely write their interesting output to stdout
/// and their diagnostics to stderr, or vice versa.
fn render_output(output: &std::process::Output) -> (String, bool) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut text = stdout.to_string();
    if !stderr.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("--- stderr ---\n");
        text.push_str(&stderr);
    }
    (text, output.status.success())
}

/// Locate Ghidra's `analyzeHeadless` launcher.
///
/// The old lookup hardcoded three Unix paths and advertised `GHIDRA_HOME` in
/// its error message without ever reading it, so a correctly configured
/// install (and every Windows install) reported "Ghidra not found".
fn find_ghidra_headless() -> Result<String> {
    let exe = if cfg!(windows) {
        "analyzeHeadless.bat"
    } else {
        "analyzeHeadless"
    };

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("GHIDRA_INSTALL_DIR").or_else(|_| std::env::var("GHIDRA_HOME"))
    {
        candidates.push(std::path::PathBuf::from(home).join("support").join(exe));
    }
    for fixed in [
        "/opt/ghidra",
        "/usr/local/ghidra",
        "/Applications/ghidra",
        "C:\\ghidra",
        "C:\\Program Files\\ghidra",
    ] {
        candidates.push(std::path::PathBuf::from(fixed).join("support").join(exe));
    }
    if let Some(user) = dirs::home_dir() {
        candidates.push(user.join("ghidra").join("support").join(exe));
    }

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    Err(anyhow::anyhow!(
        "Ghidra's analyzeHeadless not found. Install Ghidra from https://ghidra-sre.org/ \
         and set GHIDRA_INSTALL_DIR to the installation directory."
    ))
}

/// Run a CTF binary and render its output, mapping a missing binary to an
/// install hint. `install` is e.g. `apt install binwalk`.
async fn run_ctf_binary(binary: &str, args: &[String], install: &str) -> Result<String> {
    let output = super::recon_common::run_bounded(binary, args, CTF_TOOL_TIMEOUT)
        .await
        .map_err(|e| {
            if e.starts_with("failed to run") {
                anyhow::anyhow!("{e}. Install with: {install}")
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;
    let (text, ok) = render_output(&output);
    if !ok {
        return Err(anyhow::anyhow!(
            "{binary} exited with {}: {text}",
            output.status
        ));
    }
    Ok(text)
}

// ============================================================================
// pwntools — Python exploit development framework
// ============================================================================

pub struct PwntoolsTool;

#[derive(Deserialize)]
struct PwntoolsInput {
    /// Python script to execute with pwntools available
    script: String,
    /// Optional remote host
    #[serde(default)]
    host: Option<String>,
    /// Optional remote port
    #[serde(default)]
    port: Option<u16>,
    /// Working directory for the script
    #[serde(default)]
    workdir: Option<String>,
}

#[async_trait]
impl Tool for PwntoolsTool {
    fn name(&self) -> &str {
        "pwntools"
    }

    fn description(&self) -> &str {
        "Execute Python exploit scripts using pwntools. Use for binary exploitation (pwn) CTF challenges. The script has access to `from pwn import *`. For remote exploits, provide host and port."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["script"],
            "properties": {
                "intent": super::intent_schema_property(),
                "script": {
                    "type": "string",
                    "description": "Python script to execute. `from pwn import *` is already imported."
                },
                "host": {
                    "type": "string",
                    "description": "Remote host to connect to (optional)."
                },
                "port": {
                    "type": "integer",
                    "description": "Remote port to connect to (optional)."
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory for script execution (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: PwntoolsInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid pwntools input: {}", e))?;

        // The description promises `from pwn import *` is already in scope and
        // that host/port are wired up. Both were previously accepted and then
        // discarded, so every remote exploit failed with NameError. Prepend the
        // import and bind the target only when the caller supplied it —
        // shadowing a model-defined `context` would be surprising, so bind
        // under a distinct name and let the script use `remote()` directly.
        let mut preamble = String::from("from pwn import *\n");
        if let Some(ref host) = params.host
            && let Some(port) = params.port
        {
            preamble.push_str(&format!(
                "remote = pwn.remote({:?}, {})\ncontext = remote\n",
                host, port
            ));
        }
        let script = format!("{preamble}{}", params.script);

        let output =
            run_python_script("pwntools", &script, params.workdir.as_deref(), None).await?;

        let (text, status_ok) = render_output(&output);

        let mut metadata = HashMap::new();
        metadata.insert("tool".to_string(), json!("pwntools"));
        metadata.insert("script_len".to_string(), json!(params.script.len()));
        metadata.insert("host".to_string(), json!(params.host));
        metadata.insert("port".to_string(), json!(params.port));

        if !status_ok {
            return Err(anyhow::anyhow!(
                "pwntools script failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                text
            ));
        }

        Ok(ToolOutput::new(text)
            .with_title("pwntools execution result")
            .with_metadata(json!(metadata)))
    }
}

impl PwntoolsTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// binwalk — firmware/file extraction tool
// ============================================================================

pub struct BinwalkTool;

#[derive(Deserialize)]
struct BinwalkInput {
    /// File to analyze
    file: String,
    /// Extract embedded files (-e flag)
    #[serde(default)]
    extract: bool,
    /// Deep recursive extraction (-M flag)
    #[serde(default)]
    recursive: bool,
    /// Show signature analysis only (--run-as=signature)
    #[serde(default)]
    signatures_only: bool,
}

#[async_trait]
impl Tool for BinwalkTool {
    fn name(&self) -> &str {
        "binwalk"
    }

    fn description(&self) -> &str {
        "Analyze and extract embedded files using binwalk. Use for forensics CTF challenges involving firmware, images, or composite files. Use extract=true to carve out embedded files."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the file to analyze."
                },
                "extract": {
                    "type": "boolean",
                    "description": "Extract embedded files. Default: false."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Deep recursive extraction (-M). Default: false."
                },
                "signatures_only": {
                    "type": "boolean",
                    "description": "Show signature analysis only. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: BinwalkInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid binwalk input: {}", e))?;

        let mut args = Vec::new();
        if params.extract {
            args.push("-e".to_string());
        }
        if params.recursive {
            args.push("-M".to_string());
        }
        if params.signatures_only {
            args.push("--run-as=signature".to_string());
        }
        args.push(params.file.clone());

        let output = run_ctf_binary("binwalk", &args, "apt install binwalk").await?;

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));
        metadata.insert("extract".to_string(), json!(params.extract));

        Ok(ToolOutput::new(output)
            .with_title(format!("binwalk: {}", params.file))
            .with_metadata(json!(metadata)))
    }
}

impl BinwalkTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// steghide — steganography extraction tool
// ============================================================================

pub struct SteghideTool;

#[derive(Deserialize)]
struct SteghideInput {
    /// Image/audio file to extract from
    file: String,
    /// Passphrase (empty for none)
    #[serde(default)]
    passphrase: String,
    /// Extract to specific path
    #[serde(default)]
    output_path: Option<String>,
}

#[async_trait]
impl Tool for SteghideTool {
    fn name(&self) -> &str {
        "steghide"
    }

    fn description(&self) -> &str {
        "Extract hidden data from images/audio using steghide. Use for forensics CTF challenges involving steganography. Tries extraction with the provided passphrase (empty string for no passphrase)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the image/audio file."
                },
                "passphrase": {
                    "type": "string",
                    "description": "Passphrase for extraction. Empty string for no passphrase. Default: ''"
                },
                "output_path": {
                    "type": "string",
                    "description": "Output path for extracted file (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: SteghideInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid steghide input: {}", e))?;

        let mut args = vec![
            "extract".to_string(),
            "-sf".to_string(),
            params.file.clone(),
            "-f".to_string(),
        ];
        if !params.passphrase.is_empty() {
            args.push("-p".to_string());
            args.push(params.passphrase.clone());
        }
        if let Some(ref op) = params.output_path {
            args.push("-xf".to_string());
            args.push(op.clone());
        }

        let output = run_ctf_binary("steghide", &args, "apt install steghide").await?;

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));
        metadata.insert(
            "passphrase_used".to_string(),
            json!(!params.passphrase.is_empty()),
        );

        Ok(ToolOutput::new(output)
            .with_title(format!("steghide: {}", params.file))
            .with_metadata(json!(metadata)))
    }
}

impl SteghideTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// radare2 — reverse engineering framework
// ============================================================================

pub struct Radare2Tool;

#[derive(Deserialize)]
struct Radare2Input {
    /// Binary file to analyze
    file: String,
    /// radare2 commands to execute (semicolon-separated)
    commands: String,
    /// Analyze binary before running commands (-A flag)
    #[serde(default)]
    analyze: bool,
}

#[async_trait]
impl Tool for Radare2Tool {
    fn name(&self) -> &str {
        "radare2"
    }

    fn description(&self) -> &str {
        "Analyze binaries using radare2. Use for reverse engineering CTF challenges. Provide radare2 commands as a semicolon-separated string (e.g., 'aaa; afl; pdf @main'). Set analyze=true for automatic analysis before running commands."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file", "commands"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the binary file."
                },
                "commands": {
                    "type": "string",
                    "description": "radare2 commands, semicolon-separated (e.g., 'aaa; afl; pdf @main')."
                },
                "analyze": {
                    "type": "boolean",
                    "description": "Run automatic analysis (-A) before commands. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: Radare2Input = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid radare2 input: {}", e))?;

        let mut args = Vec::new();
        if params.analyze {
            args.push("-A".to_string());
        }
        args.push("-q".to_string());
        args.push("-c".to_string());
        args.push(params.commands.clone());
        args.push(params.file.clone());

        let output = run_ctf_binary("r2", &args, "apt install radare2").await?;

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));
        metadata.insert("commands".to_string(), json!(params.commands));

        Ok(ToolOutput::new(output)
            .with_title(format!("radare2: {}", params.file))
            .with_metadata(json!(metadata)))
    }
}

impl Radare2Tool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// z3 — SMT constraint solver
// ============================================================================

pub struct Z3Tool;

#[derive(Deserialize)]
struct Z3Input {
    /// Z3 SMT-LIB2 script to execute
    script: String,
    /// Optional timeout in seconds
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[async_trait]
impl Tool for Z3Tool {
    fn name(&self) -> &str {
        "z3"
    }

    fn description(&self) -> &str {
        "Solve SMT constraints using z3. Use for CTF challenges involving constraint solving, cryptography, or reverse engineering. Provide SMT-LIB2 script as input."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["script"],
            "properties": {
                "intent": super::intent_schema_property(),
                "script": {
                    "type": "string",
                    "description": "SMT-LIB2 script to execute."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: Z3Input = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid z3 input: {}", e))?;

        // Pass a unique file instead of piping stdin so execution can share
        // the bounded process runner used by the other CTF tools. Solver output
        // can be enormous for a broad model, so `Command::output()` is unsafe.
        let dir = tempfile::Builder::new().prefix("alphacode_z3_").tempdir()?;
        let script_path = dir.path().join("query.smt2");
        std::fs::write(&script_path, &params.script)?;

        let timeout = params
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(CTF_TOOL_TIMEOUT);

        let args = vec![script_path.to_string_lossy().into_owned()];
        let output = super::recon_common::run_bounded("z3", &args, timeout)
            .await
            .map_err(|error| anyhow::anyhow!("Failed to run z3: {error}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = stdout.to_string();
        if !stderr.trim().is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("--- stderr ---\n");
            result.push_str(&stderr);
        }

        // z3 exits 0 even on an unsatisfiable or malformed script, so success
        // is determined by the answer text rather than the exit status alone.
        let mut metadata = HashMap::new();
        metadata.insert("tool".to_string(), json!("z3"));
        metadata.insert("script_len".to_string(), json!(params.script.len()));
        metadata.insert("sat".to_string(), json!(result.contains("sat")));

        Ok(ToolOutput::new(result)
            .with_title("z3 solver result")
            .with_metadata(json!(metadata)))
    }
}

impl Z3Tool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// exiftool — metadata extraction
// ============================================================================

pub struct ExiftoolTool;

#[derive(Deserialize)]
struct ExiftoolInput {
    /// File to analyze
    file: String,
    /// Extract specific tag (optional)
    #[serde(default)]
    tag: Option<String>,
    /// Output in JSON format
    #[serde(default)]
    json: bool,
}

#[async_trait]
impl Tool for ExiftoolTool {
    fn name(&self) -> &str {
        "exiftool"
    }

    fn description(&self) -> &str {
        "Extract metadata from files using exiftool. Use for forensics CTF challenges involving file metadata analysis. Can extract specific tags or all metadata."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the file to analyze."
                },
                "tag": {
                    "type": "string",
                    "description": "Specific tag to extract (optional)."
                },
                "json": {
                    "type": "boolean",
                    "description": "Output in JSON format. Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: ExiftoolInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid exiftool input: {}", e))?;

        let mut args = Vec::new();
        if params.json {
            args.push("-json".to_string());
        }
        if let Some(ref tag) = params.tag {
            args.push(format!("-{}", tag));
        }
        args.push(params.file.clone());

        let output =
            run_ctf_binary("exiftool", &args, "apt install libimage-exiftool-perl").await?;

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));

        Ok(ToolOutput::new(output)
            .with_title(format!("exiftool: {}", params.file))
            .with_metadata(json!(metadata)))
    }
}

impl ExiftoolTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// volatility — memory forensics
// ============================================================================

pub struct VolatilityTool;

#[derive(Deserialize)]
struct VolatilityInput {
    /// Memory dump file
    file: String,
    /// Volatility plugin to run
    plugin: String,
    /// Additional plugin arguments
    #[serde(default)]
    args: Option<String>,
}

#[async_trait]
impl Tool for VolatilityTool {
    fn name(&self) -> &str {
        "volatility"
    }

    fn description(&self) -> &str {
        "Analyze memory dumps using volatility. Use for forensics CTF challenges involving memory analysis. Provide the plugin name (e.g., 'pslist', 'cmdscan', 'filescan') and optional arguments."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file", "plugin"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the memory dump file."
                },
                "plugin": {
                    "type": "string",
                    "description": "Volatility plugin to run (e.g., 'pslist', 'cmdscan', 'filescan')."
                },
                "args": {
                    "type": "string",
                    "description": "Additional plugin arguments (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: VolatilityInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid volatility input: {}", e))?;

        let mut args = vec!["-f".to_string(), params.file.clone(), params.plugin.clone()];
        if let Some(ref a) = params.args {
            args.push(a.clone());
        }

        let output = run_ctf_binary("volatility", &args, "pip install volatility3").await?;

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));
        metadata.insert("plugin".to_string(), json!(params.plugin));

        Ok(ToolOutput::new(output)
            .with_title(format!("volatility: {} on {}", params.plugin, params.file))
            .with_metadata(json!(metadata)))
    }
}

impl VolatilityTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// tshark — network protocol analyzer
// ============================================================================

pub struct TsharkTool;

#[derive(Deserialize)]
struct TsharkInput {
    /// PCAP file to analyze
    file: String,
    /// Display filter (Wireshark filter syntax)
    #[serde(default)]
    filter: Option<String>,
    /// Fields to extract (-T fields)
    #[serde(default)]
    fields: Option<String>,
    /// Export objects to directory
    #[serde(default)]
    export_dir: Option<String>,
}

#[async_trait]
impl Tool for TsharkTool {
    fn name(&self) -> &str {
        "tshark"
    }

    fn description(&self) -> &str {
        "Analyze network captures using tshark. Use for forensics CTF challenges involving PCAP analysis. Supports display filters, field extraction, and object export."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the PCAP file."
                },
                "filter": {
                    "type": "string",
                    "description": "Display filter in Wireshark syntax (optional)."
                },
                "fields": {
                    "type": "string",
                    "description": "Fields to extract, e.g., 'ip.src,dst.port' (optional)."
                },
                "export_dir": {
                    "type": "string",
                    "description": "Directory to export objects (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: TsharkInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid tshark input: {}", e))?;

        let mut args = vec!["-r".to_string(), params.file.clone()];

        if let Some(ref f) = params.filter {
            args.push("-Y".to_string());
            args.push(f.clone());
        }
        if let Some(ref fields) = params.fields {
            args.push("-T".to_string());
            args.push("fields".to_string());
            args.push("-e".to_string());
            args.push(fields.clone());
        }
        if let Some(ref dir) = params.export_dir {
            // tshark aborts with "does not exist" unless the target directory
            // is already present, so create it rather than surfacing a
            // confusing export failure.
            std::fs::create_dir_all(dir)
                .map_err(|e| anyhow::anyhow!("Could not create export dir {dir}: {e}"))?;
            args.push("--export-objects".to_string());
            args.push(format!("http,{dir}"));
        }

        let output = run_ctf_binary("tshark", &args, "apt install tshark").await?;

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));
        metadata.insert("filter".to_string(), json!(params.filter));
        metadata.insert("export_dir".to_string(), json!(params.export_dir));

        Ok(ToolOutput::new(output)
            .with_title(format!("tshark: {}", params.file))
            .with_metadata(json!(metadata)))
    }
}

impl TsharkTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// ghidra — reverse engineering suite (headless)
// ============================================================================

pub struct GhidraTool;

#[derive(Deserialize)]
struct GhidraInput {
    /// Binary file to analyze
    file: String,
    /// Ghidra script to run
    #[serde(default)]
    script: Option<String>,
    /// Script arguments
    #[serde(default)]
    script_args: Option<String>,
    /// Analysis timeout in seconds
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[async_trait]
impl Tool for GhidraTool {
    fn name(&self) -> &str {
        "ghidra"
    }

    fn description(&self) -> &str {
        "Analyze binaries using Ghidra headless mode. Use for reverse engineering CTF challenges. Can run custom Ghidra scripts (Java/Python) for automated analysis."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the binary file."
                },
                "script": {
                    "type": "string",
                    "description": "Ghidra script name to run (optional)."
                },
                "script_args": {
                    "type": "string",
                    "description": "Arguments for the Ghidra script (optional)."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Analysis timeout in seconds (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: GhidraInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid ghidra input: {}", e))?;

        let headless = find_ghidra_headless()?;
        let project_dir = tempfile::Builder::new()
            .prefix("alphacode_ghidra_")
            .tempdir()?;

        let mut args = vec![
            project_dir.path().to_string_lossy().to_string(),
            "ctf_temp".to_string(), // Project name
            "-import".to_string(),
            params.file.clone(),
            // Delete the imported project on exit. Without this the project
            // survives in the (temporary, but still shared) project dir and a
            // later run prompts for confirmation on stdin — which is /dev/null
            // here, so it aborts. Must precede -postScript, whose trailing tokens
            // are otherwise swallowed as script arguments.
            "-deleteProject".to_string(),
        ];

        if let Some(ref script) = params.script {
            args.push("-postScript".to_string());
            args.push(script.clone());
            if let Some(ref script_args) = params.script_args {
                args.push(script_args.clone());
            }
        }

        let timeout = params
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(300));

        let output = super::recon_common::run_bounded_in(
            &headless,
            &args,
            Some(project_dir.path()),
            timeout,
        )
        .await
        .map_err(|error| anyhow::anyhow!("Failed to run ghidra: {error}"))?;

        let (text, ok) = render_output(&output);
        if !ok {
            return Err(anyhow::anyhow!(
                "ghidra exited with {}: {text}",
                output.status
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));
        metadata.insert("script".to_string(), json!(params.script));

        Ok(ToolOutput::new(text)
            .with_title(format!("ghidra: {}", params.file))
            .with_metadata(json!(metadata)))
    }
}

impl GhidraTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// angr — symbolic execution engine
// ============================================================================

pub struct AngrTool;

#[derive(Deserialize)]
struct AngrInput {
    /// Python script using angr
    script: String,
    /// Working directory
    #[serde(default)]
    workdir: Option<String>,
}

#[async_trait]
impl Tool for AngrTool {
    fn name(&self) -> &str {
        "angr"
    }

    fn description(&self) -> &str {
        "Execute Python scripts using angr for symbolic execution. Use for reverse engineering and pwn CTF challenges requiring automated exploit generation or constraint solving."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["script"],
            "properties": {
                "intent": super::intent_schema_property(),
                "script": {
                    "type": "string",
                    "description": "Python script using angr. `import angr` is available."
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: AngrInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid angr input: {}", e))?;

        let output = run_python_script(
            "angr",
            &params.script,
            params.workdir.as_deref(),
            Some(Duration::from_secs(300)),
        )
        .await?;

        let (text, ok) = render_output(&output);

        let mut metadata = HashMap::new();
        metadata.insert("tool".to_string(), json!("angr"));
        metadata.insert("script_len".to_string(), json!(params.script.len()));

        if !ok {
            return Err(anyhow::anyhow!(
                "angr script failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                text
            ));
        }

        Ok(ToolOutput::new(text)
            .with_title("angr execution result")
            .with_metadata(json!(metadata)))
    }
}

impl AngrTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// hashcat — password/hash cracking
// ============================================================================

pub struct HashcatTool;

#[derive(Deserialize)]
struct HashcatInput {
    /// Hash file or hash string
    hash: String,
    /// Hash mode (-m)
    mode: Option<u32>,
    /// Wordlist path
    #[serde(default)]
    wordlist: Option<String>,
    /// Attack mode (-a): 0=straight, 1=combo, 3=brute-force
    #[serde(default)]
    attack_mode: Option<u32>,
    /// Rules file
    #[serde(default)]
    rules: Option<String>,
    /// Show cracked hashes (--show)
    #[serde(default)]
    show: bool,
}

#[async_trait]
impl Tool for HashcatTool {
    fn name(&self) -> &str {
        "hashcat"
    }

    fn description(&self) -> &str {
        "Crack hashes using hashcat. Use for CTF challenges involving password cracking. Provide the hash, hash mode (e.g., 0=MD5, 1000=NTLM, 22000=bcrypt), and wordlist."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["hash"],
            "properties": {
                "intent": super::intent_schema_property(),
                "hash": {
                    "type": "string",
                    "description": "Hash string or path to hash file."
                },
                "mode": {
                    "type": "integer",
                    "description": "Hash mode (-m). Common: 0=MD5, 100=SHA1, 1000=NTLM, 1400=SHA256, 22000=bcrypt."
                },
                "wordlist": {
                    "type": "string",
                    "description": "Path to wordlist (optional)."
                },
                "attack_mode": {
                    "type": "integer",
                    "description": "Attack mode (-a): 0=straight, 1=combo, 3=brute-force. Default: 0."
                },
                "rules": {
                    "type": "string",
                    "description": "Rules file (optional)."
                },
                "show": {
                    "type": "boolean",
                    "description": "Show cracked hashes (--show). Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: HashcatInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid hashcat input: {}", e))?;

        let mut args = Vec::new();
        if let Some(m) = params.mode {
            args.push("-m".to_string());
            args.push(m.to_string());
        }
        if let Some(am) = params.attack_mode {
            args.push("-a".to_string());
            args.push(am.to_string());
        }
        if params.show {
            args.push("--show".to_string());
        }
        // Positional order for hashcat is: <hashfile> <wordlist> [rules].
        // The old code pushed the wordlist before `--show` and also used
        // `-r <rules>`, which is not a hashcat flag at all; a run with both a
        // wordlist and rules therefore always failed with a usage error.
        // `--show` reads cracked hashes from the potfile and takes no
        // wordlist, so only add positionals when actually cracking.
        if !params.show {
            if let Some(ref wl) = params.wordlist {
                args.push(wl.clone());
            }
            if let Some(ref r) = params.rules {
                args.push(r.clone());
            }
        }
        args.push(params.hash.clone());

        let output = run_ctf_binary("hashcat", &args, "apt install hashcat").await?;

        let mut metadata = HashMap::new();
        metadata.insert("hash".to_string(), json!(params.hash));
        metadata.insert("mode".to_string(), json!(params.mode));
        metadata.insert("show".to_string(), json!(params.show));

        Ok(ToolOutput::new(output)
            .with_title("hashcat result")
            .with_metadata(json!(metadata)))
    }
}

impl HashcatTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// john — John the Ripper password cracker
// ============================================================================

pub struct JohnTool;

#[derive(Deserialize)]
struct JohnInput {
    /// Hash file
    file: String,
    /// Wordlist path
    #[serde(default)]
    wordlist: Option<String>,
    /// Hash format
    #[serde(default)]
    format: Option<String>,
    /// Rules
    #[serde(default)]
    rules: bool,
    /// Show cracked passwords (--show)
    #[serde(default)]
    show: bool,
}

#[async_trait]
impl Tool for JohnTool {
    fn name(&self) -> &str {
        "john"
    }

    fn description(&self) -> &str {
        "Crack password hashes using John the Ripper. Use for CTF challenges involving password cracking. Provide the hash file and optional wordlist."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file": {
                    "type": "string",
                    "description": "Path to the hash file."
                },
                "wordlist": {
                    "type": "string",
                    "description": "Path to wordlist (optional)."
                },
                "format": {
                    "type": "string",
                    "description": "Hash format (e.g., 'md5', 'sha256', 'nt') (optional)."
                },
                "rules": {
                    "type": "boolean",
                    "description": "Use rules. Default: false."
                },
                "show": {
                    "type": "boolean",
                    "description": "Show cracked passwords (--show). Default: false."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: JohnInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid john input: {}", e))?;

        let mut args = Vec::new();
        if let Some(ref fmt) = params.format {
            args.push(format!("--format={fmt}"));
        }
        if let Some(ref wl) = params.wordlist {
            args.push(format!("--wordlist={wl}"));
        }
        if params.rules {
            args.push("--rules".to_string());
        }
        if params.show {
            args.push("--show".to_string());
        }
        args.push(params.file.clone());

        let output = run_ctf_binary("john", &args, "apt install john").await?;

        let mut metadata = HashMap::new();
        metadata.insert("file".to_string(), json!(params.file));
        metadata.insert("show".to_string(), json!(params.show));

        Ok(ToolOutput::new(output)
            .with_title(format!("john: {}", params.file))
            .with_metadata(json!(metadata)))
    }
}

impl JohnTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// CTF flag detector — scan files for flag patterns
// ============================================================================

pub struct FlagScannerTool;

const FLAG_SCANNER_MAX_FILES: usize = 20_000;
const FLAG_SCANNER_MAX_RESULTS: usize = 1_000;
const FLAG_SCANNER_DEFAULT_FILE_SIZE: usize = 64 * 1024 * 1024;
const FLAG_SCANNER_HARD_FILE_SIZE: usize = 256 * 1024 * 1024;
const FLAG_SCANNER_MAX_RESULT_CHARS: usize = 1_000;

#[derive(Deserialize)]
struct FlagScannerInput {
    /// Directory or file to scan
    path: String,
    /// Custom regex pattern
    #[serde(default)]
    pattern: Option<String>,
    /// Search in binary files too
    #[serde(default)]
    binary: bool,
    /// Maximum file size to scan (MB), defaulting to a bounded size
    #[serde(default)]
    max_size_mb: Option<u64>,
}

#[async_trait]
impl Tool for FlagScannerTool {
    fn name(&self) -> &str {
        "flag_scanner"
    }

    fn description(&self) -> &str {
        "Scan files and directories for CTF flag patterns. Searches for common flag formats (flag{...}, CTF{...}, etc.) in text and binary files. Use for quick flag discovery in forensics challenges."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "intent": super::intent_schema_property(),
                "path": {
                    "type": "string",
                    "description": "Directory or file path to scan."
                },
                "pattern": {
                    "type": "string",
                    "description": "Custom regex pattern (optional)."
                },
                "binary": {
                    "type": "boolean",
                    "description": "Search in binary files. Default: false."
                },
                "max_size_mb": {
                    "type": "integer",
                    "description": "Maximum file size in MB (default 64, hard maximum 256)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: FlagScannerInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid flag_scanner input: {}", e))?;

        let pattern = params
            .pattern
            .clone()
            .unwrap_or_else(|| r"(?i)(flag|ctf|htb|pico|eno)\{[^}]+\}".to_string());
        if pattern.len() > 4_096 {
            anyhow::bail!("flag_scanner pattern is too long (maximum 4096 bytes)");
        }

        // Compile once, not once per file: this is a hot loop over a directory
        // tree and `Regex::new` re-parses the pattern every iteration.
        let regex = regex::Regex::new(&pattern)
            .map_err(|e| anyhow::anyhow!("Invalid regex `{pattern}`: {e}"))?;

        // Bound work even when callers omit the optional limit or ask for an
        // impossibly large one. File metadata is checked before allocating its
        // contents, and the subsequent read is limited to max_size + 1 bytes
        // to handle files that grow after the metadata check.
        let max_size = params
            .max_size_mb
            .map(|mb| {
                mb.saturating_mul(1024 * 1024)
                    .min(FLAG_SCANNER_HARD_FILE_SIZE as u64)
            })
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or(FLAG_SCANNER_DEFAULT_FILE_SIZE)
            .max(1);

        let mut results = Vec::new();
        let mut files_scanned = 0u64;
        let mut files_skipped = 0u64;
        let mut total_flags_found = 0u64;
        let mut scan_truncated = false;

        let path = std::path::Path::new(&params.path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", params.path));
        }

        // Keep only a bounded set of paths. A huge generated directory should
        // produce an explicit partial result instead of retaining every path
        // before scanning starts.
        let mut candidates = Vec::new();
        if path.is_file() {
            candidates.push(path.to_path_buf());
        } else {
            for entry in walkdir::WalkDir::new(path).follow_links(false) {
                let Ok(entry) = entry else {
                    files_skipped += 1;
                    continue;
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                if candidates.len() == FLAG_SCANNER_MAX_FILES {
                    scan_truncated = true;
                    break;
                }
                candidates.push(entry.into_path());
            }
        }

        for file_path in &candidates {
            let Ok(metadata) = std::fs::metadata(file_path) else {
                files_skipped += 1;
                continue;
            };
            if !metadata.is_file() || metadata.len() > max_size as u64 {
                files_skipped += 1;
                continue;
            }
            use std::io::Read as _;
            let Ok(file) = std::fs::File::open(file_path) else {
                files_skipped += 1;
                continue;
            };
            let mut bytes = Vec::with_capacity((metadata.len() as usize).min(max_size));
            if file
                .take(max_size.saturating_add(1) as u64)
                .read_to_end(&mut bytes)
                .is_err()
                || bytes.len() > max_size
            {
                files_skipped += 1;
                continue;
            }

            // In text mode skip anything that is not valid UTF-8, which is how
            // binaries are filtered out. The single-file path used to ignore
            // `binary` entirely and always scan lossy text, so it disagreed
            // with the directory path about what counts as a text file.
            let haystack: std::borrow::Cow<'_, str> = if params.binary {
                String::from_utf8_lossy(&bytes)
            } else {
                match String::from_utf8(bytes) {
                    Ok(text) => std::borrow::Cow::Owned(text),
                    Err(_) => {
                        files_skipped += 1;
                        continue;
                    }
                }
            };

            files_scanned += 1;
            for mat in regex.find_iter(haystack.as_ref()) {
                total_flags_found = total_flags_found.saturating_add(1);
                if results.len() < FLAG_SCANNER_MAX_RESULTS {
                    let matched = crate::alphacode_core::util::truncate_str(
                        mat.as_str(),
                        FLAG_SCANNER_MAX_RESULT_CHARS,
                    );
                    let full_path = file_path.display().to_string();
                    let path_label = crate::alphacode_core::util::truncate_str(&full_path, 500);
                    results.push(format!("{path_label}: {matched}"));
                } else {
                    scan_truncated = true;
                    break;
                }
            }
            if results.len() >= FLAG_SCANNER_MAX_RESULTS {
                scan_truncated = true;
                break;
            }
        }

        let mut result = format!(
            "Flag Scanner Results\n====================\nFiles scanned: {} (limit: {})\nFiles skipped (unreadable or over {} MiB file limit): {}\nFlags found: {}\n\n",
            files_scanned,
            FLAG_SCANNER_MAX_FILES,
            max_size / (1024 * 1024),
            files_skipped,
            total_flags_found
        );
        for r in &results {
            result.push_str(r);
            result.push('\n');
        }
        if scan_truncated {
            result.push_str(&format!(
                "\n[Scan stopped at a safety limit: up to {FLAG_SCANNER_MAX_FILES} files and {FLAG_SCANNER_MAX_RESULTS} results are retained. Narrow the path or pattern for complete results.]\n"
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("files_scanned".to_string(), json!(files_scanned));
        metadata.insert("files_skipped".to_string(), json!(files_skipped));
        metadata.insert("flags_found".to_string(), json!(total_flags_found));
        metadata.insert("results_shown".to_string(), json!(results.len()));
        metadata.insert("scan_truncated".to_string(), json!(scan_truncated));
        metadata.insert("pattern".to_string(), json!(pattern));

        Ok(ToolOutput::new(result)
            .with_title(format!("flag_scanner: {total_flags_found} flags found"))
            .with_metadata(json!(metadata)))
    }
}

impl FlagScannerTool {
    pub fn new() -> Self {
        Self
    }
}

// ============================================================================
// CTF challenge classifier — auto-detect challenge type
// ============================================================================

pub struct ChallengeClassifierTool;

#[derive(Deserialize)]
struct ChallengeClassifierInput {
    /// Challenge directory or file
    path: String,
}

#[async_trait]
impl Tool for ChallengeClassifierTool {
    fn name(&self) -> &str {
        "challenge_classifier"
    }

    fn description(&self) -> &str {
        "Auto-classify CTF challenge type based on file analysis. Detects web, pwn, crypto, forensics, rev, misc, and osint challenges. Use for initial triage when challenge type is unknown."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "intent": super::intent_schema_property(),
                "path": {
                    "type": "string",
                    "description": "Challenge directory or file path."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: ChallengeClassifierInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid challenge_classifier input: {}", e))?;

        let path = std::path::Path::new(&params.path);
        let mut signals: HashMap<String, i32> = HashMap::new();

        // Initialize all categories
        for cat in &["web", "pwn", "crypto", "forensics", "rev", "misc", "osint"] {
            signals.insert(cat.to_string(), 0);
        }

        if path.is_file() {
            Self::analyze_file(path, &mut signals)?;
        } else if path.is_dir() {
            for entry in walkdir::WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    Self::analyze_file(entry.path(), &mut signals)?;
                }
            }
        }

        // Determine primary category. `signals` is pre-seeded with every
        // category, so `sorted` is never empty; the `unwrap_or` below is only a
        // defensive fallback and must not borrow from a temporary.
        let mut sorted: Vec<(&String, &i32)> = signals.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

        // `sorted.first()` is `Option<&(&String, &i32)>`, so match ergonomics
        // bind `score` as `&&i32` and one deref would still yield `&i32`.
        let (primary, score) = match sorted.first() {
            Some((cat, score)) => (cat.as_str(), **score),
            None => ("misc", 0),
        };

        let mut result = format!(
            "Challenge Classification\n========================\nPrimary category: {} (score: {})\n\nSignal breakdown:\n",
            primary, score
        );
        for (cat, s) in &sorted {
            result.push_str(&format!("  {}: {}\n", cat, s));
        }

        let mut metadata = HashMap::new();
        metadata.insert("primary_category".to_string(), json!(primary));
        metadata.insert("score".to_string(), json!(score));
        metadata.insert("signals".to_string(), json!(signals));

        Ok(ToolOutput::new(result)
            .with_title(format!("classifier: {}", primary))
            .with_metadata(json!(metadata)))
    }
}

impl ChallengeClassifierTool {
    pub fn new() -> Self {
        Self
    }

    fn analyze_file(path: &std::path::Path, signals: &mut HashMap<String, i32>) -> Result<()> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Web signals
        if ext == "html" || ext == "php" || ext == "js" || ext == "sql" || ext == "json" {
            *signals.entry("web".to_string()).or_insert(0) += 2;
        }
        if name.contains("http")
            || name.contains("web")
            || name.contains("xss")
            || name.contains("sqli")
        {
            *signals.entry("web".to_string()).or_insert(0) += 1;
        }

        // Pwn signals
        if ext == "elf" || ext == "exe" || ext == "so" || ext == "dll" {
            *signals.entry("pwn".to_string()).or_insert(0) += 2;
        }
        if name.contains("pwn") || name.contains("overflow") || name.contains("stack") {
            *signals.entry("pwn".to_string()).or_insert(0) += 1;
        }

        // Crypto signals
        if ext == "sage" || (ext == "py" && name.contains("crypto")) {
            *signals.entry("crypto".to_string()).or_insert(0) += 2;
        }
        if name.contains("rsa")
            || name.contains("aes")
            || name.contains("encrypt")
            || name.contains("cipher")
        {
            *signals.entry("crypto".to_string()).or_insert(0) += 1;
        }

        // Forensics signals
        if ext == "pcap" || ext == "pcapng" || ext == "raw" || ext == "dd" || ext == "e01" {
            *signals.entry("forensics".to_string()).or_insert(0) += 3;
        }
        if name.contains("forensics") || name.contains("steg") || name.contains("memory") {
            *signals.entry("forensics".to_string()).or_insert(0) += 1;
        }

        // Rev signals
        if ext == "apk" || ext == "wasm" || ext == "pyc" {
            *signals.entry("rev".to_string()).or_insert(0) += 2;
        }
        if name.contains("rev") || name.contains("reverse") || name.contains("vm") {
            *signals.entry("rev".to_string()).or_insert(0) += 1;
        }

        // OSINT signals
        if name.contains("osint") || name.contains("geo") || name.contains("social") {
            *signals.entry("osint".to_string()).or_insert(0) += 2;
        }

        // Misc signals
        if name.contains("misc") || name.contains("jail") || name.contains("encoding") {
            *signals.entry("misc".to_string()).or_insert(0) += 1;
        }

        Ok(())
    }
}

// ============================================================================
// CTF auto-solver — automated challenge solving pipeline
// ============================================================================

pub struct CtfAutoSolverTool;

#[derive(Deserialize)]
struct CtfAutoSolverInput {
    /// Challenge directory
    path: String,
    /// Challenge category (if known)
    #[serde(default)]
    category: Option<String>,
    /// CTF platform URL (for auto-submit)
    #[serde(default)]
    ctf_url: Option<String>,
    /// CTF platform token
    #[serde(default)]
    ctf_token: Option<String>,
    /// Challenge ID on platform
    #[serde(default)]
    challenge_id: Option<u32>,
}

#[async_trait]
impl Tool for CtfAutoSolverTool {
    fn name(&self) -> &str {
        "ctf_auto_solver"
    }

    fn description(&self) -> &str {
        "Automated CTF challenge solver. Runs a complete analysis pipeline: file identification, string extraction, flag pattern search, and category-specific analysis. Use for rapid challenge triage and solving."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "intent": super::intent_schema_property(),
                "path": {
                    "type": "string",
                    "description": "Challenge directory path."
                },
                "category": {
                    "type": "string",
                    "description": "Challenge category (optional). Auto-detected if not provided."
                },
                "ctf_url": {
                    "type": "string",
                    "description": "CTF platform URL for auto-submit (optional)."
                },
                "ctf_token": {
                    "type": "string",
                    "description": "CTF platform API token (optional)."
                },
                "challenge_id": {
                    "type": "integer",
                    "description": "Challenge ID on platform (optional)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: CtfAutoSolverInput = serde_json::from_value(input.clone())
            .map_err(|e| anyhow::anyhow!("Invalid ctf_auto_solver input: {}", e))?;

        let path = std::path::Path::new(&params.path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Path does not exist: {}", params.path));
        }

        let mut result = String::new();
        result.push_str("CTF Auto-Solver Pipeline\n========================\n\n");

        // Step 1: File identification
        result.push_str("Step 1: File Identification\n");
        let mut files = Vec::new();
        if path.is_file() {
            files.push(path.to_path_buf());
        } else {
            for entry in walkdir::WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    files.push(entry.path().to_path_buf());
                }
            }
        }
        result.push_str(&format!("Found {} files\n\n", files.len()));

        // Step 2: Flag pattern search
        result.push_str("Step 2: Flag Pattern Search\n");
        let flag_pattern = regex::Regex::new(r"(flag|ctf|htb|pico|eno)\{[^}]+\}")
            .map_err(|e| anyhow::anyhow!("Regex error: {e}"))?;
        let mut flags_found = Vec::new();

        // Stream large files in chunks instead of `fs::read`: a forensics
        // challenge is routinely a multi-gigabyte disk image, and reading one
        // whole would exhaust memory and take the agent down with it.
        let mut buf = vec![0u8; 1024 * 1024];
        let mut leftover = String::new();
        let mut files_skipped = 0usize;
        let mut files_searched = 0usize;
        for file in &files {
            let Ok(mut f) = std::fs::File::open(file) else {
                files_skipped += 1;
                continue;
            };
            files_searched += 1;
            loop {
                let Ok(n) = std::io::Read::read(&mut f, &mut buf) else {
                    break;
                };
                if n == 0 {
                    break;
                }
                // Retain a small tail so a flag straddling a chunk boundary is
                // still matched.
                leftover.push_str(&String::from_utf8_lossy(&buf[..n]));
                if leftover.len() > MAX_CARRY {
                    let split = leftover.len() - MAX_CARRY;
                    for m in flag_pattern.find_iter(&leftover[..split]) {
                        flags_found.push(format!("{}: {}", file.display(), m.as_str()));
                    }
                    leftover.drain(..split);
                }
            }
            for m in flag_pattern.find_iter(&leftover) {
                flags_found.push(format!("{}: {}", file.display(), m.as_str()));
            }
            leftover.clear();
        }
        result.push_str(&format!(
            "Files searched: {} (skipped {} unreadable)\n",
            files_searched, files_skipped
        ));
        result.push_str(&format!("Flags found: {}\n", flags_found.len()));
        for flag in &flags_found {
            result.push_str(&format!("  {}\n", flag));
        }
        result.push('\n');

        // Step 3: Category classification
        result.push_str("Step 3: Category Classification\n");
        let category = params.category.clone().unwrap_or_else(|| {
            let mut signals: HashMap<String, i32> = HashMap::new();
            for cat in &["web", "pwn", "crypto", "forensics", "rev", "misc", "osint"] {
                signals.insert(cat.to_string(), 0);
            }
            for file in &files {
                let _ = ChallengeClassifierTool::analyze_file(file, &mut signals);
            }
            let mut sorted: Vec<(&String, &i32)> = signals.iter().collect();
            // Tie-break on name so an all-zero signal map still yields a stable
            // answer instead of depending on HashMap iteration order.
            sorted.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            // `k` is `&&String` here, so `k.clone()` would yield `&String` and
            // leave the closure returning the wrong type.
            sorted
                .first()
                .map(|(k, _)| (*k).clone())
                .unwrap_or_else(|| "misc".to_string())
        });
        result.push_str(&format!("Detected category: {}\n\n", category));

        // Step 4: Category-specific analysis
        result.push_str("Step 4: Category-Specific Analysis\n");
        match category.as_str() {
            "web" => {
                result.push_str("Web challenge detected. Recommended actions:\n");
                result.push_str("  - Fetch the web page and analyze HTML/JS\n");
                result.push_str("  - Check for common vulnerabilities (XSS, SQLi, SSRF)\n");
                result.push_str("  - Look for hidden parameters and endpoints\n");
            }
            "pwn" => {
                result.push_str("Pwn challenge detected. Recommended actions:\n");
                result.push_str("  - Run checksec on the binary\n");
                result.push_str("  - Analyze with strings and objdump\n");
                result.push_str("  - Identify vulnerability type and craft exploit\n");
            }
            "crypto" => {
                result.push_str("Crypto challenge detected. Recommended actions:\n");
                result.push_str("  - Analyze the encryption scheme\n");
                result.push_str("  - Check for common crypto weaknesses\n");
                result.push_str("  - Write a decryption script\n");
            }
            "forensics" => {
                result.push_str("Forensics challenge detected. Recommended actions:\n");
                result.push_str("  - Run binwalk for embedded files\n");
                result.push_str("  - Check metadata with exiftool\n");
                result.push_str("  - Analyze with steghide/zsteg for steganography\n");
            }
            "rev" => {
                result.push_str("Reverse engineering challenge detected. Recommended actions:\n");
                result.push_str("  - Analyze with radare2 or ghidra\n");
                result.push_str("  - Look for obfuscated code patterns\n");
                result.push_str("  - Use angr for symbolic execution\n");
            }
            _ => {
                result.push_str("General challenge. Recommended actions:\n");
                result.push_str("  - Run strings on all files\n");
                result.push_str("  - Check for encoded/encrypted data\n");
                result.push_str("  - Look for hidden files and metadata\n");
            }
        }
        result.push('\n');

        // Step 5: Report the submission target if platform info was provided.
        if let (Some(url), Some(token), Some(challenge_id)) =
            (&params.ctf_url, &params.ctf_token, &params.challenge_id)
        {
            result.push_str("Step 5: Auto-Submit\n");
            match flags_found.first() {
                // `flags_found` entries are "<path>: <flag>"; the flag itself is
                // what a platform expects, not the whole line.
                Some(line) => {
                    let flag = line
                        .split_once(": ")
                        .map(|(_, f)| f)
                        .unwrap_or(line.as_str());
                    result.push_str(&format!(
                        "Candidate flag for {} challenge {}: {}\n",
                        url, challenge_id, flag
                    ));
                }
                None => result.push_str("No candidate flag found; nothing to submit.\n"),
            }
            // Never echo the token itself — only confirm one was supplied.
            result.push_str(&format!(
                "(authentication token supplied: {} characters; submission is not performed \
                 automatically — confirm the flag with the platform manually)\n",
                token.len()
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("files_analyzed".to_string(), json!(files.len()));
        metadata.insert("flags_found".to_string(), json!(flags_found.len()));
        metadata.insert("category".to_string(), json!(category));

        Ok(ToolOutput::new(result)
            .with_title(format!(
                "ctf_auto_solver: {} flags found",
                flags_found.len()
            ))
            .with_metadata(json!(metadata)))
    }
}

impl CtfAutoSolverTool {
    pub fn new() -> Self {
        Self
    }
}
