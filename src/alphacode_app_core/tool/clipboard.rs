use crate::alphacode_tool_core::{Tool, ToolContext};
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

pub struct ClipboardTool;

#[async_trait]
impl Tool for ClipboardTool {
    fn name(&self) -> &str {
        "clipboard"
    }

    fn description(&self) -> &str {
        "Copy text to or paste text from the system clipboard. Use 'copy' to save text to clipboard, 'paste' to read from clipboard. Essential for transferring text between the agent and other applications."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["copy", "paste"],
                    "description": "copy: save text to clipboard. paste: read text from clipboard."
                },
                "text": {
                    "type": "string",
                    "description": "Text to copy to clipboard (required for 'copy' action)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("paste");

        match action {
            "copy" => {
                let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");

                if text.is_empty() {
                    return Ok(ToolOutput::new(
                        "Error: 'text' parameter is required for 'copy' action.".to_string(),
                    ));
                }

                match copy_to_clipboard(text) {
                    Ok(()) => Ok(ToolOutput::new(format!(
                        "Copied {} characters to clipboard.",
                        text.len()
                    ))),
                    Err(e) => Ok(ToolOutput::new(format!("Failed to copy to clipboard: {e}"))),
                }
            }
            "paste" => match paste_from_clipboard() {
                Ok((text, truncated)) => {
                    // Truncate at a valid UTF-8 boundary: slicing `&text[..500]`
                    // panicked on multi-byte characters (e.g. CJK or emoji
                    // pasted from a browser).
                    let preview = if text.len() > 500 {
                        format!(
                            "{}... ({} total chars)",
                            crate::alphacode_core::util::truncate_str(&text, 500),
                            text.chars().count()
                        )
                    } else {
                        text.clone()
                    };
                    let preview = if truncated {
                        format!(
                            "{}\n\n[Clipboard content truncated after {} MiB to protect context and memory.]",
                            crate::alphacode_core::util::truncate_str(&preview, 2_000),
                            MAX_CLIPBOARD_BYTES / (1024 * 1024)
                        )
                    } else {
                        preview
                    };
                    Ok(ToolOutput::new(preview))
                }
                Err(e) => Ok(ToolOutput::new(format!(
                    "Failed to read from clipboard: {e}"
                ))),
            },
            _ => Ok(ToolOutput::new(format!(
                "Unknown action '{action}'. Use 'copy' or 'paste'."
            ))),
        }
    }
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        // Try xclip, then xsel
        if let Ok(mut child) = Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()?;
            return Ok(());
        }
        // Fallback: write to a per-user runtime file. `std::env::temp_dir()`
        // is shared between users on multi-user Linux hosts, which leaked
        // clipboard contents across accounts; the runtime dir is per-uid.
        let path = clipboard_fallback_path();
        std::fs::write(&path, text)?;
        let _ = crate::alphacode_core::fs::set_permissions_owner_only(&path);
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        // Windows: use clip command
        use std::io::Write;
        use std::process::Command;
        let mut child = Command::new("cmd")
            .args(["/c", "clip"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(ref mut stdin) = child.stdin {
            // Windows clip expects UTF-16LE
            let utf16: Vec<u16> = text.encode_utf16().collect();
            let bytes: Vec<u8> = utf16.iter().flat_map(|w| w.to_le_bytes()).collect();
            stdin.write_all(&bytes)?;
        }
        child.wait()?;
        Ok(())
    }
}

fn paste_from_clipboard() -> Result<(String, bool)> {
    #[cfg(target_os = "macos")]
    {
        run_clipboard_reader("pbpaste", &[])
    }
    #[cfg(target_os = "linux")]
    {
        // Try xclip, then xsel
        if let Ok(output) = run_clipboard_reader("xclip", &["-selection", "clipboard", "-o"])
            && (!output.0.is_empty() || !output.1)
        {
            return Ok(output);
        }
        if let Ok(output) = run_clipboard_reader("xsel", &["--output", "--clipboard"])
            && (!output.0.is_empty() || !output.1)
        {
            return Ok(output);
        }
        // Fallback: read from the per-user runtime file (see copy).
        let path = clipboard_fallback_path();
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((String::new(), false));
            }
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::with_capacity(MAX_CLIPBOARD_BYTES + 1);
        file.take((MAX_CLIPBOARD_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        let truncated = bytes.len() > MAX_CLIPBOARD_BYTES;
        bytes.truncate(MAX_CLIPBOARD_BYTES);
        Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
    }
    #[cfg(target_os = "windows")]
    {
        // -NoProfile skips user profile scripts: measurably faster on every
        // call and keeps the paste isolated from profile-side side effects.
        run_clipboard_reader(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-Clipboard -Raw",
            ],
        )
    }
}

/// Capture clipboard output with a hard byte cap and timeout. Readers continue
/// draining after the cap so a large paste cannot deadlock its producer.
fn run_clipboard_reader(program: &str, args: &[&str]) -> Result<(String, bool)> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture {program} output"))?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::with_capacity(MAX_CLIPBOARD_BYTES + 1);
        let mut buffer = [0u8; 16 * 1024];
        let mut truncated = false;
        loop {
            let read = stdout.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            let remaining = MAX_CLIPBOARD_BYTES.saturating_sub(bytes.len());
            let keep = read.min(remaining);
            bytes.extend_from_slice(&buffer[..keep]);
            truncated |= keep < read;
        }
        Ok::<_, std::io::Error>((bytes, truncated))
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let (bytes, truncated) = reader
                    .join()
                    .map_err(|_| anyhow::anyhow!("clipboard reader thread failed"))??;
                if !status.success() {
                    anyhow::bail!("{program} exited with {status}");
                }
                return Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated));
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                anyhow::bail!("{program} timed out while reading the clipboard");
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(error.into());
            }
        }
    }
}

/// Shared fallback file for headless Linux where neither xclip nor xsel is
/// available. Lives in the per-user runtime dir (not the shared temp dir) so
/// clipboard contents never leak across user accounts.
#[cfg(target_os = "linux")]
fn clipboard_fallback_path() -> std::path::PathBuf {
    crate::storage::runtime_dir().join("clipboard-fallback.txt")
}
