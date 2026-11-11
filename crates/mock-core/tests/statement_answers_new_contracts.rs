//! Focused tests for the seven newly migrated statement-mode contracts.
//!
//! Each test verifies: reference answer passes, independently worded valid
//! answers pass (a polite hedge included), negated/wrong-entity/near-miss
//! answers earn no reward, missing source is an environment error, and a data
//! edit shows the fact follows the source.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
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

// ---- stripe customer-mrr ----

#[test]
fn stripe_mrr_binds_to_active_subscriptions_with_price_normalization() {
    let case = Case::load("business/stripe/customer-mrr");
    case.reference_is_source_bound();
    case.passes(&[
        "The MRR is $439.00.",
        "Harbor & Pine Coffee Co. has a monthly recurring revenue of $439.",
        "MRR: 439 USD",
        // A polite hedge still states the answer.
        "I think the MRR may be around $439.",
    ]);
    case.fails(&[
        // Wrong amount
        "MRR is $339.00.",
        "MRR is $100.00.",
        // Negated
        "The MRR is not $439.00.",
        // Empty
        "",
        // Look-alike customer's MRR
        "MRR is $252.50.",
    ]);
}

#[test]
fn stripe_mrr_follows_authored_prices() {
    let case = Case::load("business/stripe/customer-mrr");
    assert_eq!(
        case.outcome_after(
            "/services/stripe/prices/price_Fco47w3JW6xNqrPtsNH7Qaxm/unit_amount",
            json!(240000),
            "MRR is $439.00."
        ),
        Outcome::Fail,
    );
    assert_eq!(
        case.outcome_after(
            "/services/stripe/prices/price_Fco47w3JW6xNqrPtsNH7Qaxm/unit_amount",
            json!(240000),
            "MRR is $539.00."
        ),
        Outcome::Pass,
    );
}

// ---- supabase-slack referral signups ----

#[test]
fn supabase_referral_binds_customer_count_and_order_total() {
    let case = Case::load("cloud/multi/supabase-slack-referral-signups");
    case.reference_is_source_bound();
    case.passes(&[
        "4 customers, $410.50 total spend.",
        "There are 4 FALL26 referral sign-ups and they have spent $410.50.",
    ]);
    case.fails(&[
        // Wrong count (includes QA)
        "6 customers, $410.50 total.",
        // Wrong total (includes cancelled order)
        "4 customers, $450.50 total.",
        // Negated
        "There are not 4 customers.",
        "",
    ]);
}

// ---- elasticsearch checkout errors ----

#[test]
fn es_checkout_errors_binds_service_count_and_top_message() {
    let case = Case::load("search/elasticsearch/checkout-errors-last-hour");
    case.reference_is_source_bound();
    case.passes(&[
        "payment-gateway: 7 errors. Top message: upstream timeout from acquirer after 30000ms (6).",
        "payment-gateway had 7 5xx errors; the most common was \"upstream timeout from acquirer after 30000ms\" with 6 occurrences.",
        // The task does not ask how often the top message occurred: that count is optional.
        "payment-gateway: 7 errors. Top message: upstream timeout from acquirer after 30000ms.",
        "payment-gateway had 7 5xx errors in the last hour. Its most frequent error message was \"upstream timeout from acquirer after 30000ms\".",
        // The other services' counts, named beside the answer, are context.
        "payment-gateway: 7 errors (cart-api had 3, inventory 2). Top message: upstream timeout from acquirer after 30000ms.",
    ]);
    case.fails(&[
        // Wrong service
        "cart-api: 7 errors.",
        "cart-api: 7 errors. Top message: upstream timeout from acquirer after 30000ms.",
        // Wrong count
        "payment-gateway: 12 errors.",
        "payment-gateway: 6 errors. Top message: upstream timeout from acquirer after 30000ms.",
        // The top message is still required, and it is the service's most frequent one.
        "payment-gateway: 7 errors.",
        "payment-gateway: 7 errors. Top message: tokenization service unavailable (1).",
        // Negated
        "payment-gateway did not have the most errors.",
        "",
    ]);
}

// ---- pagerduty payments oncall ----

