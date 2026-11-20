//! Fail-closed provenance for findings derived from a source repository.
//!
//! A local checkout is useful for generating hypotheses, but is not an
//! authority for publication.  This module records the exact local object,
//! two independent views of the authoritative remote, repository eligibility,
//! clone health, and a discriminating sibling-path control.  Only a manifest
//! whose observations agree may create a validation marker.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Lifecycle for a source-derived finding.  `Validated` is deliberately the
/// only state that permits final/report output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceValidationState {
    #[default]
    Pending,
    Validated,
    Contradicted,
    CorruptedClone,
    Ineligible,
    Stale,
    Void,
}

impl SourceValidationState {
    pub fn permits_final_output(self) -> bool {
        self == Self::Validated
    }
}

/// Repository/program eligibility is separate from technical validity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EligibilityVerdict {
    #[default]
    Pending,
    Eligible,
    Uncertain,
    Ineligible,
}

/// Evidence that the selected repository is an active bounty target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryEligibility {
    pub verdict: EligibilityVerdict,
    pub security_policy_url: Option<String>,
    pub active_program_url: Option<String>,
    pub repository_archived: Option<bool>,
    pub repository_deprecated: Option<bool>,
    pub checked_at: Option<DateTime<Utc>>,
    pub rationale: String,
}

impl RepositoryEligibility {
    fn defects(&self) -> Vec<String> {
        let mut defects = Vec::new();
        if self.verdict != EligibilityVerdict::Eligible {
            defects.push(format!("repository eligibility is {:?}", self.verdict));
        }
        if self
            .security_policy_url
            .as_deref()
            .is_none_or(str::is_empty)
        {
            defects.push("security policy URL was not recorded".to_string());
        }
        if self.active_program_url.as_deref().is_none_or(str::is_empty) {
            defects.push("active program URL was not recorded".to_string());
        }
        if self.repository_archived != Some(false) {
            defects.push("repository is archived or archive status is unknown".to_string());
        }
        if self.repository_deprecated != Some(false) {
            defects.push("repository is deprecated or deprecation status is unknown".to_string());
        }
        if self.checked_at.is_none() {
            defects.push("eligibility check timestamp was not recorded".to_string());
        }
        defects
    }
}

/// Result of checking the local Git object store before evidence collection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneHealth {
    pub fsck_passed: bool,
    pub revision: String,
    pub commit_object_id: String,
    pub checked_at: Option<DateTime<Utc>>,
    pub clean_reclone_required: bool,
    pub diagnostic: String,
}

impl CloneHealth {
    fn defects(&self, revision: &str) -> Vec<String> {
        let mut defects = Vec::new();
        if !self.fsck_passed {
            defects.push("git object-store integrity check did not pass".to_string());
        }
        if self.clean_reclone_required {
            defects.push("clone was quarantined and requires a clean re-clone".to_string());
        }
        if self.revision != revision || self.commit_object_id != revision {
            defects.push("clone health record is not pinned to the manifest revision".to_string());
        }
        if self.checked_at.is_none() {
            defects.push("clone health timestamp was not recorded".to_string());
        }
        defects
    }
}

/// Independent channel used to observe a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChannel {
    LocalGitObject,
    RemoteRaw,
    RemoteApi,
}

/// One exact observation of a file at a pinned revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceObservation {
    pub channel: SourceChannel,
    /// Endpoint or local clone identity.  Remote identities must be distinct.
    pub source: String,
    pub revision: String,
    pub file_path: String,
    /// Git blob object ID where the endpoint exposes it.
    pub object_id: Option<String>,
    /// SHA-256 over the complete unmodified file bytes.
    pub content_sha256: String,
    pub collected_at: DateTime<Utc>,
    /// Optional bounded excerpt for reviewer context; never used for equality.
    #[serde(default)]
    pub excerpt: String,
}

/// Negative control required for authorization/source enforcement hypotheses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegativeControl {
    pub sibling_path: String,
    pub expected_enforcement: String,
    pub observed_enforcement: String,
    pub evidence_id: String,
    pub passed: bool,
}

/// Immutable marker produced only by a successful consensus evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationMarker {
    pub finding_id: String,
    pub manifest_sha256: String,
    pub validated_at: DateTime<Utc>,
}

/// Complete provenance record for one source-derived finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEvidenceManifest {
    pub finding_id: String,
    pub repository: String,
    pub revision: String,
    pub file_path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub eligibility: RepositoryEligibility,
    pub clone_health: CloneHealth,
    pub local: SourceObservation,
    pub remotes: Vec<SourceObservation>,
    #[serde(default)]
    pub negative_controls: Vec<NegativeControl>,
    #[serde(default)]
    pub state: SourceValidationState,
    #[serde(default)]
    pub defects: Vec<String>,
    #[serde(default)]
    pub marker: Option<ValidationMarker>,
}

