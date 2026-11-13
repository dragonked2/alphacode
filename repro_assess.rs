//! Reproduce the session's false-positive denials using the project's own
//! command-risk assessor, so the fix is validated against real behaviour
//! rather than guessed at.

use alphacode_command_risk::{assess, RiskContext};
use std::path::PathBuf;

fn main() {
    let ctx = RiskContext {
        working_dir: Some(PathBuf::from("C:\\Users\\or0to\\OneDrive\\Desktop")),
        home_dir: Some(PathBuf::from("C:\\Users\\or0to")),
        ..Default::default()
    };

    let cases = [
        "python3 -c \"import base64, json, hmac, hashlib\"",
        "python3 -c \"print('hi')\" 2>/dev/null",
        "python3 -c \"import json; print(json.dumps({\\\"a\\\":1}))\"",
        "python3 -c \"import base64\" 2>/dev/null",
        "python3 -c \"print(1)\" 2>/dev/null",
        "python3 -c \"import base64, json; print(1)\" 2>/dev/null",
        "curl -s https://example.test 2>/dev/null",
        "find . -name '*.rs' 2>/dev/null | head -20",
    ];

    let mut any_failed = false;
    for command in cases {
        let a = assess(command, &ctx);
        let ok = matches!(a.level, alphacode_command_risk::RiskLevel::Safe);
        if !ok {
            any_failed = true;
        }
        println!(
            "[{}] {:?} <- {}",
            if ok { "OK " } else { "BAD" },
            a.level,
            command
        );
    }
    if any_failed {
        std::process::exit(1);
    }
}