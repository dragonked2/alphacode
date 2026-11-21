use crate::alphacode_tool_core::{Tool, ToolContext};
use crate::alphacode_tool_types::ToolOutput;
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
struct LegacyCronJob {
    id: String,
    name: String,
    schedule: String,
    enabled: bool,
    // Keep the rest of the original record intact when disabling jobs.
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

fn cron_store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".alphacode")
        .join("cron_jobs.json")
}

fn load_jobs(path: &Path) -> Result<Vec<LegacyCronJob>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("Invalid legacy cron store"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).context("Could not read the legacy cron store"),
    }
}

fn update_jobs(path: &Path, job_id: &str, delete: bool) -> Result<()> {
    let directory = path.parent().context("Legacy cron store has no parent")?;
    fs::create_dir_all(directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(".cron_jobs.lock"))?;
    lock.lock()?;

    let mut jobs = load_jobs(path)?;
    if !jobs.iter().any(|job| job.id == job_id) {
        bail!("Legacy cron entry '{job_id}' was not found");
    }
    if delete {
        jobs.retain(|job| job.id != job_id);
    } else {
        for job in jobs.iter_mut().filter(|job| job.id == job_id) {
            job.enabled = false;
        }
    }

    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer_pretty(&mut temporary, &jobs)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .context("Could not replace the legacy cron store")?;
    Ok(())
}

pub struct CronTool;

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "List, pause, or delete legacy cron entries. Use schedule for executable one-shot tasks."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "pause", "delete"],
                    "description": "Action to perform on the legacy store."
                },
                "job_id": {
                    "type": "string",
                    "description": "Legacy entry ID for pause or delete."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("list");
        let path = cron_store_path();
        match action {
            "list" => {
                let jobs = load_jobs(&path)?;
                if jobs.is_empty() {
                    return Ok(ToolOutput::new("No legacy cron entries."));
                }

                let mut output =
                    String::from("Legacy cron entries (no executor reads this store):\n");
                for job in jobs {
                    let status = if job.enabled { "inactive" } else { "paused" };
                    output.push_str(&format!(
                        "- {} | {} | {} | {}\n",
                        job.id, job.name, job.schedule, status
                    ));
                    if let Some(prompt) = job.extra.get("prompt").and_then(Value::as_str) {
                        let preview = crate::alphacode_core::util::truncate_str(prompt, 80);
                        output.push_str(&format!("  Prompt: {preview}\n"));
                    }
                }
                Ok(ToolOutput::new(output))
            }
            "pause" | "delete" => {
                let job_id = input
                    .get("job_id")
                    .and_then(Value::as_str)
                    .context("job_id is required for pause or delete")?;
                let delete = action == "delete";
                update_jobs(&path, job_id, delete)?;
                let verb = if delete { "Deleted" } else { "Paused" };
                Ok(ToolOutput::new(format!(
                    "{verb} legacy cron entry '{job_id}'."
                )))
            }
            "create" | "resume" | "run" => {
                bail!("The legacy cron store has no executor. Use schedule for a one-shot task.")
            }
            _ => bail!("Unknown cron action '{action}'. Use list, pause, or delete."),
        }
    }
}
