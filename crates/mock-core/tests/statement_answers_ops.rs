//! Counterexamples for statement-mode answer contracts: ops scenarios.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

#[test]
fn sentry_top_unresolved_issue_is_ranked_from_the_authored_event_counts() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    case.reference_is_source_bound();
    case.passes(&[
        "CHECKOUT-API-3F - OperationalError: could not obtain connection from pool within 5.0s",
        "Short ID: `checkout-api-3f`\nTitle: \"OperationalError: could not obtain connection from pool within 5.0s\"",
        "| Short ID | Title |\n|---|---|\n| CHECKOUT-API-3F | OperationalError: could not obtain connection from pool within 5.0s |",
        // A polite hedge still states the answer.
        "I think it is CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s.",
        // A true aside after the answer sets the ignored issue aside.
        "CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s. CHECKOUT-API-2R has more events but is ignored.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "It is not CHECKOUT-API-3F (OperationalError: could not obtain connection from pool within 5.0s).",
        "Rather than CHECKOUT-API-3F (OperationalError: could not obtain connection from pool within 5.0s), the noisiest issue is CHECKOUT-API-2R.",
        // The ignored and resolved issues with more events, and the issue of
        // another project that carries the very same title.
        "CHECKOUT-API-2R: DisallowedHost: Invalid HTTP_HOST header: '10.4.2.17:8000'",
        "CHECKOUT-API-31: TimeoutError: inventory-service did not respond within 3.0s",
        "PAYMENTS-WORKER-1K: OperationalError: could not obtain connection from pool within 5.0s",
        // The right issue with another issue's title, and without any title.
        "CHECKOUT-API-3F: KeyError: 'shipping_address'",
        "CHECKOUT-API-3F",
        // A near-miss identifier.
        "CHECKOUT-API-3: OperationalError: could not obtain connection from pool within 5.0s",
        // A denial across a label noun.
        "The noisiest issue is not issue CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s",
        "The noisiest issue is not the issue CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s",
        "It is not the issue CHECKOUT-API-3F (OperationalError: could not obtain connection from pool within 5.0s).",
    ]);
    case.unresolved(&[
        // A paraphrased title is not the title.
        "CHECKOUT-API-3F: an OperationalError about the connection pool",
        // A negative predicate about the right issue is a dispute, not a statement.
        "CHECKOUT-API-3F (OperationalError: could not obtain connection from pool within 5.0s) is not the noisiest issue.",
        "CHECKOUT-API-3F (OperationalError: could not obtain connection from pool within 5.0s) does not have the most events.",
    ]);
    // Denying a distractor leaves the answer intact.
    case.passes(&[
        "CHECKOUT-API-3F, titled OperationalError: could not obtain connection from pool within 5.0s; not CHECKOUT-API-2R, which is ignored.",
    ]);
    // The ranking follows the authored counts and statuses.
    let count = "/services/sentry/issues/CHECKOUT-API-3C/count";
    assert_eq!(
        case.outcome_after(count, json!("5000"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            count,
            json!("5000"),
            "CHECKOUT-API-3C: KeyError: 'shipping_address'"
        ),
        Outcome::Pass
    );
    // Once the ignored issue with more events is unresolved, it is the one to name. The
    // resolved issue is ignored in its place: the contract's reviewed aside reads the
    // project's one ignored issue, so a state without any is an environment error.
    let mut reopened = case.initial.clone();
    for (issue, status) in [
        ("CHECKOUT-API-2R", "unresolved"),
        ("CHECKOUT-API-31", "ignored"),
    ] {
        *reopened
            .pointer_mut(&format!("/services/sentry/issues/{issue}/status"))
            .unwrap() = json!(status);
    }
    assert_eq!(
        case.grade_with(&reopened, &case.reference).outcome,
        Outcome::Fail
    );
    assert_eq!(
        case.grade_with(
            &reopened,
            "CHECKOUT-API-2R: DisallowedHost: Invalid HTTP_HOST header: '10.4.2.17:8000'"
        )
        .outcome,
        Outcome::Pass
    );
}

/// The task asks which one issue has the most events; a reply that names two issues as
/// joint leaders and never chooses lists them and does not answer.
#[test]
fn sentry_two_issues_named_as_joint_leaders_do_not_answer_which_is_noisiest() {
    let case = Case::load("ops/sentry/top-unresolved-checkout");
    case.unresolved(&[
        "CHECKOUT-API-3F (OperationalError: could not obtain connection from pool within 5.0s) and CHECKOUT-API-3C are the two noisiest.",
    ]);
}

