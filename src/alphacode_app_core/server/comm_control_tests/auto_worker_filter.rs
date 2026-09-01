// Auto-assignment must only target *drivable* workers. An independent,
// client-attached human session that happens to share the swarm is NOT driven by
// `spawn_assigned_task_run` (that only fires when the target has no live client),
// so auto-picking it would strand the task and stall `run_plan`. These tests pin
// the candidate filter so reuse of owned/headless workers keeps working while
// foreign client-attached sessions are excluded (leaving room for a fresh spawn).
//
// Included into the `comm_control::tests` module, so the parent's private
// `filter_swarm_agent_candidates` / `is_drivable_auto_worker` are in scope.

use super::{filter_swarm_agent_candidates, is_drivable_auto_worker};

fn agent_member(session_id: &str, swarm_id: &str) -> SwarmMember {
    member(session_id, swarm_id, "ready")
}

#[test]
fn headless_worker_is_drivable() {
    let mut m = agent_member("w", "s");
    m.is_headless = true;
    // No owner, but headless workers are always auto-driven in-process.
    assert!(is_drivable_auto_worker(&m, "coord"));
}

#[test]
fn worker_owned_by_requester_is_drivable_even_with_live_client() {
    let mut m = agent_member("w", "s");
    m.is_headless = false;
    m.report_back_to_session_id = Some("coord".to_string());
    // Simulate a live client attachment; ownership still makes it reusable.
    let (tx, _rx) = mpsc::unbounded_channel();
    m.event_txs.insert("conn-1".to_string(), tx);
    assert!(is_drivable_auto_worker(&m, "coord"));
}

#[test]
fn unowned_session_with_live_client_is_not_drivable() {
    let mut m = agent_member("human", "s");
    m.is_headless = false;
    m.report_back_to_session_id = None; // independent user session
    let (tx, _rx) = mpsc::unbounded_channel();
    m.event_txs.insert("conn-1".to_string(), tx);
    assert!(!is_drivable_auto_worker(&m, "coord"));
}

#[test]
fn unowned_session_is_not_auto_drivable() {
    // A foreign session that this run does not own is never auto-assignable, even
    // if it currently has no client attachment: it may be a stale "zombie" with no
    // live agent loop (the run_plan stall we are guarding against). Such sessions
    // require an explicit target_session.
    let mut m = agent_member("zombie", "s");
    m.is_headless = false;
    m.report_back_to_session_id = None;
    // No client attachment, yet still not auto-drivable because it is unowned.
    assert!(!is_drivable_auto_worker(&m, "coord"));
}

#[tokio::test]
async fn auto_candidate_filter_excludes_foreign_client_attached_session() {
    let swarm_id = "swarm-filter";
    let coord = "coord";
    let owned = "owned-worker";
    let headless = "headless-worker";
    let foreign = "foreign-human";

    let mut owned_member = agent_member(owned, swarm_id);
    owned_member.report_back_to_session_id = Some(coord.to_string());
    let (otx, _orx) = mpsc::unbounded_channel();
    owned_member.event_txs.insert("c".to_string(), otx); // owned + attached, still ok

    let mut headless_member = agent_member(headless, swarm_id);
    headless_member.is_headless = true;

    let mut foreign_member = agent_member(foreign, swarm_id);
    let (ftx, _frx) = mpsc::unbounded_channel();
    foreign_member.event_txs.insert("c".to_string(), ftx); // unowned + attached -> excluded

    let members: HashMap<String, SwarmMember> = HashMap::from([
        (coord.to_string(), {
            let mut m = agent_member(coord, swarm_id);
            m.role = "coordinator".to_string();
            m
        }),
        (owned.to_string(), owned_member),
        (headless.to_string(), headless_member),
        (foreign.to_string(), foreign_member),
    ]);

    let candidates = filter_swarm_agent_candidates(&members, coord, swarm_id);
    let ids: std::collections::HashSet<&str> =
        candidates.iter().map(|m| m.session_id.as_str()).collect();
    assert!(ids.contains(owned), "owned worker should be eligible");
    assert!(ids.contains(headless), "headless worker should be eligible");
    assert!(
        !ids.contains(foreign),
        "foreign client-attached session must be excluded from auto-assign"
    );
}

