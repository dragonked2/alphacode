//! Pure diffing of swarm member snapshots into user-facing status notices.
//!
//! The server streams full `SwarmStatus` snapshots; the strip renders them,
//! but lifecycle transitions (an agent finishing, failing, or blocking) used
//! to pass silently. This module compares the previous snapshot with the next
//! one and produces a compact one-line notice in the same spirit as the
//! "Swarm plan synced" notice from [`super::swarm_plan_core`].

use crate::alphacode_tui::protocol::SwarmMemberStatus;
use crate::alphacode_tui_render::swarm_gallery::is_active_status;

/// How many member names to list per transition category before collapsing
/// the rest into "+N".
const MAX_NAMES_PER_CATEGORY: usize = 3;

/// Lifecycle buckets worth announcing when a member newly enters them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Transition {
    Done,
    Failed,
    Blocked,
    Stopped,
}

impl Transition {
    fn verb(self) -> &'static str {
        match self {
            Transition::Done => "done",
            Transition::Failed => "failed",
            Transition::Blocked => "blocked",
            Transition::Stopped => "stopped",
        }
    }
}

fn member_label(member: &SwarmMemberStatus) -> String {
    member
        .friendly_name
        .clone()
        .unwrap_or_else(|| member.session_id.chars().take(8).collect())
}

/// Classify a status change into an announceable transition, if any.
///
/// Transitions into active states and startup transitions (`spawned` →
/// `ready`) are intentionally silent: spawning is user-initiated and already
/// visible, and active churn (running ↔ thinking) is animation-level noise.
fn classify(prev_status: &str, next_status: &str) -> Option<Transition> {
    if prev_status == next_status {
        return None;
    }
    match next_status {
        "completed" | "done" => Some(Transition::Done),
        // `ready` is the default completion-report status, but it is also the
        // idle state a fresh agent enters on startup. Only count it as "done"
        // when the agent was actually working (or stuck) before.
        "ready" if is_active_status(prev_status) || prev_status == "blocked" => {
            Some(Transition::Done)
        }
        "failed" | "crashed" => Some(Transition::Failed),
        "blocked" | "waiting_network" => Some(Transition::Blocked),
        "stopped" => Some(Transition::Stopped),
        _ => None,
    }
}

fn format_names(mut names: Vec<String>) -> String {
    names.sort();
    let hidden = names.len().saturating_sub(MAX_NAMES_PER_CATEGORY);
    names.truncate(MAX_NAMES_PER_CATEGORY);
    let mut out = names.join(", ");
    if hidden > 0 {
        out.push_str(&format!(" +{hidden}"));
    }
    out
}

/// Diff two swarm snapshots and build a status notice describing member
/// lifecycle transitions, e.g. `🐝 bat done · 2/7 active` or
/// `🐝 crab failed · hen blocked · all 7 finished`. Returns `None` when
/// nothing announceable changed.
pub(in crate::alphacode_tui::tui::app) fn swarm_status_transition_notice(
    prev: &[SwarmMemberStatus],
    next: &[SwarmMemberStatus],
) -> Option<String> {
    if prev.is_empty() || next.is_empty() {
        return None;
    }
    let prev_status: std::collections::HashMap<&str, &str> = prev
        .iter()
        .map(|m| (m.session_id.as_str(), m.status.as_str()))
        .collect();

    let mut buckets: Vec<(Transition, Vec<String>)> = Vec::new();
    for member in next {
        let Some(prev_status) = prev_status.get(member.session_id.as_str()) else {
            // New member: spawning is user/agent initiated and already visible.
            continue;
        };
        if let Some(transition) = classify(prev_status, &member.status) {
            match buckets.iter_mut().find(|(t, _)| *t == transition) {
                Some((_, names)) => names.push(member_label(member)),
                None => buckets.push((transition, vec![member_label(member)])),
            }
        }
    }
    if buckets.is_empty() {
        return None;
    }
    buckets.sort_by_key(|(t, _)| *t);

    let mut segments: Vec<String> = buckets
        .into_iter()
        .map(|(transition, names)| format!("{} {}", format_names(names), transition.verb()))
        .collect();

    // Tail: the same "M/N active" tally the strip shows, or a wrap-up line
    // when nothing is working anymore.
    let active = next.iter().filter(|m| is_active_status(&m.status)).count();
    segments.push(if active > 0 {
        format!("{active}/{} active", next.len())
    } else {
        format!("all {} finished", next.len())
    });

    Some(format!("🐝 {}", segments.join(" · ")))
}

