//! Permission setup: check, request, deep-link, and poll the macOS TCC grants
//! needed for desktop control.

use super::osa;
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use serde_json::json;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn accessibility_ok() -> bool {
    // System Events reports whether assistive access is enabled for us.
    osa::run_applescript("tell application \"System Events\" to return UI elements enabled")
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
}

fn screen_recording_ok() -> bool {
    let tmp = std::env::temp_dir().join(format!("alphacode_setup_{}.png", std::process::id()));
    let ok = Command::new("/usr/sbin/screencapture")
        .arg("-x")
        .arg(&tmp)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        && std::fs::metadata(&tmp)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
    let _ = std::fs::remove_file(&tmp);
    ok
}

fn yes_no(b: bool) -> &'static str {
    if b { "granted" } else { "NOT granted" }
}

/// Report status only.
pub fn check_permissions() -> Result<ToolOutput> {
    let ax = accessibility_ok();
    let screen = screen_recording_ok();
    let swift = std::path::Path::new("/usr/bin/swift").exists()
        || Command::new("/usr/bin/which")
            .arg("swift")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

    let mut lines = vec![
        format!("Accessibility (input + AX control): {}", yes_no(ax)),
        format!("Screen Recording (screenshots/OCR): {}", yes_no(screen)),
        format!(
            "Swift toolchain (for OCR):          {}",
            if swift { "present" } else { "missing" }
        ),
    ];
    if !ax || !screen {
        lines.push("Run action='setup' to request these and open the right settings pane.".into());
    }
    Ok(ToolOutput::new(lines.join("\n")).with_metadata(json!({
        "accessibility": ax, "screen_recording": screen, "swift": swift,
    })))
}

/// Request permissions: prompt, deep-link, and poll Accessibility until granted.
pub fn setup() -> Result<ToolOutput> {
    let mut log = Vec::new();

    let ax0 = accessibility_ok();
    let screen0 = screen_recording_ok();
    log.push(format!(
        "Initial: accessibility={}, screen_recording={}",
        ax0, screen0
    ));

    // Trigger the Screen Recording prompt by attempting a capture (already done
    // in screen_recording_ok). For Accessibility, prompt + pre-add alphacode by
    // opening the pane; the trust prompt itself is shown by the host process the
    // first time it calls an AX API.
    if !ax0 {
        // Deep-link to the exact Accessibility pane.
        let _ = Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status();
        log.push(
            "Opened Privacy & Security > Accessibility. Add and enable your terminal/alphacode there."
                .into(),
        );
    }
    if !screen0 {
        let _ = Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .status();
        log.push(
            "Opened Privacy & Security > Screen Recording. Add and enable your terminal/alphacode there."
                .into(),
        );
    }

    // Poll Accessibility for up to ~30s so the agent can report "ready".
    if !ax0 {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut granted = false;
        while Instant::now() < deadline {
            if accessibility_ok() {
                granted = true;
                break;
            }
            sleep(Duration::from_millis(1000));
        }
        log.push(format!(
            "Accessibility after wait: {}",
            if granted {
                "granted"
            } else {
                "still not granted (toggle it, then re-run check_permissions)"
            }
        ));
    }

    let ax = accessibility_ok();
    let screen = screen_recording_ok();
    log.push(format!(
        "Final: accessibility={}, screen_recording={}",
        ax, screen
    ));
    if !ax {
        log.push(
            "NOTE: the Accessibility toggle cannot be enabled programmatically (macOS security). \
             It is the one switch you must flip by hand."
                .into(),
        );
    }

    Ok(ToolOutput::new(log.join("\n")).with_metadata(json!({
        "accessibility": ax, "screen_recording": screen,
    })))
}
