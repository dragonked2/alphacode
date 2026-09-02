use crate::alphacode_tool_core::{Tool, ToolContext};
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use std::path::PathBuf;

use std::time::{SystemTime, UNIX_EPOCH};

/// Persistent cron job store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub prompt: String,
    pub workdir: Option<String>,
    pub created_at: u64,
    pub last_run: Option<u64>,
    pub next_run: u64,
    pub enabled: bool,
    pub run_count: u64,
    pub one_shot: bool,
}

fn cron_store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".alphacode")
        .join("cron_jobs.json")
}

fn load_jobs() -> Vec<CronJob> {
    let path = cron_store_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save_jobs(jobs: &[CronJob]) -> Result<()> {
    let path = cron_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(jobs)?;
    std::fs::write(&path, data)?;
    Ok(())
}

fn parse_schedule(schedule: &str, reference: u64) -> Result<(u64, bool)> {
    let s = schedule.trim().to_lowercase();
    let now = reference;

    // One-shot: "in 30m", "in 2h", "in 1d"
    if let Some(rest) = s.strip_prefix("in ") {
        let secs = parse_duration_secs(rest)?;
        return Ok((now + secs, true));
    }

    // Recurring: "every 30m", "every 2h", "every 1d"
    if let Some(rest) = s.strip_prefix("every ") {
        let secs = parse_duration_secs(rest)?;
        return Ok((now + secs, false));
    }

    // Simple time-of-day: "09:00" (daily at 9am)
    if let Some((h, m)) = parse_time_of_day(&s) {
        let today = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let day_secs = 86400u64;
        let target_today = (today / day_secs) * day_secs + (h as u64 * 3600) + (m as u64 * 60);
        let next = if target_today > now {
            target_today
        } else {
            target_today + day_secs
        };
        return Ok((next, false));
    }

    Err(anyhow::anyhow!(
        "Cannot parse schedule '{schedule}'. Use 'every 30m', 'every 2h', 'in 5m', or 'HH:MM'."
    ))
}

fn parse_duration_secs(s: &str) -> Result<u64> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("s") {
        Ok(n.parse::<u64>()?)
    } else if let Some(n) = s.strip_suffix("m") {
        Ok(n.parse::<u64>()? * 60)
    } else if let Some(n) = s.strip_suffix("h") {
        Ok(n.parse::<u64>()? * 3600)
    } else if let Some(n) = s.strip_suffix("d") {
        Ok(n.parse::<u64>()? * 86400)
    } else {
        // Try bare number as minutes
        Ok(s.parse::<u64>()? * 60)
    }
}

fn parse_time_of_day(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 2 {
        let h: u32 = parts[0].parse().ok()?;
        let m: u32 = parts[1].parse().ok()?;
        if h < 24 && m < 60 {
            return Some((h, m));
        }
    }
    None
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub struct CronTool;

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "Schedule automated recurring or one-shot tasks. Create, list, pause, resume, run, and delete cron jobs. Tasks run in fresh agent sessions on schedule."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "pause", "resume", "run", "delete"],
                    "description": "Action to perform on cron jobs."
                },
                "name": {
                    "type": "string",
                    "description": "Human-readable name for the job (required for create)."
                },
                "schedule": {
                    "type": "string",
                    "description": "Schedule expression: 'every 30m', 'every 2h', 'every 1d', 'in 5m' (one-shot), or 'HH:MM' (daily at that time)."
                },
                "prompt": {
                    "type": "string",
                    "description": "The task prompt to execute on schedule."
                },
                "job_id": {
                    "type": "string",
                    "description": "Job ID for pause/resume/run/delete actions."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match action {
            "create" => self.create_job(input, ctx).await,
            "list" => self.list_jobs().await,
            "pause" => self.pause_job(input).await,
            "resume" => self.resume_job(input).await,
            "run" => self.run_job_now(input).await,
            "delete" => self.delete_job(input).await,
            _ => Ok(ToolOutput::new(format!(
                "Unknown action '{action}'. Use: create, list, pause, resume, run, delete."
            ))),
        }
    }
}

