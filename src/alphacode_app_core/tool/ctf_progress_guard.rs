//! Track checkpoint cadence for CTF progress reminders.
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

fn advance(session: &str, fingerprint: u64) -> bool {
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
    entry.1 += 1;
    if entry.1 == MAX_PROBES {
        entry.1 = 0;
        return true;
    }
    false
}

pub(super) fn check(session: &str, directory: Option<&Path>, tool: &str, input: &Value) -> bool {
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
        return false;
    }
    let Some(directory) = directory else {
        return false;
    };
    let path = directory
        .join(super::research_ledger_relative_dir(session))
        .join("challenge.md");
    if !path.is_file() {
        return false;
    }
    let mut bytes = Vec::new();
    let Ok(file) = std::fs::File::open(&path) else {
        return false;
    };
    if file.take(12 * 1024).read_to_end(&mut bytes).is_err() {
        return false;
    }
    let mut hash = DefaultHasher::new();
    bytes.hash(&mut hash);
    advance(session, hash.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_and_other_session_ledgers_do_not_throttle_a_new_session() {
        let root = tempfile::tempdir().unwrap();
        let old = "old/session";
        let new = "new/session";
        std::fs::write(root.path().join("challenge.md"), "stale project ledger").unwrap();
        let old_dir = root
            .path()
            .join(super::super::research_ledger_relative_dir(old));
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join("challenge.md"), "old session evidence").unwrap();
        for count in 1..=MAX_PROBES {
            assert_eq!(
                check(old, Some(root.path()), "webfetch", &Value::Null),
                count == MAX_PROBES
            );
        }
        for _ in 0..(MAX_PROBES + 2) {
            assert!(!check(new, Some(root.path()), "webfetch", &Value::Null));
        }
        let new_dir = root
            .path()
            .join(super::super::research_ledger_relative_dir(new));
        assert_ne!(old_dir, new_dir);
        assert!(!new_dir.exists());
        clear(old);
        clear(new);
    }

    #[test]
    fn varied_probes_only_raise_a_periodic_checkpoint_reminder() {
        let session = uuid::Uuid::new_v4().to_string();
        let directory = tempfile::tempdir().unwrap();
        let ledger_dir = directory
            .path()
            .join(super::super::research_ledger_relative_dir(&session));
        std::fs::create_dir_all(&ledger_dir).unwrap();
        std::fs::write(ledger_dir.join("challenge.md"), "H1: inspect routes").unwrap();
        for count in 1..=MAX_PROBES * 3 {
            assert_eq!(
                check(
                    &session,
                    Some(directory.path()),
                    "bash",
                    &serde_json::json!({"command":format!("curl https://example.test/{count}")})
                ),
                count.is_multiple_of(MAX_PROBES)
            );
        }
        assert!(!check(
            &session,
            Some(directory.path()),
            "read",
            &Value::Null
        ));
        std::fs::write(
            ledger_dir.join("challenge.md"),
            "H1 closed; inspect supplied source",
        )
        .unwrap();
        assert!(!check(
            &session,
            Some(directory.path()),
            "webfetch",
            &Value::Null
        ));
        clear(&session);
    }
}
