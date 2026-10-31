//! A checkpoint budget catches varied probes that byte-identical retry guards miss.
use serde_json::Value;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

const MAX_PROBES: u32 = 12;
static PROGRESS: LazyLock<Mutex<HashMap<String, (u64, u32)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn clear(session: &str) {
    PROGRESS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(session);
}

fn advance(session: &str, fingerprint: u64) -> Result<(), String> {
    let mut progress = PROGRESS.lock().unwrap_or_else(|error| error.into_inner());
    if progress.len() >= 512
        && !progress.contains_key(session)
        && let Some(oldest) = progress.keys().next().cloned()
    {
        progress.remove(&oldest);
    }
    let entry = progress
        .entry(session.to_string())
        .or_insert((fingerprint, 0));
    if entry.0 != fingerprint {
        *entry = (fingerprint, 0);
    }
    if entry.1 >= MAX_PROBES {
        return Err("CTF checkpoint budget exhausted: 12 probing tool calls since challenge.md last changed. Stop issuing payload variants. Read the hints and closed hypotheses, record the observations, close unsupported hypotheses, and write an evidence-backed alternate hypothesis to challenge.md before further probing. A cosmetic edit is not progress; do not reopen a closed hypothesis without new evidence. This does not indicate a solve.".into());
    }
    entry.1 += 1;
    Ok(())
}

pub(super) fn check(
    session: &str,
    directory: Option<&Path>,
    tool: &str,
    input: &Value,
) -> Result<(), String> {
    let network = super::scope::is_network_tool(tool)
        || (tool == "bash"
            && super::coerce_command_field(input, tool)
                .ok()
                .is_some_and(|command| {
                    let lower = command.to_ascii_lowercase();
                    [
                        "http://",
                        "https://",
                        "curl ",
                        "wget ",
                        "requests.",
                        "urllib.",
                        "hydra ",
                        "patator ",
                    ]
                    .iter()
                    .any(|needle| lower.contains(needle))
                }));
    if !network {
        return Ok(());
    }
    let Some(directory) = directory else {
        return Ok(());
    };
    let path = directory.join("challenge.md");
    if !path.is_file() {
        return Ok(());
    }
    let mut bytes = Vec::new();
    let file = std::fs::File::open(path).map_err(|_| {
        "CTF checkpoint unreadable; recover challenge.md before probing".to_string()
    })?;
    file.take(12 * 1024).read_to_end(&mut bytes).map_err(|_| {
        "CTF checkpoint read failed; recover challenge.md before probing".to_string()
    })?;
    let mut hash = DefaultHasher::new();
    bytes.hash(&mut hash);
    advance(session, hash.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_payloads_share_a_budget_until_evidence_is_checkpointed() {
        let session = uuid::Uuid::new_v4().to_string();
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("challenge.md"), "H1: inspect routes").unwrap();
        for n in 0..MAX_PROBES {
            assert!(
                check(
                    &session,
                    Some(directory.path()),
                    "bash",
                    &serde_json::json!({"command":format!("curl https://example.test/{n}")})
                )
                .is_ok()
            );
        }
        assert!(
            check(
                &session,
                Some(directory.path()),
                "webfetch",
                &serde_json::json!({"url":"https://example.test/new-guess"})
            )
            .is_err()
        );
        assert!(check(&session, Some(directory.path()), "read", &Value::Null).is_ok());
        std::fs::write(
            directory.path().join("challenge.md"),
            "H1 closed: wildcard responses; H2: inspect supplied source",
        )
        .unwrap();
        assert!(check(&session, Some(directory.path()), "webfetch", &Value::Null).is_ok());
        clear(&session);
    }
}