impl SourceEvidenceManifest {
    /// Recompute consensus from the recorded observations.  Caller-supplied
    /// state and markers are discarded, so serialized `validated` text cannot
    /// bypass the gate.
    pub fn evaluate(&mut self, now: DateTime<Utc>) -> SourceValidationState {
        self.marker = None;
        self.defects.clear();

        if self.finding_id.trim().is_empty()
            || self.repository.trim().is_empty()
            || self.revision.trim().is_empty()
            || self.revision == "unknown"
            || self.file_path.trim().is_empty()
        {
            self.defects
                .push("finding, repository, exact revision, and file path are required".into());
        }
        if matches!((self.line_start, self.line_end), (Some(start), Some(end)) if start > end) {
            self.defects.push("source line range is inverted".into());
        }
        let eligibility_defects = self.eligibility.defects();
        let ineligible = self.eligibility.verdict == EligibilityVerdict::Ineligible
            || self.eligibility.repository_archived == Some(true)
            || self.eligibility.repository_deprecated == Some(true);
        self.defects.extend(eligibility_defects);
        self.defects
            .extend(self.clone_health.defects(&self.revision));

        if let Some(defect) = Self::observation_defect(
            &self.revision,
            &self.file_path,
            &self.local,
            SourceChannel::LocalGitObject,
            "local",
        ) {
            self.defects.push(defect);
        }
        if self.local.object_id.as_deref().is_none_or(str::is_empty) {
            self.defects
                .push("local Git blob object ID was not recorded".into());
        }

        let independent_remotes = self.remotes.iter().any(|raw| {
            raw.channel == SourceChannel::RemoteRaw
                && self.remotes.iter().any(|api| {
                    api.channel == SourceChannel::RemoteApi
                        && raw.source.trim() != api.source.trim()
                })
        });
        if !independent_remotes {
            self.defects
                .push("distinct authoritative raw and API endpoints are required".into());
        }
        for remote in &self.remotes {
            if remote.channel == SourceChannel::LocalGitObject {
                self.defects
                    .push("remote evidence contains a local Git observation".into());
            }
            if let Some(defect) = Self::observation_defect(
                &self.revision,
                &self.file_path,
                remote,
                remote.channel,
                "remote",
            ) {
                self.defects.push(defect);
            }
        }
        if !self.remotes.iter().any(|remote| {
            remote.channel == SourceChannel::RemoteApi
                && remote.object_id.as_deref().is_some_and(|id| !id.is_empty())
        }) {
            self.defects
                .push("remote API Git blob object ID was not recorded".into());
        }

        if !self.negative_controls.iter().any(|control| {
            control.passed
                && !control.sibling_path.trim().is_empty()
                && !control.expected_enforcement.trim().is_empty()
                && !control.observed_enforcement.trim().is_empty()
                && !control.evidence_id.trim().is_empty()
        }) {
            self.defects.push(
                "a passed sibling enforcement-path negative control with evidence is required"
                    .into(),
            );
        }

        let local_hash = self.local.content_sha256.as_str();
        let content_mismatch = self
            .remotes
            .iter()
            .any(|remote| remote.content_sha256 != local_hash)
            || self
                .remotes
                .windows(2)
                .any(|pair| pair[0].content_sha256 != pair[1].content_sha256);
        let same_object_different_content = self.remotes.iter().any(|remote| {
            self.local.object_id.is_some()
                && self.local.object_id == remote.object_id
                && remote.content_sha256 != local_hash
        });
        let object_mismatch = self.remotes.iter().any(|remote| {
            remote.object_id.is_some()
                && self.local.object_id.is_some()
                && remote.object_id != self.local.object_id
        });

        if content_mismatch {
            self.defects
                .push("local and authoritative remote file hashes do not agree".into());
        }
        if object_mismatch {
            self.defects
                .push("local and authoritative remote Git blob IDs do not agree".into());
        }

        let clone_corrupt = same_object_different_content
            || !self.clone_health.fsck_passed
            || self.clone_health.clean_reclone_required;
        self.state = if clone_corrupt {
            self.clone_health.clean_reclone_required = true;
            SourceValidationState::CorruptedClone
        } else if content_mismatch || object_mismatch {
            SourceValidationState::Contradicted
        } else if ineligible {
            SourceValidationState::Ineligible
        } else if self.defects.is_empty() {
            SourceValidationState::Validated
        } else {
            SourceValidationState::Pending
        };

        if self.state == SourceValidationState::Validated {
            // Hash a canonical copy without a marker; callers cannot forge this
            // by supplying a serialized marker because it was cleared above.
            let encoded = serde_json::to_vec(self).expect("manifest is serializable");
            self.marker = Some(ValidationMarker {
                finding_id: self.finding_id.clone(),
                manifest_sha256: format!("{:x}", Sha256::digest(encoded)),
                validated_at: now,
            });
        }
        self.state
    }

    pub fn permits_final_output(&self) -> bool {
        if !self.state.permits_final_output() || !self.defects.is_empty() {
            return false;
        }
        let Some(marker) = &self.marker else {
            return false;
        };
        let mut recomputed = self.clone();
        let state = recomputed.evaluate(marker.validated_at);
        state.permits_final_output() && recomputed.marker.as_ref() == Some(marker)
    }

    /// Any authoritative contradiction immediately voids the marker and pauses
    /// publication until a fresh clean-source evaluation succeeds.
    pub fn record_external_contradiction(&mut self, reason: impl Into<String>) {
        self.state = SourceValidationState::Contradicted;
        self.marker = None;
        self.defects.push(reason.into());
    }

    fn observation_defect(
        revision: &str,
        file_path: &str,
        observation: &SourceObservation,
        expected_channel: SourceChannel,
        label: &str,
    ) -> Option<String> {
        if observation.channel != expected_channel
            || observation.source.trim().is_empty()
            || observation.revision != revision
            || observation.file_path != file_path
            || observation.content_sha256.len() != 64
            || !observation
                .content_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            Some(format!(
                "{label} observation is incomplete or not pinned to the manifest file/revision"
            ))
        } else {
            None
        }
    }
}
