use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Artifact type classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactType {
    #[default]
    SecurityAssessment,
    BugBountyReport,
    CtfWriteup,
    Web3SecurityAssessment,
    ReconnaissanceReport,
    PenetrationTestReport,
    SourceCodeSecurityReview,
    ThreatIntelligenceReport,
    Custom(String),
}

impl ArtifactType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SecurityAssessment => "security-assessment",
            Self::BugBountyReport => "bug-bounty-report",
            Self::CtfWriteup => "ctf-writeup",
            Self::Web3SecurityAssessment => "web3-security-assessment",
            Self::ReconnaissanceReport => "reconnaissance-report",
            Self::PenetrationTestReport => "penetration-test-report",
            Self::SourceCodeSecurityReview => "source-code-security-review",
            Self::ThreatIntelligenceReport => "threat-intelligence-report",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::SecurityAssessment => "Security Assessment",
            Self::BugBountyReport => "Bug Bounty Report",
            Self::CtfWriteup => "CTF Writeup",
            Self::Web3SecurityAssessment => "Web3 Security Assessment",
            Self::ReconnaissanceReport => "Reconnaissance Report",
            Self::PenetrationTestReport => "Penetration Test Report",
            Self::SourceCodeSecurityReview => "Source Code Security Review",
            Self::ThreatIntelligenceReport => "Threat Intelligence Report",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// Classification level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Classification {
    Public,
    Internal,
    #[default]
    Confidential,
    Restricted,
    TopSecret,
}

impl Classification {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Public => "PUBLIC",
            Self::Internal => "INTERNAL",
            Self::Confidential => "CONFIDENTIAL",
            Self::Restricted => "RESTRICTED",
            Self::TopSecret => "TOP SECRET",
        }
    }
}

/// Confidentiality level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidentiality {
    Public,
    #[default]
    Private,
    Secret,
}

impl Confidentiality {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
            Self::Secret => "secret",
        }
    }
}

/// Report status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    #[default]
    Draft,
    InReview,
    Final,
    Published,
    Archived,
}

impl ReportStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Draft => "draft",
            Self::InReview => "in_review",
            Self::Final => "final",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
}

/// Artifact metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub title: String,
    pub subtitle: Option<String>,
    pub artifact_type: ArtifactType,
    pub target: Option<String>,
    pub target_type: Option<String>,
    pub organization: Option<String>,
    pub assessment_type: Option<String>,
    pub author: Option<String>,
    pub assessor: Option<String>,
    pub date: DateTime<Utc>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub version: String,
    pub report_id: String,
    pub agent_version: Option<String>,
    pub alphacode_version: String,
    pub classification: Classification,
    pub confidentiality: Confidentiality,
    pub status: ReportStatus,
    pub tags: Vec<String>,
    pub custom_fields: HashMap<String, serde_json::Value>,
}

impl ArtifactMetadata {
    pub fn new(title: impl Into<String>, artifact_type: ArtifactType) -> Self {
        let now = Utc::now();
        Self {
            title: title.into(),
            subtitle: None,
            artifact_type,
            target: None,
            target_type: None,
            organization: None,
            assessment_type: None,
            author: None,
            assessor: None,
            date: now,
            start_date: None,
            end_date: None,
            version: "1.0.0".to_string(),
            report_id: format!(
                "AC-{}-{:04}",
                now.format("%Y"),
                rand::random::<u16>() % 10000
            ),
            agent_version: None,
            alphacode_version: env!("CARGO_PKG_VERSION").to_string(),
            classification: Classification::default(),
            confidentiality: Confidentiality::default(),
            status: ReportStatus::default(),
            tags: Vec::new(),
            custom_fields: HashMap::new(),
        }
    }

    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn with_target(
        mut self,
        target: impl Into<String>,
        target_type: impl Into<String>,
    ) -> Self {
        self.target = Some(target.into());
        self.target_type = Some(target_type.into());
        self
    }

    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }

    pub fn with_assessment_type(mut self, atype: impl Into<String>) -> Self {
        self.assessment_type = Some(atype.into());
        self
    }

    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    pub fn with_assessor(mut self, assessor: impl Into<String>) -> Self {
        self.assessor = Some(assessor.into());
        self
    }

    pub fn with_dates(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_date = Some(start);
        self.end_date = Some(end);
        self
    }

    pub fn with_classification(mut self, classification: Classification) -> Self {
        self.classification = classification;
        self
    }

    pub fn with_confidentiality(mut self, confidentiality: Confidentiality) -> Self {
        self.confidentiality = confidentiality;
        self
    }

    pub fn with_status(mut self, status: ReportStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_report_id(mut self, report_id: impl Into<String>) -> Self {
        self.report_id = report_id.into();
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_custom_field(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.custom_fields.insert(key.into(), value);
        self
    }
}

/// Artifact manifest for reproducibility tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub artifact_id: String,
    pub artifact_type: ArtifactType,
    pub created_at: DateTime<Utc>,
    pub generator_version: String,
    pub alphacode_version: String,
    pub template_version: String,
    pub schema_version: String,
    pub source_hash: String,
    pub input_files: Vec<String>,
    pub dependencies: HashMap<String, String>,
}

impl ArtifactManifest {
    pub fn new(artifact_type: ArtifactType, source_hash: String) -> Self {
        Self {
            artifact_id: uuid::Uuid::new_v4().to_string(),
            artifact_type,
            created_at: Utc::now(),
            generator_version: env!("CARGO_PKG_VERSION").to_string(),
            alphacode_version: env!("CARGO_PKG_VERSION").to_string(),
            template_version: "1.0.0".to_string(),
            schema_version: "1.0.0".to_string(),
            source_hash,
            input_files: Vec::new(),
            dependencies: HashMap::new(),
        }
    }
}