#[test]
fn sentry_release_is_traced_to_its_pull_request_and_author_by_joins() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    case.reference_is_source_bound();
    case.passes(&[
        "Issue STOREFRONT-WEB-7Q first appeared in release storefront@5.2.0. It was introduced by PR #57, authored by Sofia Alvarez.",
        "| Sentry issue | STOREFRONT-WEB-7Q |\n| First release | storefront@5.2.0 |\n| Pull request | #57 |\n| Author | sofia |",
        "storefront-web-7q, storefront@5.2.0, pull request 57 by @sofia",
        "STOREFRONT-WEB-7Q, storefront@5.2.0, PR #57 (https://github.com/lumen-labs/storefront/pull/57), Sofia Alvarez",
        // A polite hedge still states the answer, and a number may be spelled out.
        "It is probably STOREFRONT-WEB-7Q, first seen in storefront@5.2.0, from PR #57 by Sofia Alvarez.",
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; pull request fifty-seven by Sofia Alvarez.",
        // The open fix attempt, named after the answer and as what it is, is context.
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #57 by Sofia Alvarez. PR #70 by marcus is an open fix.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "The issue is not STOREFRONT-WEB-7Q; release storefront@5.2.0, PR #57 by Sofia Alvarez.",
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0. The pull request is not PR #57 by Sofia Alvarez.",
        "STOREFRONT-WEB-7Q, storefront@5.2.0. Rather than #57 by Sofia Alvarez, the pull request is #63.",
        "STOREFRONT-WEB-7Q, storefront@5.2.0. Rather than PR #57 (Sofia Alvarez) I would blame another change.",
        "STOREFRONT-WEB-7Q is the issue, but not in release storefront@5.2.0; PR #57 by Sofia Alvarez.",
        "STOREFRONT-WEB-7Q, storefront@5.2.0, not pull request 57, Sofia Alvarez.",
        "STOREFRONT-WEB-7Q, storefront@5.2.0, instead of pull request #57 it was #63, Sofia Alvarez.",
        "STOREFRONT-WEB-7Q did not first appear in storefront@5.2.0; PR #57, Sofia Alvarez.",
        // The look-alike issue with its own release, pull request and author.
        "STOREFRONT-WEB-8D first appeared in storefront@5.3.0; PR #63 by Kenji Watanabe.",
        // The right issue with the look-alike's release, the open fix attempt
        // instead of the introducing pull request, and the wrong author.
        "STOREFRONT-WEB-7Q first appeared in storefront@5.3.0; PR #57 by Sofia Alvarez.",
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #70 by Sofia Alvarez.",
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #57 by Marcus Chen.",
        // Near-miss pull request number.
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #75 by Sofia Alvarez.",
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #570 by Sofia Alvarez.",
    ]);
    case.unresolved(&[
        // A bare number is not a pull request reference.
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; 57, Sofia Alvarez.",
        // A partial author name and a partial release name.
        "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #57 by S. Alvarez.",
        "STOREFRONT-WEB-7Q first appeared in storefront 5.2.0; PR #57 by Sofia Alvarez.",
        // A negated verb after the author.
        "STOREFRONT-WEB-7Q, storefront@5.2.0, PR #57. Sofia Alvarez did not write it.",
        // The right issue under the opposite ranking.
        "The quietest issue is STOREFRONT-WEB-7Q, first seen in storefront@5.2.0; PR #57 by Sofia Alvarez.",
    ]);
    // The chain follows the authored data: a busier issue changes every fact,
    // and a different pull request author changes only the author.
    let count = "/services/sentry/issues/STOREFRONT-WEB-8D/count";
    assert_eq!(
        case.outcome_after(count, json!("9000"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            count,
            json!("9000"),
            "STOREFRONT-WEB-8D first appeared in storefront@5.3.0; PR #63 by Kenji Watanabe."
        ),
        Outcome::Pass
    );
    let author = "/services/github/pulls/lumen-labs~1storefront#57/user";
    assert_eq!(
        case.outcome_after(author, json!("lena"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            author,
            json!("lena"),
            "STOREFRONT-WEB-7Q first appeared in storefront@5.2.0; PR #57 by Lena Weber."
        ),
        Outcome::Pass
    );
}

/// Notations the reader does not accept. Each is safe (never a pass for a
/// wrong answer) but stricter than the keyword predicate the contract replaces.
#[test]
fn pull_request_references_without_a_standalone_number_never_pass() {
    let case = Case::load("ops/multi/sentry-release-commit-lookup");
    // Digits inside a URL path or a hyphenated compound are not read as a number.
    case.fails(&[
        "STOREFRONT-WEB-7Q, storefront@5.2.0, https://github.com/lumen-labs/storefront/pull/57, Sofia Alvarez",
        "STOREFRONT-WEB-7Q, storefront@5.2.0, PR-57, Sofia Alvarez",
    ]);
}

#[test]
fn datadog_peak_is_the_highest_production_point_inside_the_hour() {
    let case = Case::load("ops/datadog/checkout-p95-peak");
    case.reference_is_source_bound();
    case.passes(&[
        "Peak: 1843 ms at 14:35 UTC.",
        "| Peak p95 | 1,843 ms |\n| Time (UTC) | 14:35 |",
        "The highest p95 was 1.843 s, reached at 2026-10-29T14:35:00Z.",
        "1843 milliseconds at 14:35:00 UTC",
        "1843 ms at 14:35 UTC, not the 2210 ms staging peak.",
        // A polite hedge still states the answer, and a 12-hour clock time is read.
        "It probably peaked at 1843 ms at 14:35 UTC.",
        "1843 ms at 2:35 PM UTC",
        // Other points named after the peak, as what they are, are context.
        "1843 ms at 14:35 UTC (outside the window production reached 2950 ms at 13:30).",
        "1843 ms at 14:35 UTC; five minutes later it was still 1701 ms at 14:40.",
        // A negated verb that bounds the peak still states it. The reader does not tell
        // this from "did not reach 1843 ms", which passes too: see
        // `negated-verb-before-measured-value` in the wrong-answer fixtures.
        "Production did not exceed 1843 ms, reached at 14:35 UTC.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "The peak was not 1843 ms and it was not at 14:35.",
        "Rather than 1843 ms at 14:35, the peak was 2210 ms at 14:15.",
        // The staging peak, production just outside the hour, and another service.
        "2210 ms at 14:15 UTC",
        "2950 ms at 13:30 UTC",
        "2400 ms at 15:15 UTC",
        "2120 ms at 14:20 UTC",
        // The right value at another point's time, and another point's value
        // at the right time.
        "1843 ms at 14:15 UTC",
        "1843 ms at 13:30 UTC",
        "1701 ms at 14:35 UTC",
        // Near misses.
        "1834 ms at 14:35 UTC",
        "1843 ms at 14:53 UTC",
    ]);
    case.unresolved(&[
        // A rounded value in words is not the peak.
        "about eighteen hundred ms at 14:35",
        // A value without its unit is not read.
        "1843 at 14:35 UTC",
        // The right point under the opposite ranking.
        "The lowest p95 of the hour was 1843 ms at 14:35 UTC.",
    ]);
    // The peak follows the authored points of the hour, and only those.
    let series =
        "/services/datadog/metrics/checkout.api.latency.p95{env:prod,service:checkout-api}";
    let inside = format!("{series}/points/17/1");
    assert_eq!(
        case.outcome_after(&inside, json!(1900), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&inside, json!(1900), "1900 ms at 14:25 UTC"),
        Outcome::Pass
    );
    let outside = format!("{series}/points/3/1");
    assert_eq!(
        case.outcome_after(&outside, json!(5000), &case.reference),
        Outcome::Pass
    );
}

#[test]
fn kubernetes_crashloop_error_is_read_from_the_log_joined_by_record_key() {
    let case = Case::load("ops/kubernetes/crashloop-root-cause");
    case.reference_is_source_bound();
    case.passes(&[
        "Pod: ledger-worker-zx4mrndj8b-gdg8j\nError: pq: password authentication failed for user \"ledger\"",
        "`ledger-worker-zx4mrndj8b-gdg8j` is crash-looping. Its previous container log ends with:\n```\n2026-11-03T08:33:44.302Z ERROR db: connect ledger-db.payments.svc:5432: pq: password authentication failed for user \"ledger\"\n2026-11-03T08:33:44.302Z FATAL could not initialise ledger store, exiting (exit code 1)\n```",
        // Reviewed negative predicates of a diagnosis do not deny the pod.
        "ledger-worker-zx4mrndj8b-gdg8j cannot authenticate to Postgres: password authentication failed for user \"ledger\".",
        "ledger-worker-zx4mrndj8b-gdg8j never becomes ready: password authentication failed for user \"ledger\".",
        // A polite hedge still states the answer.
        "It is probably ledger-worker-zx4mrndj8b-gdg8j: password authentication failed for user \"ledger\".",
        // The healthy sibling, named after the answer and as what it is, is context.
        "ledger-worker-zx4mrndj8b-gdg8j: password authentication failed for user \"ledger\". The other replica ledger-worker-zx4mrndj8b-dwvg9 is healthy.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "The pod is not ledger-worker-zx4mrndj8b-gdg8j; password authentication failed for user \"ledger\".",
        "Rather than ledger-worker-zx4mrndj8b-gdg8j (password authentication failed for user \"ledger\"), look at ledger-worker-c64m9bbw8-v45ss.",
        "ledger-worker-zx4mrndj8b-gdg8j is fine; the error is not password authentication failed for user \"ledger\".",
        // The staging crash-looper with its own panic, and the healthy replica.
        "ledger-worker-c64m9bbw8-v45ss: panic: invalid config: unknown field \"retryBackoffMs\" in kafka section",
        "ledger-worker-zx4mrndj8b-dwvg9: password authentication failed for user \"ledger\"",
        // The right error on the staging pod, and the staging panic or an
        // invented cause on the right pod.
        "ledger-worker-c64m9bbw8-v45ss: password authentication failed for user \"ledger\"",
        "ledger-worker-zx4mrndj8b-gdg8j: panic: invalid config: unknown field \"retryBackoffMs\" in kafka section",
        "ledger-worker-zx4mrndj8b-gdg8j: OOMKilled",
    ]);
    case.unresolved(&[
        // A truncated pod name, the workload name alone, and a shortened or
        // unquoted error are not the authored values.
        "ledger-worker-zx4mrndj8b-gdg8: password authentication failed for user \"ledger\"",
        "ledger-worker: password authentication failed for user \"ledger\"",
        "ledger-worker-zx4mrndj8b-gdg8j: password authentication failed",
        "ledger-worker-zx4mrndj8b-gdg8j: password authentication failed for user ledger",
        // Unreviewed negative predicates about the right pod.
        "ledger-worker-zx4mrndj8b-gdg8j does not crash-loop: password authentication failed for user \"ledger\".",
        "ledger-worker-zx4mrndj8b-gdg8j is not the crash-looping pod: password authentication failed for user \"ledger\".",
    ]);
    // The error follows the authored log of the selected pod.
    let line = "/services/kubernetes/pod_logs/payments~1ledger-worker-zx4mrndj8b-gdg8j~1ledger-worker/previous/2";
    let edited = json!(
        "2026-11-03T08:33:44.302Z ERROR db: connect ledger-db.payments.svc:5432: pq: SSL required"
    );
    assert_eq!(
        case.outcome_after(line, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            line,
            edited,
            "ledger-worker-zx4mrndj8b-gdg8j: pq: SSL required"
        ),
        Outcome::Pass
    );
    // The pod follows the authored status: once the container runs again no
    // pod matches, which is an environment fault rather than a score.
    assert_eq!(
        case.outcome_after(
            "/services/kubernetes/pods/payments~1ledger-worker-zx4mrndj8b-gdg8j/status/containerStatuses/0/state",
            json!({"running": {"startedAt": "2026-11-03T08:39:00Z"}}),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
}

#[test]
fn grafana_top_5xx_pod_is_ranked_within_the_production_datasource() {
    let case = Case::load("ops/grafana/top-5xx-checkout-pod");
    case.reference_is_source_bound();
    case.passes(&[
        "checkout-7b9d6c5f4-xk2lp",
        "Pod to cordon: `checkout-7b9d6c5f4-xk2lp` (~3.5 5xx/s, roughly 8% of its requests).",
        // A polite hedge still states the answer.
        "It is probably checkout-7b9d6c5f4-xk2lp.",
        // The busier staging pod, named after the answer and as staging, is context.
        "checkout-7b9d6c5f4-xk2lp (3.5/s); in staging checkout-5f6d7c8b9-zq4wd is at 4.5/s.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the name.
        "It is not checkout-7b9d6c5f4-xk2lp.",
        "Rather than checkout-7b9d6c5f4-xk2lp, cordon checkout-7b9d6c5f4-m8vqt.",
        // The staging pod has the highest 5xx rate overall, but it is not
        // production; the other production pods are healthy.
        "checkout-5f6d7c8b9-zq4wd is returning the most 5xx responses (4.5/s).",
        "checkout-7b9d6c5f4-m8vqt is returning the most 5xx responses.",
        // A near-miss pod name.
        "checkout-7b9d6c5f4-xk2lq",
    ]);
    case.unresolved(&[
        // The right pod under the opposite ranking, and a fragment of its name.
        "checkout-7b9d6c5f4-xk2lp has the fewest 5xx responses.",
        "The xk2lp checkout pod.",
    ]);
    // The ranking follows the authored rates: once another production pod
    // returns more 5xx responses, it is the pod to name.
    let rate = "/services/grafana/prom_series/prom-prod~117/rate";
    assert_eq!(
        case.outcome_after(rate, json!(9.5), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(rate, json!(9.5), "checkout-7b9d6c5f4-m8vqt"),
        Outcome::Pass
    );
}
