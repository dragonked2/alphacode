use anyhow::Result;
use std::collections::BTreeMap;
use std::path::PathBuf;

const VALIDATION_STATUS_FILE: &str = "auth-validation.json";

pub use crate::alphacode_auth_types::ProviderValidationRecord;

pub fn status_path() -> Result<PathBuf> {
    Ok(crate::storage::alphacode_dir()?.join(VALIDATION_STATUS_FILE))
}

pub fn load_all() -> BTreeMap<String, ProviderValidationRecord> {
    let Ok(path) = status_path() else {
        return BTreeMap::new();
    };
    crate::storage::read_json(&path).unwrap_or_default()
}

pub fn get(provider_id: &str) -> Option<ProviderValidationRecord> {
    load_all().get(provider_id).cloned()
}

pub fn save(provider_id: &str, record: ProviderValidationRecord) -> Result<()> {
    let mut records = load_all();
    records.insert(provider_id.to_string(), record);
    crate::storage::write_json(&status_path()?, &records)
}

pub fn status_label(provider_id: &str) -> Option<String> {
    get(provider_id).map(|record| format_record_label(&record))
}

pub fn format_record_label(record: &ProviderValidationRecord) -> String {
    let age = age_label(record.checked_at_ms);
    let base = if record.success {
        if record.tool_smoke_ok == Some(true) {
            "runtime + tool validated"
        } else if record.provider_smoke_ok == Some(true) {
            "runtime validated"
        } else {
            "validated"
        }
    } else {
        "validation failed"
    };

    // A cached validation record describes the *last* probe, not the live
    // credential. Once it ages past the staleness window we must not present it
    // as current truth: a long-expired "validation failed" record routinely
    // misled both humans and agents into believing a working credential was
    // broken (e.g. an OAuth token that has since auto-refreshed). Mark stale
    // records explicitly so every surface re-checks instead of trusting them.
    if crate::alphacode_base::auth::doctor::validation_is_stale(record.checked_at_ms) {
        return format!("{} (stale, last checked {}; re-validate)", base, age);
    }

    format!("{} ({})", base, age)
}

fn age_label(checked_at_ms: i64) -> String {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let delta_ms = now_ms.saturating_sub(checked_at_ms).max(0);
    let delta_secs = delta_ms / 1000;
    match delta_secs {
        0..=89 => "just now".to_string(),
        90..=3599 => format!("{}m ago", delta_secs / 60),
        3600..=86_399 => format!("{}h ago", delta_secs / 3600),
        _ => format!("{}d ago", delta_secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_record_label_prefers_tool_validated_wording() {
        let record = ProviderValidationRecord {
            checked_at_ms: chrono::Utc::now().timestamp_millis(),
            success: true,
            provider_smoke_ok: Some(true),
            tool_smoke_ok: Some(true),
            summary: "ok".to_string(),
        };
        assert!(format_record_label(&record).starts_with("runtime + tool validated"));
    }

    #[test]
    fn format_record_label_reports_failures() {
        let record = ProviderValidationRecord {
            checked_at_ms: chrono::Utc::now().timestamp_millis(),
            success: false,
            provider_smoke_ok: Some(false),
            tool_smoke_ok: Some(false),
            summary: "provider smoke failed".to_string(),
        };
        assert!(format_record_label(&record).starts_with("validation failed"));
    }

    #[test]
    fn format_record_label_marks_stale_records() {
        // A record older than the staleness window must not be presented as a
        // current fact: it should be explicitly flagged so surfaces re-validate
        // instead of trusting an outdated pass/fail verdict.
        let stale_ms = chrono::Utc::now().timestamp_millis()
            - crate::alphacode_base::auth::doctor::VALIDATION_STALE_AFTER_MS
            - 1;
        let failed = ProviderValidationRecord {
            checked_at_ms: stale_ms,
            success: false,
            provider_smoke_ok: Some(false),
            tool_smoke_ok: Some(false),
            summary: "auth status is expired".to_string(),
        };
        let label = format_record_label(&failed);
        assert!(label.contains("stale"), "stale label missing: {label}");
        assert!(
            label.contains("re-validate"),
            "stale label should prompt re-validation: {label}"
        );

        let passed = ProviderValidationRecord {
            checked_at_ms: stale_ms,
            success: true,
            provider_smoke_ok: Some(true),
            tool_smoke_ok: Some(true),
            summary: "ok".to_string(),
        };
        let label = format_record_label(&passed);
        assert!(
            label.starts_with("runtime + tool validated"),
            "stale-but-passing record should keep its verdict prefix: {label}"
        );
        assert!(label.contains("stale"), "stale label missing: {label}");
    }
}
