use super::*;

fn leak_case() -> AccessCase {
    let mut case = AccessCase::new()
        .with_identity(TestIdentity::new("tester", "tester", vec![Role::Member]))
        .with_resource(ResourceRef::owned_by("document", "private", "owner"));
    for exchange in ["first", "repeat"] {
        case.observations.push(
            AccessObservation::new(
                "tester",
                "document:private",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("controlled-owner-canary")
            .with_exchange(exchange),
        );
    }
    case.observations.push(
        AccessObservation::new(
            "tester",
            "document:control",
            AccessOperation::Read,
            ObservedOutcome::Denied,
        )
        .with_exchange("denied-control"),
    );
    case
}

#[test]
fn a_failed_repeat_cannot_confirm_a_leak() {
    for outcome in [
        ObservedOutcome::Denied,
        ObservedOutcome::Error,
        ObservedOutcome::Empty,
        ObservedOutcome::NotObserved,
    ] {
        let mut case = leak_case();
        case.observations[1].outcome = outcome;
        let assessment = analyze_access(&case, "tester", "document:private");
        assert!(!assessment.is_reportable(), "{outcome:?}: {assessment:?}");
        assert_eq!(assessment.supporting_exchanges, 1);
    }
}

#[test]
fn blank_or_missing_markers_cannot_corroborate_a_leak() {
    for marker in [None, Some(String::new()), Some(" \t".into())] {
        let mut case = leak_case();
        case.observations[1].protected_data_marker = marker;
        assert!(!analyze_access(&case, "tester", "document:private").is_reportable());
    }
}

#[test]
fn denial_for_a_different_operation_is_not_a_read_control() {
    let mut case = leak_case();
    case.observations[2].operation = AccessOperation::Delete;
    let assessment = analyze_access(&case, "tester", "document:private");
    assert!(!assessment.is_reportable());
    assert!(
        assessment
            .unmet_requirements
            .iter()
            .any(|r| r.kind == RequirementKind::EstablishBaseline)
    );
    assert_eq!(analyze_matrix(&case), vec![assessment]);
}

#[test]
fn contradictory_absence_blocks_reporting_until_reconciled() {
    let mut case = leak_case();
    assert!(analyze_access(&case, "tester", "document:private").is_reportable());
    case.confirmed_absent
        .push(("tester".into(), "document:private".into()));
    let assessment = analyze_access(&case, "tester", "document:private");
    assert!(!assessment.is_reportable());
    assert!(
        assessment
            .false_positive_causes
            .contains(&FalsePositiveCause::FixtureInconsistency)
    );
}

fn matrix_case(identities: usize, resources: usize) -> AccessCase {
    let mut case = AccessCase::new();
    for i in 0..identities {
        case.identities.push(TestIdentity::new(
            format!("actor-{i}"),
            format!("actor-{i}"),
            vec![Role::Member],
        ));
    }
    for r in 0..resources {
        let owner = format!("actor-{}", r % identities);
        let mut resource = ResourceRef::owned_by("document", r.to_string(), owner);
        if r % 7 == 0 {
            resource.visibility = ResourceVisibility::Public;
        }
        let label = resource.report_label();
        case.resources.push(resource);
        for i in 0..identities {
            for repeat in 0..2 {
                let outcome = if (i + r) % 3 == 0 {
                    ObservedOutcome::Denied
                } else {
                    ObservedOutcome::Returned
                };
                case.observations.push(
                    AccessObservation::new(
                        format!("actor-{i}"),
                        &label,
                        AccessOperation::Read,
                        outcome,
                    )
                    .with_marker("owned-fixture-canary")
                    .with_exchange(format!("{i}-{r}-{repeat}")),
                );
            }
        }
    }
    case
}

fn scalar_matrix(case: &AccessCase) -> Vec<AuthorizationAssessment> {
    case.resources
        .iter()
        .flat_map(|resource| {
            case.identities
                .iter()
                .map(|identity| analyze_access(case, &identity.id, &resource.report_label()))
        })
        .collect()
}

#[test]
fn indexed_matrix_preserves_complete_scalar_results_and_order() {
    let mut case = matrix_case(8, 30);
    case.identities.push(case.identities[0].clone());
    case.resources.push(ResourceRef::public_("document", "1"));
    case.identities[1].authenticated = false;
    case.matrix.expect_allowed("actor-2", "document:1");
    case.confirmed_absent
        .push(("actor-3".into(), "document:2".into()));
    assert_eq!(analyze_matrix(&case), scalar_matrix(&case));
    // Mutated evidence must never reuse an index from an earlier analysis.
    case.observations.clear();
    assert_eq!(analyze_matrix(&case), scalar_matrix(&case));
}

#[test]
#[ignore = "manual timing experiment; measures local matrix analysis, not discovery"]
fn matrix_timing_experiment() {
    use std::hint::black_box;
    use std::time::Instant;
    let case = matrix_case(20, 200);
    assert_eq!(analyze_matrix(&case), scalar_matrix(&case));
    let mut scalar = Vec::new();
    let mut indexed = Vec::new();
    for iteration in 0..7 {
        let mut measure = |fast| {
            let started = Instant::now();
            if fast {
                black_box(analyze_matrix(black_box(&case)));
            } else {
                black_box(scalar_matrix(black_box(&case)));
            }
            let seconds = started.elapsed().as_secs_f64();
            if fast {
                indexed.push(seconds);
            } else {
                scalar.push(seconds);
            }
        };
        measure(iteration % 2 == 0);
        measure(iteration % 2 != 0);
    }
    scalar.sort_by(f64::total_cmp);
    indexed.sort_by(f64::total_cmp);
    println!(
        "matrix pairs=4000 observations={} trials=7 scalar_median_seconds={} indexed_median_seconds={} median_speedup={} scalar_max_seconds={} indexed_max_seconds={}",
        case.observations.len(),
        scalar[3],
        indexed[3],
        scalar[3] / indexed[3],
        scalar[6],
        indexed[6]
    );
}
