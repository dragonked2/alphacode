use crate::alphacode_tool_core::{Tool, ToolContext};
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

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
                let text = input
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

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
                    Err(e) => Ok(ToolOutput::new(format!(
                        "Failed to copy to clipboard: {e}"
                    ))),
                }
            }
            "paste" => match paste_from_clipboard() {
                Ok(text) => {
                    let preview = if text.len() > 500 {
                        format!("{}... ({} total chars)", &text[..500], text.len())
                    } else {
                        text.clone()
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
        // Fallback: write to a temp file
        let path = std::env::temp_dir().join("alphacode_clipboard.txt");
        std::fs::write(&path, text)?;
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

fn paste_from_clipboard() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("pbpaste").output()?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        // Try xclip, then xsel
        if let Ok(output) = Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-o")
            .output()
        {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
        if let Ok(output) = Command::new("xsel")
            .arg("--output")
            .arg("--clipboard")
            .output()
        {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
        // Fallback: read from temp file
        let path = std::env::temp_dir().join("alphacode_clipboard.txt");
        Ok(std::fs::read_to_string(&path).unwrap_or_default())
    }
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .args(["-command", "Get-Clipboard"])
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