#[cfg(test)]
mod tests {
    use super::swarm_status_transition_notice;
    use crate::alphacode_tui::protocol::SwarmMemberStatus;

    fn member(id: &str, status: &str) -> SwarmMemberStatus {
        SwarmMemberStatus {
            session_id: id.to_string(),
            friendly_name: Some(id.to_string()),
            status: status.to_string(),
            detail: None,
            task_label: None,
            role: None,
            is_headless: Some(true),
            live_attachments: None,
            status_age_secs: Some(1),
            output_tail: None,
            report_back_to_session_id: None,
            todo_progress: None,
            todo_items: Vec::new(),
            runtime: crate::protocol::SwarmMemberRuntime::default(),
        }
    }

    #[test]
    fn agent_completing_is_announced_with_active_tally() {
        let prev = vec![member("ant", "running"), member("bat", "running")];
        let next = vec![member("ant", "completed"), member("bat", "running")];
        assert_eq!(
            swarm_status_transition_notice(&prev, &next).as_deref(),
            Some("🐝 ant done · 1/2 active")
        );
    }

    #[test]
    fn ready_after_working_counts_as_done() {
        let prev = vec![member("ant", "running"), member("bat", "thinking")];
        let next = vec![member("ant", "ready"), member("bat", "thinking")];
        assert_eq!(
            swarm_status_transition_notice(&prev, &next).as_deref(),
            Some("🐝 ant done · 1/2 active")
        );
    }

    #[test]
    fn ready_on_startup_is_silent() {
        let prev = vec![member("ant", "spawned"), member("bat", "running")];
        let next = vec![member("ant", "ready"), member("bat", "running")];
        assert_eq!(swarm_status_transition_notice(&prev, &next), None);
    }

    #[test]
    fn failure_and_block_are_announced_together() {
        let prev = vec![
            member("ant", "running"),
            member("bat", "running"),
            member("crab", "running"),
        ];
        let next = vec![
            member("ant", "failed"),
            member("bat", "blocked"),
            member("crab", "running"),
        ];
        assert_eq!(
            swarm_status_transition_notice(&prev, &next).as_deref(),
            Some("🐝 ant failed · bat blocked · 1/3 active")
        );
    }

    #[test]
    fn last_agent_finishing_reports_all_finished() {
        let prev = vec![member("ant", "completed"), member("bat", "running")];
        let next = vec![member("ant", "completed"), member("bat", "completed")];
        assert_eq!(
            swarm_status_transition_notice(&prev, &next).as_deref(),
            Some("🐝 bat done · all 2 finished")
        );
    }

    #[test]
    fn unchanged_snapshot_is_silent() {
        let prev = vec![member("ant", "running"), member("bat", "completed")];
        assert_eq!(swarm_status_transition_notice(&prev, &prev.clone()), None);
    }

    #[test]
    fn active_churn_and_new_members_are_silent() {
        let prev = vec![member("ant", "running")];
        let next = vec![
            member("ant", "thinking"), // running -> thinking: animation noise
            member("bat", "spawned"),  // new member: spawn already visible
        ];
        assert_eq!(swarm_status_transition_notice(&prev, &next), None);
    }

    #[test]
    fn first_snapshot_is_silent() {
        let next = vec![member("ant", "completed")];
        assert_eq!(swarm_status_transition_notice(&[], &next), None);
    }

    #[test]
    fn many_names_collapse_into_more_count() {
        let prev: Vec<_> = ["ant", "bat", "crab", "dove", "elk"]
            .iter()
            .map(|id| member(id, "running"))
            .collect();
        let next: Vec<_> = ["ant", "bat", "crab", "dove", "elk"]
            .iter()
            .map(|id| member(id, "completed"))
            .collect();
        assert_eq!(
            swarm_status_transition_notice(&prev, &next).as_deref(),
            Some("🐝 ant, bat, crab +2 done · all 5 finished")
        );
    }

    #[test]
    fn unnamed_member_falls_back_to_session_id_prefix() {
        let mut prev_member = member("session-long-identifier", "running");
        prev_member.friendly_name = None;
        let mut next_member = member("session-long-identifier", "completed");
        next_member.friendly_name = None;
        assert_eq!(
            swarm_status_transition_notice(&[prev_member], &[next_member]).as_deref(),
            Some("🐝 session- done · all 1 finished")
        );
    }
}
