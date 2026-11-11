//! Replies a requester would accept that the search contracts' own lists (`contradictions`,
//! `context`, `rejected`, `units`) used to leave unresolved, each beside the nearest reply
//! that must still earn nothing.

mod statement_support;

use mock_core::grading::Outcome;
use statement_support::Case;

/// Neither verdict earns reward; which of the two the reader returns is incidental.
#[track_caller]
fn no_reward(case: &Case, answers: &[&str]) {
    for answer in answers {
        let outcome = case.outcome(answer);
        assert!(
            matches!(outcome, Outcome::Fail | Outcome::Unresolved),
            "answer: {answer}\noutcome: {outcome:?}"
        );
    }
}

#[test]
fn checkout_errors_at_least_is_not_a_claim_about_the_fewest_errors() {
    let case = Case::load("search/elasticsearch/checkout-errors-last-hour");
    case.passes(&[
        "payment-gateway produced the most production 5xx errors in the last hour: 7. Its most frequent error message was \"upstream timeout from acquirer after 30000ms\", which accounts for at least 6 of the 7.",
        "Service: payment-gateway\n5xx count: 7\nTop error message: upstream timeout from acquirer after 30000ms (at least 6 of 7 errors)",
        "payment-gateway had 7 production 5xx errors in the last hour, at least twice as many as any other service. Top message: upstream timeout from acquirer after 30000ms.",
    ]);
    // The right values under the opposite ranking.
    no_reward(
        &case,
        &[
            "payment-gateway had the fewest 5xx errors: 7. Top message: upstream timeout from acquirer after 30000ms.",
            "payment-gateway had the least 5xx errors: 7. Top message: upstream timeout from acquirer after 30000ms.",
            "payment-gateway errored the least with 7 errors. Top message: upstream timeout from acquirer after 30000ms.",
        ],
    );
}
