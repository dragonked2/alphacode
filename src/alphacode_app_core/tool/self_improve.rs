use crate::alphacode_tool_core::{Tool, ToolContext};
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// A recorded task outcome for learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub task_type: String,
    pub prompt: String,
    pub success: bool,
    pub tools_used: Vec<String>,
    pub error: Option<String>,
    pub timestamp: u64,
    pub duration_ms: u64,
}

/// A learned skill from repeated patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedSkill {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub tool_sequence: Vec<String>,
    pub success_count: u64,
    pub failure_count: u64,
    pub created_at: u64,
    pub last_used: u64,
}

fn skills_store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".alphacode")
        .join("learned_skills.json")
}

fn records_store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".alphacode")
        .join("task_records.json")
}

fn load_skills() -> Vec<LearnedSkill> {
    let path = skills_store_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save_skills(skills: &[LearnedSkill]) -> Result<()> {
    let path = skills_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(skills)?)?;
    Ok(())
}

fn load_records() -> Vec<TaskRecord> {
    let path = records_store_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save_records(records: &[TaskRecord]) -> Result<()> {
    let path = records_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Keep only last 500 records
    let mut records = records.to_vec();
    if records.len() > 500 {
        records.drain(0..records.len() - 500);
    }
    std::fs::write(&path, serde_json::to_string_pretty(&records)?)?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct SelfImproveTool;

#[async_trait]
impl Tool for SelfImproveTool {
    fn name(&self) -> &str {
        "self_improve"
    }

    fn description(&self) -> &str {
        "Self-Improvement system: record task outcomes, learn from patterns, and create reusable skills. Track what works, what fails, and automatically generate skills for recurring task types."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["record", "learn", "skills", "stats", "suggest"],
                    "description": "Action: record an outcome, learn from patterns, list skills, show stats, or get suggestions."
                },
                "task_type": {
                    "type": "string",
                    "description": "Type of task (e.g., 'bug_fix', 'feature', 'refactor', 'test', 'docs')."
                },
                "prompt": {
                    "type": "string",
                    "description": "The original task prompt."
                },
                "success": {
                    "type": "boolean",
                    "description": "Whether the task succeeded (for 'record' action)."
                },
                "tools_used": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of tools used in the task."
                },
                "error": {
                    "type": "string",
                    "description": "Error message if the task failed."
                },
                "duration_ms": {
                    "type": "integer",
                    "description": "How long the task took in milliseconds."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("stats");

        match action {
            "record" => self.record_outcome(input).await,
            "learn" => self.learn_patterns().await,
            "skills" => self.list_skills().await,
            "stats" => self.show_stats().await,
            "suggest" => self.suggest().await,
            _ => Ok(ToolOutput::new(format!(
                "Unknown action '{action}'. Use: record, learn, skills, stats, suggest."
            ))),
        }
    }
}

impl SelfImproveTool {
    async fn record_outcome(&self, input: Value) -> Result<ToolOutput> {
        let task_type = input
            .get("task_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let success = input
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let tools_used: Vec<String> = input
            .get("tools_used")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let error = input
            .get("error")
            .and_then(|v| v.as_str())
            .map(String::from);
        let duration_ms = input
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let record = TaskRecord {
            id: format!("task_{}", now_ms()),
            task_type,
            prompt,
            success,
            tools_used,
            error,
            timestamp: now_ms(),
            duration_ms,
        };

        let mut records = load_records();
        records.push(record.clone());
        save_records(&records)?;

        let status = if record.success { "✅" } else { "❌" };
        Ok(ToolOutput::new(format!(
            "{status} Recorded task outcome: {} ({})\nDuration: {}ms\nTotal records: {}",
            record.task_type,
            if record.success { "success" } else { "failure" },
            record.duration_ms,
            records.len()
        )))
    }

    async fn learn_patterns(&self) -> Result<ToolOutput> {
        let records = load_records();
        if records.len() < 5 {
            return Ok(ToolOutput::new(
                "Need at least 5 task records to learn patterns. Keep recording outcomes!"
                    .to_string(),
            ));
        }

        let mut tool_sequences: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut type_success: std::collections::HashMap<String, (u64, u64)> =
            std::collections::HashMap::new();

        for record in &records {
            let entry = type_success
                .entry(record.task_type.clone())
                .or_insert((0, 0));
            if record.success {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }

            if record.tools_used.len() >= 2 {
                let key = record.tools_used.join(" → ");
                tool_sequences
                    .entry(record.task_type.clone())
                    .or_default()
                    .push(key);
            }
        }

        let mut skills = load_skills();
        let mut new_skills = 0;

        for (task_type, sequences) in &tool_sequences {
            let mut freq: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
            for seq in sequences {
                *freq.entry(seq.clone()).or_insert(0) += 1;
            }

            for (pattern, count) in freq {
                if count >= 3 {
                    let exists = skills.iter().any(|s| s.pattern == pattern);
                    if !exists {
                        let skill = LearnedSkill {
                            id: format!("skill_{}", now_ms()),
                            name: format!("{} pattern", task_type),
                            pattern: pattern.clone(),
                            tool_sequence: pattern.split(" → ").map(String::from).collect(),
                            success_count: 0,
                            failure_count: 0,
                            created_at: now_ms(),
                            last_used: now_ms(),
                        };
                        skills.push(skill);
                        new_skills += 1;
                    }
                }
            }
        }

        save_skills(&skills)?;

        Ok(ToolOutput::new(format!(
            "🧠 Learning complete!\n\nAnalyzed {} task records.\nFound {} new patterns.\nTotal learned skills: {}.\n\nTip: Continue recording task outcomes to discover more patterns.",
            records.len(),
            new_skills,
            skills.len()
        )))
    }

    async fn list_skills(&self) -> Result<ToolOutput> {
        let skills = load_skills();
        if skills.is_empty() {
            return Ok(ToolOutput::new(
                "No learned skills yet. Use 'learn' after recording enough task outcomes."
                    .to_string(),
            ));
        }

        let mut output = format!("📚 {} Learned Skills:\n\n", skills.len());
        for skill in &skills {
            output.push_str(&format!(
                "  {} (id: {})\n    Pattern: {}\n    Tools: {}\n    Occurrences: {}\n\n",
                skill.name,
                skill.id,
                skill.pattern,
                skill.tool_sequence.join(" → "),
                skill.success_count + skill.failure_count
            ));
        }

        Ok(ToolOutput::new(output))
    }

    async fn show_stats(&self) -> Result<ToolOutput> {
        let records = load_records();
        let skills = load_skills();

        let total = records.len();
        let successes = records.iter().filter(|r| r.success).count();
        let failures = total - successes;
        let avg_duration = if total > 0 {
            records.iter().map(|r| r.duration_ms).sum::<u64>() / total as u64
        } else {
            0
        };

        let mut by_type: std::collections::HashMap<String, (u64, u64)> =
            std::collections::HashMap::new();
        for record in &records {
            let entry = by_type.entry(record.task_type.clone()).or_insert((0, 0));
            if record.success {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }

        let success_pct = if total > 0 {
            successes as f64 / total as f64 * 100.0
        } else {
            0.0
        };

        let mut output = format!(
            "📊 Self-Improvement Stats:\n\n\
             Total tasks recorded: {total}\n\
             Successes: {successes} ({success_pct:.0}%)\n\
             Failures: {failures}\n\
             Avg duration: {avg_duration}ms\n\
             Learned skills: {}\n\n",
            skills.len()
        );

        if !by_type.is_empty() {
            output.push_str("By task type:\n");
            let mut sorted: Vec<_> = by_type.into_iter().collect();
            sorted.sort_by(|a, b| b.1.0.cmp(&a.1.0));
            for (task_type, (s, f)) in sorted.iter().take(10) {
                output.push_str(&format!("  {task_type}: {} success, {} fail\n", s, f));
            }
        }

        Ok(ToolOutput::new(output))
    }

    async fn suggest(&self) -> Result<ToolOutput> {
        let records = load_records();
        let skills = load_skills();

        if records.is_empty() {
            return Ok(ToolOutput::new(
                "No data yet. Start recording task outcomes to get suggestions.".to_string(),
            ));
        }

        let mut output = String::from("💡 Suggestions:\n\n");

        let failures: Vec<&TaskRecord> = records.iter().filter(|r| !r.success).collect();
        if !failures.is_empty() {
            output.push_str("Common failure patterns:\n");
            let mut error_freq: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            for f in &failures {
                if let Some(ref err) = f.error {
                    let short = if err.len() > 80 {
                        format!("{}...", &err[..80])
                    } else {
                        err.clone()
                    };
                    *error_freq.entry(short).or_insert(0) += 1;
                }
            }
            let mut sorted_errors: Vec<_> = error_freq.into_iter().collect();
            sorted_errors.sort_by(|a, b| b.1.cmp(&a.1));
            for (err, count) in sorted_errors.iter().take(5) {
                output.push_str(&format!("  ⚠️  [{count}x] {err}\n"));
            }
            output.push('\n');
        }

        if !skills.is_empty() {
            output.push_str("Recommended skills to use:\n");
            for skill in skills.iter().take(3) {
                output.push_str(&format!(
                    "  🎯 {} - Pattern: {}\n",
                    skill.name, skill.pattern
                ));
            }
            output.push('\n');
        }

        let total = records.len();
        let successes = records.iter().filter(|r| r.success).count();
        let success_rate = if total > 0 {
            successes as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        if success_rate < 80.0 {
            output.push_str("⚠️ Success rate below 80%. Consider reviewing failure patterns.\n");
        } else if success_rate > 95.0 {
            output.push_str("✨ Excellent success rate! Keep up the good work.\n");
        }

        Ok(ToolOutput::new(output))
    }
}
