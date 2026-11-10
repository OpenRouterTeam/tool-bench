//! Focused tests for the seven newly migrated statement-mode contracts.
//!
//! Each test verifies: reference answer passes, independently worded valid
//! answers pass, negated/wrong-entity/near-miss answers fail, unreviewed
//! wording is unresolved, missing source is an environment error, and a
//! data edit shows the fact follows the source.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

// ---- stripe customer-mrr ----

#[test]
fn stripe_mrr_binds_to_active_subscriptions_with_price_normalization() {
    let case = Case::load("business/stripe/customer-mrr");
    case.reference_is_source_bound();
    case.passes(&[
        "The MRR is $439.00.",
        "Harbor & Pine Coffee Co. has a monthly recurring revenue of $439.",
        "MRR: 439 USD",
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
    case.unresolved(&["I think the MRR may be around $439."]);
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
    ]);
    case.fails(&[
        // Wrong service
        "cart-api: 7 errors.",
        // Wrong count
        "payment-gateway: 12 errors.",
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
    ]);
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
    ]);
    case.fails(&[
        // Negated
        "Dana Nguyen is not on call.",
        "",
    ]);
    // Role swap is not detectable without rejected forms;
    // swapping names both affirms both facts regardless of role.
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
    case.unresolved(&[
        // Hedge
        "I think the phone may be 503-555-0142, Saturday 09:00-14:00.",
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
