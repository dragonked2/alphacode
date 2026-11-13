//! Rolling request budgets and cooldowns shared by local Alphacode processes.
//! Files contain timestamps only; names hash the origin, never credentials.
//! The OS file lock is released on cancellation/process exit. No lock is held
//! while sleeping or making network calls.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Default)]
pub(super) struct Limits {
    minute: usize,
    hour: usize,
}

impl Limits {
    fn for_origin(origin: &url::Origin) -> Self {
        // Leave headroom below TokenHarbor's free-account limits. Sharing by
        // origin intentionally also covers multiple keys on the same account.
        let tokenharbor = matches!(origin, url::Origin::Tuple(_, host, _) if host.to_string() == "tokenharbor.ai");
        Self {
            minute: env_limit(
                "ALPHACODE_PROVIDER_REQUESTS_PER_MINUTE",
                if tokenharbor { 50 } else { 0 },
            ),
            hour: env_limit(
                "ALPHACODE_PROVIDER_REQUESTS_PER_HOUR",
                if tokenharbor { 1500 } else { 0 },
            ),
        }
    }
}

fn env_limit(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[derive(Default, Serialize, Deserialize)]
struct Ledger {
    #[serde(default)]
    starts: Vec<u64>,
    #[serde(default)]
    cooldown_until_ms: u64,
}

/// No reservation is made when admission is deferred. A cancelled request
/// after admission may conservatively consume one slot, but cannot send twice.
fn transact(
    path: &Path,
    limits: Limits,
    now_ms: u64,
    cooldown: Option<Duration>,
) -> Result<Duration> {
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))?;
    match lock_file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(Duration::from_millis(25)),
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    let mut bytes = Vec::new();
    // Bound damaged/untrusted local state before allocating.
    match std::fs::File::open(path) {
        Ok(file) => {
            file.take(4 * 1024 * 1024).read_to_end(&mut bytes)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut ledger: Ledger = if bytes.is_empty() {
        Ledger::default()
    } else {
        serde_json::from_slice(&bytes).context("Invalid provider request budget state")?
    };
    if let Some(delay) = cooldown {
        let millis = delay.min(super::retry_after::MAX_RETRY_AFTER).as_millis() as u64;
        ledger.cooldown_until_ms = ledger.cooldown_until_ms.max(now_ms.saturating_add(millis));
        save(path, &ledger)?;
        return Ok(Duration::ZERO);
    }
    ledger
        .starts
        .retain(|&start| now_ms.saturating_sub(start) < 3_600_000);
    ledger.starts.sort_unstable();
    let mut wait_ms = ledger.cooldown_until_ms.saturating_sub(now_ms);
    for (limit, window) in [(limits.minute, 60_000_u64), (limits.hour, 3_600_000)] {
        if limit == 0 {
            continue;
        }
        let recent: Vec<_> = ledger
            .starts
            .iter()
            .copied()
            .filter(|&start| now_ms.saturating_sub(start) < window)
            .collect();
        if recent.len() >= limit {
            wait_ms = wait_ms.max(
                recent[recent.len() - limit]
                    .saturating_add(window)
                    .saturating_sub(now_ms),
            );
        }
    }
    if wait_ms > 0 {
        return Ok(Duration::from_millis(wait_ms));
    }
    if limits.minute > 0 || limits.hour > 0 {
        // Bound the ledger even if the configured limits are excessively large.
        if ledger.starts.len() >= 100_000 {
            return Ok(Duration::from_millis(
                ledger.starts[0]
                    .saturating_add(3_600_000)
                    .saturating_sub(now_ms),
            ));
        }
        ledger.starts.push(now_ms);
        save(path, &ledger)?;
    }
    Ok(Duration::ZERO)
}

fn save(path: &Path, ledger: &Ledger) -> Result<()> {
    let bytes = serde_json::to_vec(ledger)?;
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("Budget path has no parent")?)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn ledger_path(key: &str) -> Result<PathBuf> {
    #[cfg(not(test))]
    let root = dirs::cache_dir()
        .context("No local cache directory for provider budgets")?
        .join("alphacode")
        .join("provider-limits");
    // Local-server tests must never modify the user's real quota/cooldowns.
    #[cfg(test)]
    let root = std::env::temp_dir().join(format!(
        "alphacode-provider-budget-tests-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root)?;
    Ok(root.join(format!("{:x}.json", Sha256::digest(key.as_bytes()))))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

async fn shared(key: String, limits: Limits, cooldown: Option<Duration>) -> Result<Duration> {
    tokio::task::spawn_blocking(move || {
        let path = ledger_path(&key)?;
        // Unlimited healthy origins need no ledger/lock file. This also keeps
        // tool fetches across many different hosts from growing the cache.
        if cooldown.is_none() && limits.minute == 0 && limits.hour == 0 && !path.try_exists()? {
            return Ok(Duration::ZERO);
        }
        transact(&path, limits, now_ms(), cooldown)
    })
    .await
    .context("Provider budget worker failed")?
}

pub(super) async fn try_admit(origin: &url::Origin) -> Result<Duration> {
    shared(
        origin.ascii_serialization(),
        Limits::for_origin(origin),
        None,
    )
    .await
}

pub(super) async fn note_cooldown(origin: &url::Origin, delay: Duration) -> Result<()> {
    // A busy file lock defers the write; never silently lose a server cooldown.
    loop {
        let wait = shared(origin.ascii_serialization(), Limits::default(), Some(delay)).await?;
        if wait.is_zero() {
            return Ok(());
        }
        tokio::time::sleep(wait).await;
    }
}

/// Sidecar callers skip optional work rather than parking a backlog of memory
/// jobs. All backends, clones and local processes share this small allowance.
pub(crate) async fn try_background() -> Result<bool> {
    let limits = Limits {
        minute: env_limit("ALPHACODE_MEMORY_REQUESTS_PER_MINUTE", 12),
        hour: env_limit("ALPHACODE_MEMORY_REQUESTS_PER_HOUR", 120),
    };
    let key = "background-memory".to_string();
    // Sidecar tests already isolate app state with temporary ALPHACODE_HOME.
    #[cfg(test)]
    let key = format!(
        "{key}:{}",
        std::env::var("ALPHACODE_HOME").unwrap_or_default()
    );
    Ok(shared(key, limits, None).await?.is_zero())
}

pub(super) fn deferred_error(delay: Duration) -> anyhow::Error {
    super::retry_after::error_with_retry_after(
        format!(
            "Provider request deferred\n  status: 429\n  rate limit cooldown\n\"retry_after_seconds\":{}",
            delay
                .as_secs()
                .saturating_add(u64::from(delay.subsec_nanos() > 0))
        ),
        Some(super::retry_after::RetryAfter::new(delay)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_windows_share_state_and_release_at_exact_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("budget.json");
        let limits = Limits { minute: 2, hour: 3 };
        assert!(transact(&path, limits, 0, None).unwrap().is_zero());
        assert!(transact(&path, limits, 1, None).unwrap().is_zero());
        assert_eq!(
            transact(&path, limits, 2, None).unwrap(),
            Duration::from_millis(59_998)
        );
        assert!(transact(&path, limits, 60_000, None).unwrap().is_zero());
        assert_eq!(
            transact(&path, limits, 60_001, None).unwrap(),
            Duration::from_millis(3_539_999)
        );
        assert!(transact(&path, limits, 3_600_000, None).unwrap().is_zero());
    }

    #[test]
    fn long_cooldown_survives_reopen_and_shorter_hints() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("budget.json");
        transact(
            &path,
            Limits::default(),
            1000,
            Some(Duration::from_secs(3541)),
        )
        .unwrap();
        transact(&path, Limits::default(), 2000, Some(Duration::from_secs(1))).unwrap();
        assert_eq!(
            transact(&path, Limits::default(), 2000, None).unwrap(),
            Duration::from_secs(3540)
        );
    }

    #[test]
    fn concurrent_admissions_cannot_exceed_shared_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("budget.json");
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    transact(&path, Limits { minute: 1, hour: 0 }, 1000, None)
                        .unwrap()
                        .is_zero()
                })
            })
            .collect();
        assert_eq!(
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .filter(|&allowed| allowed)
                .count(),
            1
        );
    }

    #[test]
    fn subprocess_admission_worker() {
        let Some(path) = std::env::var_os("ALPHACODE_BUDGET_TEST_FILE") else {
            return;
        };
        assert!(
            transact(Path::new(&path), Limits { minute: 1, hour: 0 }, 1000, None)
                .unwrap()
                .is_zero()
        );
    }

    #[test]
    fn independent_process_reservation_is_visible() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("budget.json");
        let name = format!(
            "{}::subprocess_admission_worker",
            module_path!().split_once("::").unwrap().1
        );
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env("ALPHACODE_BUDGET_TEST_FILE", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            path.exists(),
            "child test did not run: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            transact(&path, Limits { minute: 1, hour: 0 }, 1000, None).unwrap(),
            Duration::from_secs(60)
        );
    }
}