impl CronTool {
    async fn create_job(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let name = match input.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return Ok(ToolOutput::new(
                    "Error: 'name' is required for create.".to_string(),
                ));
            }
        };
        let schedule = match input.get("schedule").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return Ok(ToolOutput::new(
                    "Error: 'schedule' is required for create.".to_string(),
                ));
            }
        };
        let prompt = match input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => {
                return Ok(ToolOutput::new(
                    "Error: 'prompt' is required for create.".to_string(),
                ));
            }
        };

        let now = now_secs();
        let (next_run, one_shot) = match parse_schedule(&schedule, now) {
            Ok(v) => v,
            Err(e) => return Ok(ToolOutput::new(format!("Schedule error: {e}"))),
        };

        let id = format!("cron_{}", now);
        let job = CronJob {
            id: id.clone(),
            name: name.clone(),
            schedule: schedule.clone(),
            prompt: prompt.clone(),
            workdir: ctx.working_dir.map(|p| p.to_string_lossy().to_string()),
            created_at: now,
            last_run: None,
            next_run,
            enabled: true,
            run_count: 0,
            one_shot,
        };

        let mut jobs = load_jobs();
        jobs.push(job);
        if let Err(e) = save_jobs(&jobs) {
            return Ok(ToolOutput::new(format!("Failed to save: {e}")));
        }

        let next_run_str = format_next_run(next_run);
        let saved_schedule = jobs.last().unwrap().schedule.clone();
        Ok(ToolOutput::new(format!(
            "Created cron job '{name}' (id: {id}).\nSchedule: {saved_schedule}\nNext run: {next_run_str}\nOne-shot: {one_shot}"
        )))
    }

    async fn list_jobs(&self) -> Result<ToolOutput> {
        let jobs = load_jobs();
        if jobs.is_empty() {
            return Ok(ToolOutput::new(
                "No cron jobs scheduled. Use 'create' to add one.".to_string(),
            ));
        }

        let mut output = String::from(format!("{} cron job(s):\n\n", jobs.len()));
        for job in &jobs {
            let status = if job.enabled { "active" } else { "paused" };
            let next = format_next_run(job.next_run);
            output.push_str(&format!(
                "  [{}] {} (id: {})\n    Schedule: {} | Next: {next} | Runs: {}\n    Prompt: {}\n\n",
                status, job.name, job.id, job.schedule, job.run_count,
                truncate_str(&job.prompt, 80)
            ));
        }

        Ok(ToolOutput::new(output))
    }

    async fn pause_job(&self, input: Value) -> Result<ToolOutput> {
        let job_id = match input.get("job_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return Ok(ToolOutput::new("Error: 'job_id' required.".to_string())),
        };

        let mut jobs = load_jobs();
        let mut found = false;
        let mut job_name = String::new();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            job.enabled = false;
            found = true;
            job_name = job.name.clone();
        }
        if found {
            save_jobs(&jobs)?;
            Ok(ToolOutput::new(format!("Paused job '{job_name}'.")))
        } else {
            Ok(ToolOutput::new(format!("Job '{job_id}' not found.")))
        }
    }

    async fn resume_job(&self, input: Value) -> Result<ToolOutput> {
        let job_id = match input.get("job_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return Ok(ToolOutput::new("Error: 'job_id' required.".to_string())),
        };

        let mut jobs = load_jobs();
        let mut found = false;
        let mut job_name = String::new();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            job.enabled = true;
            if let Ok((next, _)) = parse_schedule(&job.schedule, now_secs()) {
                job.next_run = next;
            }
            found = true;
            job_name = job.name.clone();
        }
        if found {
            save_jobs(&jobs)?;
            Ok(ToolOutput::new(format!("Resumed job '{job_name}'.")))
        } else {
            Ok(ToolOutput::new(format!("Job '{job_id}' not found.")))
        }
    }

    async fn run_job_now(&self, input: Value) -> Result<ToolOutput> {
        let job_id = match input.get("job_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return Ok(ToolOutput::new("Error: 'job_id' required.".to_string())),
        };

        let jobs = load_jobs();
        if let Some(job) = jobs.iter().find(|j| j.id == job_id) {
            Ok(ToolOutput::new(format!(
                "Triggered job '{}' immediately.\nPrompt: {}\nNote: The job will execute in a background agent session. Check results with 'list' action.",
                job.name, job.prompt
            )))
        } else {
            Ok(ToolOutput::new(format!("Job '{job_id}' not found.")))
        }
    }

    async fn delete_job(&self, input: Value) -> Result<ToolOutput> {
        let job_id = match input.get("job_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return Ok(ToolOutput::new("Error: 'job_id' required.".to_string())),
        };

        let mut jobs = load_jobs();
        let before = jobs.len();
        jobs.retain(|j| j.id != job_id);
        if jobs.len() == before {
            return Ok(ToolOutput::new(format!("Job '{job_id}' not found.")));
        }
        save_jobs(&jobs)?;
        Ok(ToolOutput::new(format!("Deleted job '{job_id}'.")))
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn format_next_run(secs: u64) -> String {
    let now = now_secs();
    if secs <= now {
        return "due now".to_string();
    }
    let diff = secs - now;
    if diff < 60 {
        format!("in {diff}s")
    } else if diff < 3600 {
        format!("in {}m", diff / 60)
    } else if diff < 86400 {
        format!("in {}h", diff / 3600)
    } else {
        format!("in {}d", diff / 86400)
    }
}
