//! Track checkpoint cadence for CTF progress reminders.
use serde_json::Value;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

const MAX_PROBES: u32 = 12;
/// CTF completion has distinct evidence levels. `PLATFORM_ACCEPTED` is never
/// inferred from a flag-like string or a successful HTTP response.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum SolveState {
    Candidate,
    LocallyVerified,
    PlatformAccepted,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SolveRecord {
    pub state: SolveState,
    pub evidence: String,
    pub hint_used: bool,
}

pub(crate) fn advance_solve(
    record: &mut SolveRecord,
    next: SolveState,
    evidence: &str,
    platform_confirmation: bool,
) -> Result<(), &'static str> {
    let valid = matches!(
        (record.state, next),
        (SolveState::Candidate, SolveState::Candidate)
            | (SolveState::Candidate, SolveState::LocallyVerified)
            | (SolveState::LocallyVerified, SolveState::LocallyVerified)
            | (SolveState::LocallyVerified, SolveState::PlatformAccepted)
            | (SolveState::PlatformAccepted, SolveState::PlatformAccepted)
    );
    if !valid {
        return Err("solve state cannot move backwards");
    }
    if evidence.trim().is_empty() {
        return Err("solve state requires evidence");
    }
    if next == SolveState::PlatformAccepted && !platform_confirmation {
        return Err("PLATFORM_ACCEPTED requires an explicit platform confirmation");
    }
    record.state = next;
    record.evidence = evidence.to_owned();
    Ok(())
}

static PROGRESS: LazyLock<Mutex<HashMap<String, (u64, u32)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// On-disk filename for the per-session CTF solve record.
const SOLVE_RECORD_FILE: &str = "solve.json";

fn solve_record_path(session: &str, directory: &Path) -> std::path::PathBuf {
    directory
        .join(super::research_ledger_relative_dir(session))
        .join(SOLVE_RECORD_FILE)
}

/// Load the recorded solve state for a session, if one was ever written.
///
/// A missing or unreadable record is reported as `None` rather than guessed at:
/// "no evidence recorded" must never be presented as "solved".
pub(crate) fn load_solve_record(session: &str, directory: &Path) -> Option<SolveRecord> {
    let path = solve_record_path(session, directory);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_solve_record(session: &str, directory: &Path, record: &SolveRecord) -> std::io::Result<()> {
    let path = solve_record_path(session, directory);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write then rename so a crash mid-write cannot leave a half-parsed record
    // that would be silently read back as a different claim.
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_string_pretty(record)?)?;
    std::fs::rename(&temp, &path)
}

/// Record a flag-shaped candidate the solver observed.
///
/// This only ever establishes `Candidate`. A regex match proves the *appearance*
/// of a flag, not that a platform accepted it, so the state is deliberately not
/// advanced further here.
pub(crate) fn record_candidate(
    session: &str,
    directory: &Path,
    evidence: &str,
    hint_used: bool,
) -> Result<SolveState, String> {
    let mut record = load_solve_record(session, directory).unwrap_or(SolveRecord {
        state: SolveState::Candidate,
        evidence: String::new(),
        hint_used,
    });
    // A later run must not demote a previously established higher state.
    if record.state == SolveState::Candidate {
        advance_solve(&mut record, SolveState::Candidate, evidence, false)
            .map_err(|error| error.to_string())?;
        record.hint_used |= hint_used;
        save_solve_record(session, directory, &record).map_err(|error| error.to_string())?;
    }
    Ok(record.state)
}

/// Human-readable completion level, used in reminders so the model cannot
/// report a candidate match as a platform-accepted solve.
pub(crate) fn solve_state_summary(session: &str, directory: &Path) -> &'static str {
    match load_solve_record(session, directory).map(|record| record.state) {
        Some(SolveState::PlatformAccepted) => {
            "Solve state: PLATFORM_ACCEPTED (platform confirmed the flag)."
        }
        Some(SolveState::LocallyVerified) => {
            "Solve state: LOCALLY_VERIFIED (verified locally; platform acceptance is not \
             confirmed, so do not report the challenge as solved)."
        }
        Some(SolveState::Candidate) => {
            "Solve state: CANDIDATE (a flag-shaped string was observed, not verified). Do not \
             report this challenge as solved."
        }
        None => "Solve state: none recorded. No flag has been verified for this challenge.",
    }
}

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
    fn solve_claim_requires_evidence_and_platform_confirmation() {
        let mut record = SolveRecord {
            state: SolveState::Candidate,
            evidence: "initial candidate".into(),
            hint_used: false,
        };
        assert!(
            advance_solve(
                &mut record,
                SolveState::LocallyVerified,
                "local test passed",
                false
            )
            .is_ok()
        );
        assert!(
            advance_solve(
                &mut record,
                SolveState::PlatformAccepted,
                "flag string matched",
                false
            )
            .is_err()
        );
        assert!(
            advance_solve(
                &mut record,
                SolveState::PlatformAccepted,
                "judge accepted submission",
                true
            )
            .is_ok()
        );
        assert_eq!(record.state, SolveState::PlatformAccepted);
    }
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

    /// A flag-shaped regex match must never be persisted as an accepted solve.
    /// This is the overclaim the state machine exists to prevent.
    #[test]
    fn candidate_record_never_reports_platform_acceptance() {
        let root = tempfile::tempdir().unwrap();
        let session = "session/candidate";

        assert!(
            load_solve_record(session, root.path()).is_none(),
            "an unrecorded challenge must not report a solve state"
        );
        assert!(solve_state_summary(session, root.path()).contains("none recorded"));

        let state = record_candidate(session, root.path(), "regex match for flag{x}", false)
            .expect("recording a candidate must succeed");
        assert_eq!(
            state,
            SolveState::Candidate,
            "a regex match establishes a candidate, not a verified solve"
        );

        let summary = solve_state_summary(session, root.path());
        assert!(summary.contains("CANDIDATE"), "{summary}");
        assert!(
            summary.contains("Do not report this challenge as solved"),
            "the summary must forbid the overclaim: {summary}"
        );

        // A second observation must not demote or promote the recorded state.
        let again = record_candidate(session, root.path(), "second regex match", false).unwrap();
        assert_eq!(again, SolveState::Candidate);
        let persisted = load_solve_record(session, root.path()).expect("record persists");
        assert_eq!(
            persisted.state,
            SolveState::Candidate,
            "repeated candidate observations must never advance the state"
        );
        assert!(
            !persisted.evidence.trim().is_empty(),
            "a persisted record must keep the evidence that justified it"
        );
    }

    /// A previously verified solve must survive a later candidate-only pass.
    /// Demoting it would invite the model to redo finished work.
    #[test]
    fn a_verified_record_is_not_overwritten_by_a_weaker_candidate() {
        let root = tempfile::tempdir().unwrap();
        let session = "session/verified";

        let mut record = SolveRecord {
            state: SolveState::Candidate,
            evidence: "initial".into(),
            hint_used: false,
        };
        advance_solve(
            &mut record,
            SolveState::LocallyVerified,
            "local check passed",
            false,
        )
        .expect("candidate -> locally verified is allowed");
        let path = solve_record_path(session, root.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&record).unwrap()).unwrap();

        let observed = record_candidate(session, root.path(), "another regex hit", false).unwrap();
        assert_eq!(
            observed,
            SolveState::LocallyVerified,
            "a weaker candidate observation must not demote a verified record"
        );
        assert!(solve_state_summary(session, root.path()).contains("LOCALLY_VERIFIED"));
    }
}
