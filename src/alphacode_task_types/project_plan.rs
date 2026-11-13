//! Serializable project-plan state shared by coordination and persistence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single phase in a master execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanPhase {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    #[serde(default)]
    pub milestones: Vec<Milestone>,
    #[serde(default)]
    pub agent_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PhaseStatus {
    #[default]
    Pending,
    Active,
    Complete,
    Blocked,
    Skipped,
}

/// A milestone within a phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Milestone {
    pub id: String,
    pub name: String,
    pub criteria: Vec<String>,
    pub status: PhaseStatus,
    pub created_at: DateTime<Utc>,
}
