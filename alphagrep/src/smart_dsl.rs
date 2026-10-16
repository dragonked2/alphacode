//! The trace DSL: a handful of `field:value` terms describing what to trace.
//!
//! Recognised fields are `subject` (what to find), `relation` (how it is
//! connected), `support` (extra ranking terms), `path` (scope), and `kind`
//! (declaration kind). Bare terms are subjects, `-term`/`!term` exclude, and an
//! unknown `field:value` pair is kept verbatim as a ranking term so that paths
//! such as `src/lib.rs:12` never fail to parse.

/// Parsed trace query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmartQuery {
    /// Terms exactly as supplied.
    pub raw: Vec<String>,
    /// Subjects: what to find.
    pub subjects: Vec<String>,
    /// Relations: how the subject is connected to the code.
    pub relations: Vec<String>,
    /// Supporting terms that boost ranking without defining the subject.
    pub supports: Vec<String>,
    /// Path prefix filters.
    pub paths: Vec<String>,
    /// Declaration kind filters (`fn`, `class`, ...).
    pub kinds: Vec<String>,
    /// Terms that must not appear.
    pub exclude: Vec<String>,
    /// Unrecognised terms, kept as subjects.
    pub free: Vec<String>,
}

impl SmartQuery {
    /// Whether the query has any positive term to search for.
    pub fn is_empty(&self) -> bool {
        self.positive_terms().is_empty()
    }

    /// Subjects plus free terms, the terms a file must match to be considered.
    pub fn positive_terms(&self) -> Vec<String> {
        let mut terms = self.subjects.clone();
        terms.extend(self.free.clone());
        terms
    }

    /// Compact description used in notes and debug output.
    pub fn describe(&self) -> String {
        format!(
            "subject={:?} relation={:?} support={:?} path={:?} kind={:?} exclude={:?}",
            self.subjects, self.relations, self.supports, self.paths, self.kinds, self.exclude
        )
    }
}

/// Parse DSL terms into a [`SmartQuery`].
pub fn parse_smart_query(terms: &[String]) -> Result<SmartQuery, String> {
    let mut query = SmartQuery::default();

    for raw_term in terms {
        let term = raw_term.trim();
        if term.is_empty() {
            continue;
        }
        query.raw.push(term.to_string());

        if let Some(rest) = term.strip_prefix('-').or_else(|| term.strip_prefix('!')) {
            let rest = rest.trim();
            if !rest.is_empty() {
                query.exclude.push(rest.to_string());
            }
            continue;
        }

        let Some((field, value)) = term.split_once(':') else {
            query.subjects.push(term.to_string());
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            return Err(format!("trace term {term:?} has no value after ':'"));
        }
        match field.trim().to_ascii_lowercase().as_str() {
            "subject" | "subj" | "term" | "text" => query.subjects.push(value.to_string()),
            "relation" | "rel" => query.relations.push(value.to_string()),
            "support" | "supports" | "supporting" => query.supports.push(value.to_string()),
            "path" | "file" | "under" => query.paths.push(value.to_string()),
            "kind" | "symbol" => query.kinds.push(value.to_string()),
            // Unknown fields stay searchable: an agent may reasonably write
            // `crate:auth`, and silently dropping the term would be worse than
            // treating it as a literal subject.
            _ => query.free.push(term.to_string()),
        }
    }

    if query.subjects.is_empty()
        && query.free.is_empty()
        && query.supports.is_empty()
        && query.paths.is_empty()
        && query.kinds.is_empty()
    {
        return Err(
            "trace query needs at least one subject, support, path, or kind term".to_string(),
        );
    }
    Ok(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn fields_and_bare_terms_are_separated() {
        let query = parse_smart_query(&terms(&[
            "subject:auth_status",
            "relation:rendered",
            "support:ui",
            "path:src/tui",
            "kind:fn",
            "auth",
        ]))
        .expect("query");
        assert_eq!(query.subjects, vec!["auth_status", "auth"]);
        assert_eq!(query.relations, vec!["rendered"]);
        assert_eq!(query.supports, vec!["ui"]);
        assert_eq!(query.paths, vec!["src/tui"]);
        assert_eq!(query.kinds, vec!["fn"]);
    }

    #[test]
    fn exclusions_are_recognised() {
        let query =
            parse_smart_query(&terms(&["subject:auth", "-legacy", "!deprecated"])).expect("query");
        assert_eq!(query.exclude, vec!["legacy", "deprecated"]);
    }

    #[test]
    fn unknown_fields_stay_searchable() {
        let query = parse_smart_query(&terms(&["crate:auth", "src/lib.rs:12"])).expect("query");
        assert!(query.subjects.is_empty());
        assert_eq!(query.free, vec!["crate:auth", "src/lib.rs:12"]);
        assert!(!query.is_empty());
    }

    #[test]
    fn empty_values_are_rejected() {
        let error = parse_smart_query(&terms(&["subject:"])).expect_err("should fail");
        assert!(error.contains("has no value"));
    }

    #[test]
    fn a_query_without_positive_terms_is_rejected() {
        let error = parse_smart_query(&terms(&["relation:rendered"])).expect_err("should fail");
        assert!(error.contains("needs at least one subject"));
    }

    #[test]
    fn describe_lists_every_bucket() {
        let query = parse_smart_query(&terms(&["subject:auth"])).expect("query");
        let description = query.describe();
        assert!(description.contains("subject=[\"auth\"]"));
        assert!(description.contains("relation=[]"));
    }
}
