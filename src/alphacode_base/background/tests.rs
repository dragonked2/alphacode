use super::*;
use crate::bus::{BackgroundTaskProgressKind, BackgroundTaskProgressSource, BusEvent};
use anyhow::anyhow;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn adopted_process_failure_preserves_exit_code_and_output() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    let handle = tokio::spawn(async {
        Ok(
            crate::alphacode_tool_types::ToolOutput::new("failed output")
                .with_metadata(serde_json::json!({"process_exit_code": 7})),
        )
    });
    let info = manager.adopt("bash", "test-adopted-failure", handle).await;
    for _ in 0..100 {
        let status = manager
            .status(&info.task_id)
            .await
            .ok_or_else(|| anyhow!("missing status"))?;
        if status.status != BackgroundTaskStatus::Running {
            assert_eq!(status.status, BackgroundTaskStatus::Failed);
            assert_eq!(status.exit_code, Some(7));
            assert_eq!(
                tokio::fs::read_to_string(&info.output_file).await?,
                "failed output"
            );
            return Ok(());
        }
        sleep(Duration::from_millis(10)).await;
    }
    Err(anyhow!("adopted task did not finish"))
}

#[tokio::test]
async fn spawn_with_notify_emits_started_ui_activity() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    let mut bus_rx = Bus::global().subscribe();

    let info = manager
        .spawn_with_notify(
            "bash",
            Some("checks".to_string()),
            "session-started",
            true,
            false,
            |_output_path| async move {
                sleep(Duration::from_millis(10)).await;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;

    for _ in 0..20 {
        let event = tokio::time::timeout(Duration::from_millis(200), bus_rx.recv())
            .await
            .map_err(|err| anyhow!("timed out waiting for UI activity event: {err}"))?
            .map_err(|err| anyhow!("bus should stay open: {err}"))?;
        if let BusEvent::UiActivity(activity) = event
            && activity.session_id.as_deref() == Some("session-started")
            && activity.message.contains(&info.task_id)
        {
            assert_eq!(activity.kind, crate::bus::UiActivityKind::Background);
            assert!(activity.message.contains("Background task started"));
            assert!(activity.message.contains("checks"));
            assert_eq!(
                activity.status_notice.as_deref(),
                Some("Background task started · checks")
            );
            return Ok(());
        }
    }

    Err(anyhow!(
        "started UI activity event for task {} not received",
        info.task_id
    ))
}

#[tokio::test]
async fn update_delivery_applies_to_running_task_completion() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let info = manager
        .spawn_with_notify(
            "bash",
            None,
            "session-test",
            false,
            false,
            |output_path| async move {
                sleep(Duration::from_millis(25)).await;
                tokio::fs::write(&output_path, "hello").await?;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;

    let updated = manager
        .update_delivery(&info.task_id, true, true)
        .await
        .map_err(|err| anyhow!("update delivery should succeed: {err}"))?
        .ok_or_else(|| anyhow!("task should exist"))?;
    assert!(updated.notify);
    assert!(updated.wake);

    for _ in 0..40 {
        let status = manager
            .status(&info.task_id)
            .await
            .ok_or_else(|| anyhow!("status should exist"))?;
        if status.status != BackgroundTaskStatus::Running {
            assert!(status.notify);
            assert!(status.wake);
            assert_eq!(status.status, BackgroundTaskStatus::Completed);
            return Ok(());
        }
        sleep(Duration::from_millis(10)).await;
    }

    Err(anyhow!("background task did not complete in time"))
}

#[tokio::test]
async fn update_progress_persists_status_and_emits_bus_event() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let info = manager
        .spawn_with_notify(
            "bash",
            None,
            "session-progress",
            false,
            false,
            |_output_path| async move {
                sleep(Duration::from_millis(50)).await;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;

    let progress = BackgroundTaskProgress {
        kind: BackgroundTaskProgressKind::Determinate,
        percent: Some(42.0),
        message: Some("Running checks".to_string()),
        current: Some(21),
        total: Some(50),
        unit: Some("tests".to_string()),
        eta_seconds: Some(8),
        updated_at: Utc::now().to_rfc3339(),
        source: BackgroundTaskProgressSource::Reported,
    };

    let mut bus_rx = Bus::global().subscribe();
    let updated = manager
        .update_progress(&info.task_id, progress.clone())
        .await
        .map_err(|err| anyhow!("update progress should succeed: {err}"))?
        .ok_or_else(|| anyhow!("task should exist"))?;

    assert_eq!(updated.progress, Some(progress.clone().normalize()));

    for _ in 0..20 {
        let event = tokio::time::timeout(Duration::from_millis(200), bus_rx.recv())
            .await
            .map_err(|err| anyhow!("timed out waiting for progress event: {err}"))?
            .map_err(|err| anyhow!("bus should stay open: {err}"))?;
        if let BusEvent::BackgroundTaskProgress(event) = event
            && event.task_id == info.task_id
        {
            assert_eq!(event.session_id, "session-progress");
            assert_eq!(event.progress, progress.normalize());
            return Ok(());
        }
    }

    Err(anyhow!(
        "progress event for task {} not received",
        info.task_id
    ))
}

#[tokio::test]
async fn wait_returns_when_task_finishes() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let info = manager
        .spawn_with_notify(
            "bash",
            None,
            "session-wait-finish",
            false,
            false,
            |output_path| async move {
                sleep(Duration::from_millis(25)).await;
                tokio::fs::write(&output_path, "done").await?;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;

    let wait_result = manager
        .wait(&info.task_id, Duration::from_secs(2), true)
        .await
        .ok_or_else(|| anyhow!("task should exist"))?;

    assert_eq!(wait_result.reason, BackgroundTaskWaitReason::Finished);
    assert_eq!(wait_result.task.status, BackgroundTaskStatus::Completed);
    assert_eq!(wait_result.task.exit_code, Some(0));
    Ok(())
}

#[tokio::test]
async fn wait_returns_on_progress_checkpoint() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let info = manager
        .spawn_with_notify(
            "bash",
            None,
            "session-wait-progress",
            false,
            false,
            |_output_path| async move {
                sleep(Duration::from_secs(2)).await;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;

    let progress = BackgroundTaskProgress {
        kind: BackgroundTaskProgressKind::Determinate,
        percent: Some(25.0),
        message: Some("checkpoint".to_string()),
        current: Some(1),
        total: Some(4),
        unit: Some("steps".to_string()),
        eta_seconds: Some(3),
        updated_at: Utc::now().to_rfc3339(),
        source: BackgroundTaskProgressSource::Reported,
    };

    let waiter = manager.wait(&info.task_id, Duration::from_secs(2), true);
    let updater = async {
        sleep(Duration::from_millis(25)).await;
        manager
            .update_progress(&info.task_id, progress.clone())
            .await
            .map_err(|err| anyhow!("progress update should succeed: {err}"))?
            .ok_or_else(|| anyhow!("task should exist"))?;
        Result::<()>::Ok(())
    };
    let (wait_result, updater_result) = tokio::join!(waiter, updater);
    updater_result?;
    let wait_result = wait_result.ok_or_else(|| anyhow!("task should exist"))?;

    assert_eq!(wait_result.reason, BackgroundTaskWaitReason::Progress);
    assert_eq!(wait_result.task.status, BackgroundTaskStatus::Running);
    assert_eq!(wait_result.task.progress, Some(progress.normalize()));
    assert!(wait_result.progress_event.is_some());
    Ok(())
}

#[tokio::test]
async fn wait_returns_on_timeout() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let info = manager
        .spawn_with_notify(
            "bash",
            None,
            "session-wait-timeout",
            false,
            false,
            |_output_path| async move {
                sleep(Duration::from_millis(250)).await;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;

    let wait_result = manager
        .wait(&info.task_id, Duration::from_millis(25), true)
        .await
        .ok_or_else(|| anyhow!("task should exist"))?;

    assert_eq!(wait_result.reason, BackgroundTaskWaitReason::Timeout);
    assert_eq!(wait_result.task.status, BackgroundTaskStatus::Running);
    Ok(())
}

/// Spawn a process that stays alive long enough for the test to observe it as
/// a live foreign PID. `sleep 30` on Unix; `ping` loop on Windows (which has no
/// `sleep` command on older systems and where PID 1 is never alive).
fn spawn_sleep_process() -> Result<std::process::Child> {
    #[cfg(unix)]
    {
        std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .map_err(|err| anyhow!("spawn sleep: {err}"))
    }
    #[cfg(windows)]
    {
        std::process::Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|err| anyhow!("spawn ping: {err}"))
    }
}

fn running_status_fixture(task_id: &str, session_id: &str) -> TaskStatusFile {
    TaskStatusFile {
        task_id: task_id.to_string(),
        tool_name: "swarm".to_string(),
        display_name: None,
        session_id: session_id.to_string(),
        status: BackgroundTaskStatus::Running,
        exit_code: None,
        error: None,
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
        duration_secs: None,
        pid: None,
        owner_pid: None,
        owner_instance: None,
        detached: false,
        notify: false,
        wake: false,
        progress: None,
        event_history: Vec::new(),
    }
}

async fn write_status_fixture(manager: &BackgroundTaskManager, status: &TaskStatusFile) {
    let path = manager.status_path_for(&status.task_id);
    let json = serde_json::to_string_pretty(status).expect("serialize status fixture");
    tokio::fs::write(&path, json).await.expect("write fixture");
}

#[tokio::test]
async fn tasks_map_prunes_entry_after_natural_completion() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let info = manager
        .spawn_with_notify(
            "bash",
            None,
            "session-prune",
            false,
            false,
            |_output_path| async move {
                sleep(Duration::from_millis(10)).await;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;
    assert!(
        manager.is_live_task(&info.task_id),
        "task should be live right after spawn"
    );

    for _ in 0..200 {
        let status = manager
            .status(&info.task_id)
            .await
            .ok_or_else(|| anyhow!("status should exist"))?;
        if status.status != BackgroundTaskStatus::Running && !manager.is_live_task(&info.task_id) {
            // Pruned only after the status file was finalized, so the live
            // map never claims a task whose status file is already terminal.
            let (running_count, labels, _) = manager.running_snapshot();
            assert_eq!(running_count, 0, "snapshot should not count finished tasks");
            assert!(labels.is_empty());
            return Ok(());
        }
        sleep(Duration::from_millis(10)).await;
    }

    Err(anyhow!(
        "task {} was not pruned from the live map after completion",
        info.task_id
    ))
}

#[tokio::test]
async fn reconcile_marks_orphan_from_reloaded_process_failed() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    // Same PID, different instance token: exactly what an exec-based server
    // reload leaves behind.
    let mut orphan = running_status_fixture("orphan1aaaa", "session-orphan");
    orphan.owner_pid = Some(std::process::id());
    orphan.owner_instance = Some("previous-process-image".to_string());
    write_status_fixture(&manager, &orphan).await;

    let reconciled = manager.reconcile_orphaned_tasks().await;
    assert_eq!(reconciled, 1);

    let status = manager
        .status("orphan1aaaa")
        .await
        .ok_or_else(|| anyhow!("status should exist"))?;
    assert_eq!(status.status, BackgroundTaskStatus::Failed);
    let error = status.error.unwrap_or_default();
    assert!(
        error.contains("orphaned") && error.contains("reloaded"),
        "error should explain the reload orphaning, got: {error}"
    );
    assert!(status.completed_at.is_some());
    Ok(())
}

#[tokio::test]
async fn reconcile_marks_orphan_from_dead_process_failed() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    // A child process that has already exited and been reaped gives us a PID
    // that is provably not running.
    #[cfg(unix)]
    let mut child = std::process::Command::new("true")
        .spawn()
        .map_err(|err| anyhow!("spawn child: {err}"))?;
    #[cfg(windows)]
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit", "0"])
        .spawn()
        .map_err(|err| anyhow!("spawn child: {err}"))?;
    let dead_pid = child.id();
    child.wait().map_err(|err| anyhow!("wait child: {err}"))?;

    let mut orphan = running_status_fixture("orphan2bbbb", "session-orphan-dead");
    orphan.owner_pid = Some(dead_pid);
    orphan.owner_instance = Some("some-dead-instance".to_string());
    write_status_fixture(&manager, &orphan).await;

    let reconciled = manager.reconcile_orphaned_tasks().await;
    assert_eq!(reconciled, 1);
    let status = manager
        .status("orphan2bbbb")
        .await
        .ok_or_else(|| anyhow!("status should exist"))?;
    assert_eq!(status.status, BackgroundTaskStatus::Failed);
    Ok(())
}

#[tokio::test]
async fn reconcile_leaves_non_orphans_alone() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    // Owned by this exact process image: could still be bootstrapping.
    let mut own = running_status_fixture("keep1aaaa", "session-keep");
    own.owner_pid = Some(std::process::id());
    own.owner_instance = Some(model::process_instance_token().to_string());
    write_status_fixture(&manager, &own).await;

    // Legacy file without owner metadata: no safe liveness signal, leave it.
    let legacy = running_status_fixture("keep2bbbb", "session-keep");
    write_status_fixture(&manager, &legacy).await;

    // Owned by a live foreign process (spawn a sleeper so the PID is genuinely
    // alive on every platform; PID 1 is only guaranteed alive on Unix).
    let mut live_child = spawn_sleep_process()?;
    let live_pid = live_child.id();
    let mut foreign = running_status_fixture("keep3cccc", "session-keep");
    foreign.owner_pid = Some(live_pid);
    foreign.owner_instance = Some("foreign-instance".to_string());
    write_status_fixture(&manager, &foreign).await;

    // Detached with a live pid: reconciled by the detached path, not this one.
    let mut detached = running_status_fixture("keep4dddd", "session-keep");
    detached.detached = true;
    detached.pid = Some(std::process::id());
    write_status_fixture(&manager, &detached).await;

    let reconciled = manager.reconcile_orphaned_tasks().await;
    assert_eq!(reconciled, 0);

    // NB: `manager.status()` itself finalizes orphaned status files, so all
    // status reads must happen while the foreign owner is still alive.
    for task_id in ["keep1aaaa", "keep2bbbb", "keep3cccc", "keep4dddd"] {
        let status = manager
            .status(task_id)
            .await
            .ok_or_else(|| anyhow!("status for {task_id} should exist"))?;
        assert_eq!(
            status.status,
            BackgroundTaskStatus::Running,
            "{task_id} should not be reconciled"
        );
    }

    // Reap the sleeper only after the assertions so its PID stays live
    // throughout every liveness check.
    let _ = live_child.kill();
    let _ = live_child.wait();
    Ok(())
}

#[tokio::test]
async fn status_read_self_heals_orphaned_task() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let mut orphan = running_status_fixture("orphan3cccc", "session-orphan-read");
    orphan.owner_pid = Some(std::process::id());
    orphan.owner_instance = Some("previous-process-image".to_string());
    write_status_fixture(&manager, &orphan).await;

    // A plain status read (used by bg status / bg wait) heals the phantom
    // without waiting for the startup sweep.
    let status = manager
        .status("orphan3cccc")
        .await
        .ok_or_else(|| anyhow!("status should exist"))?;
    assert_eq!(status.status, BackgroundTaskStatus::Failed);

    // And wait() returns immediately instead of blocking to timeout.
    let wait_result = manager
        .wait("orphan3cccc", Duration::from_secs(5), false)
        .await
        .ok_or_else(|| anyhow!("wait should find the task"))?;
    assert_eq!(
        wait_result.reason,
        BackgroundTaskWaitReason::AlreadyFinished
    );
    Ok(())
}

#[tokio::test]
async fn abort_live_tasks_for_reload_finalizes_running_tasks() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    // A long-running task that would otherwise vanish across an exec reload.
    let info = manager
        .spawn_with_notify(
            "selfdev-build",
            Some("selfdev build".to_string()),
            "session-reload-abort",
            false,
            false,
            |_output_path| async move {
                sleep(Duration::from_secs(60)).await;
                Ok(TaskResult::completed(Some(0)))
            },
        )
        .await;
    assert!(manager.is_live_task(&info.task_id));

    let aborted = manager.abort_live_tasks_for_reload().await;
    assert_eq!(aborted, 1);
    assert!(
        !manager.is_live_task(&info.task_id),
        "live map should be drained"
    );

    let status = manager
        .status(&info.task_id)
        .await
        .ok_or_else(|| anyhow!("status should exist"))?;
    assert_eq!(status.status, BackgroundTaskStatus::Failed);
    let error = status.error.unwrap_or_default();
    assert!(
        error.contains("Interrupted by server reload"),
        "error should explain the reload interruption, got: {error}"
    );
    assert!(status.completed_at.is_some());

    // Idempotent: a second sweep finds nothing.
    assert_eq!(manager.abort_live_tasks_for_reload().await, 0);
    Ok(())
}

#[tokio::test]
async fn abort_live_tasks_for_reload_keeps_naturally_finished_status() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());

    let info = manager
        .spawn_with_notify(
            "bash",
            None,
            "session-reload-finished",
            false,
            false,
            |_output_path| async move { Ok(TaskResult::completed(Some(0))) },
        )
        .await;

    // Let the task finish and write its terminal status.
    for _ in 0..200 {
        let status = manager
            .status(&info.task_id)
            .await
            .ok_or_else(|| anyhow!("status should exist"))?;
        if status.status != BackgroundTaskStatus::Running {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(
        manager.abort_live_tasks_for_reload().await,
        0,
        "a naturally completed task should not be counted as finalized"
    );

    let status = manager
        .status(&info.task_id)
        .await
        .ok_or_else(|| anyhow!("status should exist"))?;
    assert_eq!(
        status.status,
        BackgroundTaskStatus::Completed,
        "a task that finished before the sweep must keep its real status"
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_adoption_aborts_unregistered_task() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
    struct SignalDrop(Option<tokio::sync::oneshot::Sender<()>>);
    impl Drop for SignalDrop {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
    let handle = tokio::spawn(async move {
        let _signal = SignalDrop(Some(dropped_tx));
        let _ = started_tx.send(());
        std::future::pending::<Result<crate::alphacode_tool_types::ToolOutput>>().await
    });
    started_rx.await?;
    let registry = manager.tasks.write().await;
    let mut adoption = Box::pin(manager.adopt("read", "cancelled-adoption", handle));
    // Poll registration while the registry is locked, then cancel the future.
    assert!(matches!(
        futures::poll!(adoption.as_mut()),
        std::task::Poll::Pending
    ));
    drop(adoption);
    drop(registry);
    tokio::time::timeout(Duration::from_secs(1), dropped_rx).await??;
    assert!(manager.tasks.read().await.is_empty());
    assert!(manager.list().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn cancelling_adopted_monitor_aborts_underlying_tool() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
    struct SignalDrop(Option<tokio::sync::oneshot::Sender<()>>);
    impl Drop for SignalDrop {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
    let handle = tokio::spawn(async move {
        let _signal = SignalDrop(Some(dropped_tx));
        let _ = started_tx.send(());
        std::future::pending::<Result<crate::alphacode_tool_types::ToolOutput>>().await
    });
    started_rx.await?;
    let info = manager
        .adopt("read", "cancel-adopted-monitor", handle)
        .await;
    assert!(manager.cancel(&info.task_id).await?);
    tokio::time::timeout(Duration::from_secs(1), dropped_rx).await??;
    assert!(manager.tasks.read().await.is_empty());
    let status = manager
        .status(&info.task_id)
        .await
        .expect("cancelled status");
    assert_eq!(status.status, BackgroundTaskStatus::Failed);
    assert_eq!(status.error.as_deref(), Some("Cancelled by user"));
    Ok(())
}

#[test]
fn missing_status_progress_fallback_never_claims_completion() {
    let event = BackgroundTaskProgressEvent {
        task_id: "missing-status".into(),
        tool_name: "bash".into(),
        display_name: Some("Building project".into()),
        session_id: "progress-session".into(),
        progress: BackgroundTaskProgress {
            kind: BackgroundTaskProgressKind::Determinate,
            percent: Some(100.0),
            message: Some("progress is not an exit result".into()),
            current: Some(4),
            total: Some(4),
            unit: Some("steps".into()),
            eta_seconds: None,
            updated_at: Utc::now().to_rfc3339(),
            source: BackgroundTaskProgressSource::Reported,
        },
    };
    let status = running_snapshot_from_progress(&event);
    assert_eq!(status.status, BackgroundTaskStatus::Running);
    assert_eq!(status.completed_at, None);
    assert_eq!(status.exit_code, None);
    assert_eq!(status.duration_secs, None);
    assert_eq!(status.display_name, event.display_name);
    assert_eq!(status.session_id, event.session_id);
    assert_eq!(status.progress, Some(event.progress));
}

#[tokio::test]
async fn cancelled_spawn_registration_never_launches_work() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = Arc::clone(&executed);
    let registry = manager.tasks.write().await;
    let mut spawning = Box::pin(manager.spawn("read", "cancel-spawn", move |_| async move {
        observed.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(TaskResult::completed(Some(0)))
    }));
    assert!(matches!(
        futures::poll!(spawning.as_mut()),
        std::task::Poll::Pending
    ));
    drop(spawning);
    drop(registry);
    tokio::task::yield_now().await;
    assert!(!executed.load(std::sync::atomic::Ordering::SeqCst));
    assert!(manager.tasks.read().await.is_empty());
    assert!(manager.list().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn background_execution_panic_publishes_failure_and_releases_registry() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    let info = manager
        .spawn("read", "panic-execution", |_| async {
            tokio::task::yield_now().await;
            panic!("execution failed after yield");
            #[allow(unreachable_code)]
            Ok(TaskResult::completed(Some(0)))
        })
        .await;
    let waited = manager
        .wait(&info.task_id, Duration::from_secs(1), false)
        .await
        .expect("task status should survive panic");
    assert_ne!(waited.reason, BackgroundTaskWaitReason::Timeout);
    assert_eq!(waited.task.status, BackgroundTaskStatus::Failed);
    assert_eq!(waited.task.exit_code, None);
    assert!(
        waited
            .task
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("execution failed after yield")
    );
    assert!(waited.task.completed_at.is_some());
    // The monitor writes status before pruning; yield until the finalizer has
    // finished rather than treating the status write alone as registry cleanup.
    tokio::time::timeout(Duration::from_secs(1), async {
        while manager.tasks.read().await.contains_key(&info.task_id) {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn background_recovery_restores_panic_scope_between_polls() -> Result<()> {
    let mut polled_once = false;
    let execution = std::future::poll_fn(|cx| {
        assert!(crate::alphacode_core::panic_util::panic_output_suppressed());
        if !polled_once {
            polled_once = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(Ok(TaskResult::completed(Some(0))))
        }
    });
    let mut recovery = Box::pin(recover_background_execution(execution));
    assert!(matches!(
        futures::poll!(recovery.as_mut()),
        std::task::Poll::Pending
    ));
    assert!(!crate::alphacode_core::panic_util::panic_output_suppressed());
    recovery.await?;
    assert!(!crate::alphacode_core::panic_util::panic_output_suppressed());
    Ok(())
}

#[test]
fn returned_background_outcomes_cannot_claim_contradictory_success() {
    let nonzero = TaskResult::completed(Some(7)).into_terminal();
    assert_eq!(nonzero.status, Some(BackgroundTaskStatus::Failed));
    assert_eq!(nonzero.exit_code, Some(7));
    assert_eq!(nonzero.error.as_deref(), Some("Command exited with code 7"));
    let running = TaskResult {
        status: Some(BackgroundTaskStatus::Running),
        exit_code: None,
        error: None,
    }
    .into_terminal();
    assert_eq!(running.status, Some(BackgroundTaskStatus::Failed));
    assert!(running.error.is_some());
    let contradictory = TaskResult {
        status: Some(BackgroundTaskStatus::Completed),
        exit_code: Some(0),
        error: Some("verification failed".into()),
    }
    .into_terminal();
    assert_eq!(contradictory.status, Some(BackgroundTaskStatus::Failed));
    assert_eq!(contradictory.error.as_deref(), Some("verification failed"));
    let superseded = TaskResult::superseded(Some(1), "newer task owns work").into_terminal();
    assert_eq!(superseded.status, Some(BackgroundTaskStatus::Superseded));
    let success = TaskResult::completed(Some(0)).into_terminal();
    assert_eq!(success.status, Some(BackgroundTaskStatus::Completed));
    assert!(success.error.is_none());
}

#[tokio::test]
async fn spawned_nonzero_exit_is_persisted_as_failed() -> Result<()> {
    let tmp = tempdir()?;
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    let info = manager
        .spawn("bash", "nonzero-spawn", |_| async {
            Ok(TaskResult::completed(Some(9)))
        })
        .await;
    let waited = manager
        .wait(&info.task_id, Duration::from_secs(1), false)
        .await
        .expect("persisted outcome");
    assert_ne!(waited.reason, BackgroundTaskWaitReason::Timeout);
    assert_eq!(waited.task.status, BackgroundTaskStatus::Failed);
    assert_eq!(waited.task.exit_code, Some(9));
    assert_eq!(
        waited.task.error.as_deref(),
        Some("Command exited with code 9")
    );
    Ok(())
}

#[test]
fn atomic_status_replacement_publishes_json_and_cleans_failed_scratch() -> Result<()> {
    let tmp = tempdir()?;
    let status_path = tmp.path().join("task.status.json");
    persist_status_bytes(&status_path, br#"{"status":"running"}"#)?;
    persist_status_bytes(&status_path, br#"{"status":"completed"}"#)?;
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&status_path)?)?;
    assert_eq!(value["status"], "completed");
    // Replacing a directory is rejected; scratch files must still be removed.
    let blocked = tmp.path().join("blocked.status.json");
    std::fs::create_dir(&blocked)?;
    assert!(persist_status_bytes(&blocked, br#"{"status":"failed"}"#).is_err());
    assert!(blocked.is_dir());
    assert_eq!(std::fs::read(&status_path)?, br#"{"status":"completed"}"#);
    assert_eq!(std::fs::read_dir(tmp.path())?.count(), 2);
    Ok(())
}

#[test]
fn stale_running_publication_cannot_reopen_terminal_status() -> Result<()> {
    let tmp = tempdir()?;
    let path = tmp.path().join("terminal.status.json");
    for outcome in ["completed", "failed", "superseded"] {
        let terminal = serde_json::to_vec(&serde_json::json!({"status":outcome}))?;
        persist_status_bytes(&path, &terminal)?;
        persist_status_bytes(&path, br#"{"status":"running","progress":25}"#)?;
        assert_eq!(std::fs::read(&path)?, terminal);
    }
    Ok(())
}

#[tokio::test]
async fn registry_contention_is_unknown_and_never_authorizes_duplicate_work() {
    let tmp = tempdir().expect("temporary output directory");
    let manager = BackgroundTaskManager::with_output_dir(tmp.path().to_path_buf());
    assert_eq!(manager.try_is_live_task("unregistered"), Some(false));
    let registry = manager.tasks.write().await;
    assert_eq!(manager.try_is_live_task("unregistered"), None);
    assert!(manager.is_live_task("unregistered"));
    let mut confirmed = Box::pin(manager.is_live_task_confirmed("unregistered"));
    assert!(matches!(
        futures::poll!(confirmed.as_mut()),
        std::task::Poll::Pending
    ));
    drop(registry);
    assert!(!confirmed.await);
    assert_eq!(manager.try_is_live_task("unregistered"), Some(false));
    assert!(!manager.is_live_task("unregistered"));
}
