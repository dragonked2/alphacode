use anyhow::Result;
use chrono::Utc;
use serde::Deserialize;
use std::io::IsTerminal;
use std::path::PathBuf;

pub async fn run_restart_save_command(auto_restore: bool) -> Result<()> {
    let mut snapshot = if let Some(snapshot) = capture_connected_restart_snapshot().await? {
        snapshot
    } else {
        crate::restart_snapshot::save_current_snapshot()?
    };
    snapshot.auto_restore_on_next_start = auto_restore;
    crate::restart_snapshot::write_snapshot(&snapshot)?;
    let path = crate::restart_snapshot::snapshot_path()?;

    if snapshot.sessions.is_empty() {
        println!("Saved empty reboot snapshot to {}", path.display());
        if auto_restore {
            println!("Automatic restore is armed for the next plain `alphacode` launch.");
        }
        println!("\nNo active alphacode windows were detected.");
        return Ok(());
    }

    println!(
        "Saved reboot snapshot with {} session(s) to {}\n",
        snapshot.sessions.len(),
        path.display()
    );
    for session in &snapshot.sessions {
        let suffix = if session.is_selfdev {
            " [self-dev]"
        } else {
            ""
        };
        println!(
            "- {} ({}){}",
            session.display_name, session.session_id, suffix
        );
    }
    if auto_restore {
        println!("\nAutomatic restore is armed for the next plain `alphacode` launch.");
    }
    println!("\nAfter reboot, restore them with:\n  alphacode restart restore");

    Ok(())
}

pub fn run_restart_status_command() -> Result<()> {
    let path = crate::restart_snapshot::snapshot_path()?;
    let snapshot = match crate::restart_snapshot::load_snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            println!("No reboot snapshot saved.\n\nCreate one with:\n  alphacode restart save");
            return Ok(());
        }
    };

    println!(
        "Reboot snapshot: {}\nCreated: {}\nSessions: {}\nAuto-restore on next plain startup: {}\n",
        path.display(),
        snapshot.created_at,
        snapshot.sessions.len(),
        if snapshot.auto_restore_on_next_start {
            "armed"
        } else {
            "off"
        }
    );
    for session in &snapshot.sessions {
        let suffix = if session.is_selfdev {
            " [self-dev]"
        } else {
            ""
        };
        println!(
            "- {} ({}){}",
            session.display_name, session.session_id, suffix
        );
    }

    Ok(())
}

pub async fn maybe_run_pending_restart_restore_on_startup() -> Result<bool> {
    let snapshot = match crate::restart_snapshot::load_snapshot() {
        Ok(snapshot) => snapshot,
        // Do not synthesize an auto-restore snapshot from crashed sessions here.
        // A crashed session should remain crashed until the user explicitly resumes
        // or restores it, rather than being respawned by the next default startup.
        Err(_) => return Ok(false),
    };

    if snapshot.auto_restore_on_next_start {
        let _ = crate::restart_snapshot::set_auto_restore_on_next_start(false);
        println!(
            "Found a reboot snapshot with auto-restore enabled. Restoring {} alphacode window(s)...\n",
            snapshot.sessions.len()
        );
        run_restart_restore_command()?;
        return Ok(true);
    }

    if std::io::stdin().is_terminal() || std::io::stderr().is_terminal() {
        println!("Saved reboot snapshot detected. Restore it with:\n  alphacode restart restore\n");
    }

    Ok(false)
}

pub fn run_restart_clear_command() -> Result<()> {
    if crate::restart_snapshot::clear_snapshot()? {
        println!("Cleared reboot snapshot.");
    } else {
        println!("No reboot snapshot was saved.");
    }
    Ok(())
}

pub fn run_restart_restore_command() -> Result<()> {
    let exe = current_restart_restore_exe()?;
    let result = match crate::restart_snapshot::restore_snapshot(&exe) {
        Ok(result) => result,
        Err(error) => {
            let path = crate::restart_snapshot::snapshot_path()?;
            return Err(anyhow::anyhow!(
                "Failed to restore reboot snapshot at {}: {}",
                path.display(),
                error
            ));
        }
    };

    if result.snapshot.sessions.is_empty() {
        println!("Saved reboot snapshot is empty. Nothing to restore.");
        let _ = crate::restart_snapshot::clear_snapshot();
        return Ok(());
    }

    let launched = result
        .outcomes
        .iter()
        .filter(|outcome| outcome.launched)
        .count();
    let fallback = result.outcomes.len().saturating_sub(launched);

    if launched > 0 {
        println!("Restored {} alphacode window(s).", launched);
    }

    if fallback > 0 {
        println!(
            "\n{} session(s) could not be opened automatically. Run these commands manually:\n",
            fallback
        );
        for outcome in result.outcomes.iter().filter(|outcome| !outcome.launched) {
            println!("# {}", outcome.session.display_name);
            println!("{}", outcome.command);
        }
        println!(
            "\nThe reboot snapshot was kept so you can try `alphacode restart restore` again later."
        );
        return Ok(());
    }

    let _ = crate::restart_snapshot::clear_snapshot();
    println!("Cleared reboot snapshot after successful restore.");
    Ok(())
}

fn current_restart_restore_exe() -> Result<PathBuf> {
    crate::build::client_update_candidate(false)
        .map(|(path, _)| path)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| anyhow::anyhow!("Could not determine alphacode executable for restore"))
}

#[derive(Debug, Deserialize)]
struct ConnectedRestartSessionRow {
    session_id: String,
    #[serde(default)]
    working_dir: Option<String>,
}

async fn capture_connected_restart_snapshot()
-> Result<Option<crate::restart_snapshot::RestartSnapshot>> {
    let mut client = match crate::alphacode_app_core::server::Client::connect_debug().await {
        Ok(client) => client,
        Err(_) => return Ok(None),
    };

    let request_id = client.debug_command("sessions", None).await?;
    let response = loop {
        match client.read_event().await? {
            crate::protocol::ServerEvent::DebugResponse { id, ok, output } if id == request_id => {
                if !ok {
                    anyhow::bail!(output);
                }
                break output;
            }
            crate::protocol::ServerEvent::Ack { id } if id == request_id => {}
            crate::protocol::ServerEvent::Done { id } if id == request_id => {}
            crate::protocol::ServerEvent::Error { id, message, .. } if id == request_id => {
                anyhow::bail!(message);
            }
            _ => {}
        }
    };

    let rows: Vec<ConnectedRestartSessionRow> = serde_json::from_str(&response)?;
    if rows.is_empty() {
        return Ok(Some(crate::restart_snapshot::RestartSnapshot {
            version: 1,
            created_at: Utc::now(),
            auto_restore_on_next_start: false,
            sessions: Vec::new(),
        }));
    }

    let mut seen = std::collections::HashSet::new();
    let mut sessions = Vec::new();
    for row in rows {
        if !seen.insert(row.session_id.clone()) {
            continue;
        }
        let Ok(mut session) = crate::session::Session::load(&row.session_id) else {
            continue;
        };
        if session.detect_crash() {
            let _ = session.save();
            continue;
        }
        sessions.push(crate::restart_snapshot::RestartSnapshotSession {
            session_id: session.id.clone(),
            display_name: session.display_name().to_string(),
            working_dir: session.working_dir.clone().or(row.working_dir),
            is_selfdev: session.is_canary,
        });
    }

    sessions.sort_by(|a, b| {
        a.display_name
            .cmp(&b.display_name)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    Ok(Some(crate::restart_snapshot::RestartSnapshot {
        version: 1,
        created_at: Utc::now(),
        auto_restore_on_next_start: false,
        sessions,
    }))
}
