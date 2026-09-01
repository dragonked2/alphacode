fn awaited_member(session_id: &str, done: bool) -> AwaitedMemberStatus {
    AwaitedMemberStatus {
        session_id: session_id.to_string(),
        friendly_name: Some(session_id.to_string()),
        status: if done { "completed" } else { "running" }.to_string(),
        done,
        completion_report: None,
    }
}

#[test]
fn awaited_members_header_all_done() {
    let members = vec![awaited_member("fox", true), awaited_member("wolf", true)];
    let output = format_comm_awaited_members_with_reports(
        true,
        "All 2 members are done: fox, wolf",
        &members,
        &std::collections::HashMap::new(),
    );
    assert!(
        output.starts_with("All members done."),
        "expected all-done header, got: {output}"
    );
}

#[test]
fn awaited_members_header_any_mode_partial_match() {
    let members = vec![awaited_member("fox", true), awaited_member("wolf", false)];
    let output = format_comm_awaited_members_with_reports(
        true,
        "Matched 1 member: fox",
        &members,
        &std::collections::HashMap::new(),
    );
    assert!(
        output.starts_with("Await satisfied."),
        "any-mode partial match must not claim all members are done, got: {output}"
    );
    assert!(!output.starts_with("All members done."));
}

#[test]
fn awaited_members_header_incomplete() {
    let members = vec![awaited_member("fox", false)];
    let output = format_comm_awaited_members_with_reports(
        false,
        "Timed out. Still waiting on: fox (running)",
        &members,
        &std::collections::HashMap::new(),
    );
    assert!(
        output.starts_with("Await incomplete."),
        "expected incomplete header, got: {output}"
    );
}