#[test]
fn pd_payments_oncall_binds_override_and_backup() {
    let case = Case::load("ops/pagerduty/payments-oncall-now");
    case.reference_is_source_bound();
    case.passes(&[
        "Priya Raman and Marcus Chen.",
        "Level 1: Priya Raman. Level 2: Marcus Chen.",
        "On call: Priya Raman, backup: Marcus Chen.",
        "Priya Raman is on call right now (she is covering Riya Patel via a schedule override); Marcus Chen is next in line.",
    ]);
    // The two people in each other's role.
    no_reward(
        &case,
        &[
            "Marcus Chen and Priya Raman.",
            "Level 1: Marcus Chen. Level 2: Priya Raman.",
            "Marcus Chen is the first responder; Priya Raman is next in line.",
        ],
    );
    case.fails(&[
        // Scheduled person instead of override
        "Riya Patel and Marcus Chen.",
        // Wrong backup
        "Priya Raman and Jordan Alvarez.",
        // Negated
        "Priya Raman is not on call.",
        "",
    ]);
}

// ---- slack-pagerduty dbre oncall ----

#[test]
fn pd_dbre_oncall_binds_rotation_and_backup() {
    let case = Case::load("collab/multi/slack-pagerduty-dbre-oncall");
    case.reference_is_source_bound();
    case.passes(&[
        "Dana Nguyen and Sam Okafor.",
        "Level 1: Dana Nguyen. Level 2: Sam Okafor.",
        "Dana Nguyen is first on call and Sam Okafor is the backup.",
    ]);
    case.fails(&[
        // Negated
        "Dana Nguyen is not on call.",
        "",
    ]);
    // The two people in each other's role: the contract declares the order in which a
    // reply first states them, and rejects the swapped labels outright.
    no_reward(
        &case,
        &[
            "Sam Okafor and Dana Nguyen.",
            "First on call is Sam Okafor, with Dana Nguyen as the backup.",
            "Level 1: Sam Okafor. Level 2: Dana Nguyen.",
        ],
    );
}

#[test]
fn oncall_resolution_uses_the_business_instant_not_its_printed_local_date() {
    let case = Case::load("collab/multi/slack-pagerduty-dbre-oncall");
    assert_eq!(
        case.outcome_after("/now", json!("2026-10-31T04:00:00+14:00"), &case.reference),
        Outcome::Pass,
    );
    assert_eq!(
        case.outcome_after("/now", json!("2026-02-30T14:00:00Z"), &case.reference),
        Outcome::EnvironmentError,
    );
}

// ---- crashloop oncall lookup ----

#[test]
fn crashloop_binds_pod_error_and_oncall() {
    let case = Case::load("ops/multi/crashloop-oncall-lookup");
    case.reference_is_source_bound();
    case.passes(&[
         "Pod ledger-worker-zx4mrndj8b-gdg8j, error: password authentication failed for user. On call: Priya Raman, backup Marcus Chen.",
    ]);
    case.fails(&[
        // Missing error
        "Pod ledger-worker-zx4mrndj8b-gdg8j. Priya Raman, Marcus Chen is on call.",
        "",
    ]);
    // The two people in each other's role.
    no_reward(
        &case,
        &[
            "Pod ledger-worker-zx4mrndj8b-gdg8j, error: password authentication failed for user. On call: Marcus Chen, backup Priya Raman.",
        ],
    );
}

// ---- brave caterer saturday hours ----

#[test]
fn brave_caterer_binds_phone_and_hours() {
    let case = Case::load("search/brave/caterer-saturday-hours");
    case.reference_is_source_bound();
    case.passes(&[
        "Phone: 503-555-0142. Saturday: 09:00-14:00.",
        "(503) 555-0142, Saturday hours 9am-2pm.",
        "+1 503-555-0142, Saturday 9 am to 2 pm.",
        // A polite hedge still states the answer.
        "I think the phone may be 503-555-0142, Saturday 09:00-14:00.",
    ]);
    case.fails(&[
        // Wrong phone (Portland ME)
        "Phone: 207-555-0198. Saturday: 09:00-14:00.",
        // Wrong phone (bakery)
        "Phone: 503-555-0177. Saturday: 09:00-14:00.",
        // Negated
        "The phone is not 503-555-0142.",
        "",
    ]);
}

#[test]
fn brave_caterer_hours_follow_authored_data() {
    let case = Case::load("search/brave/caterer-saturday-hours");
    // Change Saturday closes to 16:00
    assert_eq!(
        case.outcome_after(
            "/services/brave/pois/loc4CQWMJWLD4VBEBZ62XQLJTGK6YCJEEJDNA3BTZZQ=/opening_hours/days/5/0/closes",
            json!("16:00"),
            "503-555-0142, Saturday 09:00-14:00."
        ),
        Outcome::Fail,
    );
}
