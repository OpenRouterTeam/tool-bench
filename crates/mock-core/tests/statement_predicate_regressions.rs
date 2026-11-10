mod statement_support;

use statement_support::Case;

#[test]
fn latency_region_unreviewed_relationship_must_not_pass() {
    let case = Case::load("search/elasticsearch/latency-by-region");

    case.unresolved(&[
        "The region with the best performance is eu-west-1 at 412.5 ms.",
        "eu-west-1 has the shortest average latency at 412.5 ms.",
        "eu-west-1 leads with the minimum average of 412.5 ms.",
        "eu-west-1 has the least latency, averaging 412.5 ms.",
        "eu-west-1 wins on speed at 412.5 ms.",
    ]);

    case.unresolved(&[
        "eu-west-1 is the fastest at 412.5 ms.",
        "eu-west-1 is the quickest region at 412.5 ms.",
    ]);

    case.passes(&[
        "eu-west-1 is the slowest, averaging 412.5 ms.",
        "Slowest region: eu-west-1\nAverage duration: 412.5 ms",
        "The region with the largest average latency is eu-west-1 at 412.5 ms.",
    ]);

    case.passes(&["eu-west-1 averages 412.5 ms for successful /checkout/submit requests."]);
}

#[test]
fn sentry_reference_stays_valid_after_predicate_hardening() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");

    let reference = case.reference.clone();
    case.passes(&[&reference]);
}

#[test]
fn mongodb_reference_stays_valid_after_predicate_hardening() {
    let case = Case::load("cloud/mongodb/top-category-revenue");

    let reference = case.reference.clone();
    case.passes(&[&reference]);
}

#[test]
fn multi_sentence_evasion_does_not_pass() {
    let case = Case::load("search/elasticsearch/latency-by-region");

    case.unresolved(&["The fastest region overall is eu-west-1, with an average of 412.5 ms."]);

    // A remark about something else neither denies nor replaces the answer.
    case.passes(&["eu-west-1 averages 412.5 ms. It has the best uptime too."]);

    case.unresolved(&[
        "eu-west-1 averages 412.5 ms. It wins on speed.",
        "eu-west-1: 412.5 ms. This result proves every other region is worse.",
    ]);
}

#[test]
fn reviewed_clauses_keep_conversions_and_structured_role_binding() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.passes(&[
        "eu-west-1 averages 0.4125 seconds.",
        "eu-west-1 averages 412.5 milliseconds.",
        "| Region | Average duration |\n| eu-west-1 | 0.4125 s |",
        "| Region | Average duration |\n|---|---|\n| eu-west-1 | 412.5 ms |",
        r#"{"region":"eu-west-1","average":"0.4125 s"}"#,
    ]);
    case.unresolved(&[
        r#"{"fastest_region":"eu-west-1","average":"412.5 ms"}"#,
        "| Fastest region | Average duration |\n| eu-west-1 | 412.5 ms |",
        "eu-west-1: 412.5 ms != the result.",
        "eu-west-1: 412.5 ms ≠",
    ]);
}