// ----- composite-aware turn completion -----

use super::turn_end_should_auto_complete;

#[test]
fn atomic_turn_auto_completes() {
    // A plain running atomic node that the worker just ran should be
    // auto-marked done. `running_stale` (revived after a reload) counts too.
    assert!(turn_end_should_auto_complete("running", false));
    assert!(turn_end_should_auto_complete("running_stale", false));
}

#[test]
fn requeued_node_does_not_auto_complete() {
    // A node that is `queued` at turn end was re-queued mid-turn by someone
    // else: a gate that called inject_gap (re-queued behind its gap nodes), or
    // a reassign/requeue. Force-closing it would bypass gate artifact
    // validation and strand the injected gap nodes.
    assert!(!turn_end_should_auto_complete("queued", false));
}

#[test]
fn expanded_composite_turn_does_not_auto_complete() {
    // The worker decomposed the node; it must stay open to synthesize later, even
    // though its own turn ended. This is the bug that stranded composite subtrees.
    assert!(!turn_end_should_auto_complete("running", true));
    assert!(!turn_end_should_auto_complete("queued", true));
}

#[test]
fn already_terminal_turn_is_not_reclosed() {
    // If the worker already completed/failed the node itself, the turn end must
    // not stomp that status.
    assert!(!turn_end_should_auto_complete("completed", false));
    assert!(!turn_end_should_auto_complete("done", false));
    assert!(!turn_end_should_auto_complete("failed", false));
    assert!(!turn_end_should_auto_complete("stopped", false));
}

// ----- deep mode: artifact-or-nothing at turn end -----

use super::{TurnEndDisposition, turn_end_disposition};

#[test]
fn deep_turn_without_artifact_requeues_then_fails() {
    // Deep mode never auto-completes: the typed-artifact contract is only real
    // if there is no path to "done" that skips complete_node. First offense is
    // a requeue to a fresh worker; a repeat fails the node loudly.
    assert_eq!(
        turn_end_disposition(true, "running", false, 0),
        TurnEndDisposition::RequeueNoArtifact
    );
    assert_eq!(
        turn_end_disposition(true, "running_stale", false, 0),
        TurnEndDisposition::RequeueNoArtifact
    );
    assert_eq!(
        turn_end_disposition(true, "running", false, 1),
        TurnEndDisposition::FailNoArtifact
    );
    // A re-woken composite synthesis that never synthesized is equally guilty.
    assert_eq!(
        turn_end_disposition(true, "running", true, 0),
        TurnEndDisposition::RequeueNoArtifact
    );
}

#[test]
fn deep_turn_leaves_non_running_nodes_alone() {
    // Terminal states were set by complete_node/fail; queued means the node was
    // re-queued mid-turn (expand_node or inject_gap). None are this turn's
    // responsibility.
    for status in ["queued", "completed", "done", "failed", "stopped"] {
        assert_eq!(
            turn_end_disposition(true, status, false, 0),
            TurnEndDisposition::LeaveAlone,
            "status {status}"
        );
    }
}

#[test]
fn light_turn_disposition_matches_legacy_auto_complete() {
    assert_eq!(
        turn_end_disposition(false, "running", false, 0),
        TurnEndDisposition::AutoComplete
    );
    assert_eq!(
        turn_end_disposition(false, "running", true, 0),
        TurnEndDisposition::LeaveAlone
    );
    assert_eq!(
        turn_end_disposition(false, "queued", false, 0),
        TurnEndDisposition::LeaveAlone
    );
}

// ----- composite synthesis assignment content -----

use super::composite_synthesis_content;

#[test]
fn composite_synthesis_content_injects_complete_node_instruction() {
    let out = composite_synthesis_content("root", "explore the thing", true);
    assert!(out.contains("Synthesis turn for composite node 'root'"));
    assert!(out.contains("complete_node"));
    assert!(out.contains("Do NOT"));
    // The original brief is preserved for context.
    assert!(out.contains("explore the thing"));
}

#[test]
fn non_composite_content_is_verbatim() {
    let out = composite_synthesis_content("leaf", "just do this", false);
    assert_eq!(out, "just do this");
}
