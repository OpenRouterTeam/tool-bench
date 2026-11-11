//! How the answer reader treats differently phrased replies to the business and cloud
//! lookup scenarios.
//!
//! A correct reply passes however it is phrased. A reply that names another record, swaps
//! two roles, states a wrong value or repeats a value the source no longer holds earns no
//! reward: it fails or is left unresolved.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::{Value, json};
use statement_support::Case;

fn unrewarded(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Fail | Outcome::Unresolved)
}

/// Every reply must earn no reward.
#[track_caller]
fn rejects(case: &Case, answers: &[&str]) {
    let rewarded: Vec<String> = answers
        .iter()
        .map(|answer| (answer, case.outcome(answer)))
        .filter(|(_, outcome)| !unrewarded(*outcome))
        .map(|(answer, outcome)| format!("{outcome:?}: {answer}"))
        .collect();
    assert!(
        rewarded.is_empty(),
        "replies that must earn no reward:\n{}",
        rewarded.join("\n")
    );
}

/// The reply must earn no reward once the authored source at `pointer` holds `value`.
#[track_caller]
fn rejects_after(case: &Case, pointer: &str, value: Value, answer: &str) {
    let outcome = case.outcome_after(pointer, value, answer);
    assert!(
        unrewarded(outcome),
        "a reply that must earn no reward after the source changed was graded {outcome:?}: {answer}"
    );
}

#[track_caller]
fn passes_after(case: &Case, pointer: &str, value: Value, answer: &str) {
    assert_eq!(
        case.outcome_after(pointer, value, answer),
        Outcome::Pass,
        "answer: {answer}"
    );
}

// ---------- business/hubspot/largest-open-deal ----------

#[test]
fn hubspot_largest_open_deal_terse_reply_passes() {
    let case = Case::load("business/hubspot/largest-open-deal");
    case.passes(&["Northwind Logistics, $412,000."]);
}

#[test]
fn hubspot_largest_open_deal_on_another_company_earns_no_reward() {
    let case = Case::load("business/hubspot/largest-open-deal");
    rejects(
        &case,
        &["Northwind Traders has the largest open deal, worth $412,000."],
    );
}

#[test]
fn hubspot_largest_open_deal_follows_the_source() {
    let case = Case::load("business/hubspot/largest-open-deal");
    let stage = "/services/hubspot/deals/9100107/properties/dealstage";
    rejects_after(&case, stage, json!("contractsent"), &case.reference);
    passes_after(
        &case,
        stage,
        json!("contractsent"),
        "Northwind Traders has the largest open deal, worth $530,000.",
    );
}

// ---------- business/multi/hubspot-largest-deal-salesforce-owner ----------

#[test]
fn hubspot_salesforce_owner_wrong_company_or_owner_earns_no_reward() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    rejects(
        &case,
        &[
            "Northwind Traders; account owner Daniel Okafor; open cases 00002107 and 00002113.",
            "Northwind Logistics; account owner Elena Sokolova; open cases 00002107 and 00002113.",
        ],
    );
}

#[test]
fn hubspot_salesforce_owner_missing_an_open_case_earns_no_reward() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    rejects(
        &case,
        &["Northwind Logistics; account owner Daniel Okafor; one open case, 00002107."],
    );
}

#[test]
fn hubspot_salesforce_owner_follows_a_reassigned_account() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    let owner = "/services/salesforce/Account/0015g00000NwLg1AAF/OwnerId";
    rejects_after(&case, owner, json!("0055g00000V8W5LAAV"), &case.reference);
}

#[test]
fn hubspot_salesforce_owner_follows_a_closed_case() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    let closed = "/services/salesforce/Case/5005g00000NwC13AAF/IsClosed";
    passes_after(
        &case,
        closed,
        json!(true),
        "Northwind Logistics; account owner Daniel Okafor; one open case, 00002107.",
    );
}

// Once one of the two cases is closed, a reply that still lists both as open has listed
// another record beside the answer.
#[test]
fn hubspot_salesforce_owner_listing_a_closed_case_as_open_earns_no_reward() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    let closed = "/services/salesforce/Case/5005g00000NwC13AAF/IsClosed";
    rejects_after(
        &case,
        closed,
        json!(true),
        "The largest open deal is with Northwind Logistics. Its Salesforce account is owned by Daniel Okafor and has two open cases: 00002107 and 00002113.",
    );
}

// ---------- business/multi/zendesk-order-status-shopify ----------

#[test]
fn zendesk_order_status_on_another_order_earns_no_reward() {
    let case = Case::load("business/multi/zendesk-order-status-shopify");
    rejects(&case, &["Order #1060, not yet shipped, total $176.00."]);
}

#[test]
fn zendesk_order_status_follows_the_order_total() {
    let case = Case::load("business/multi/zendesk-order-status-shopify");
    let total = "/services/shopify/orders/5563/totalPrice";
    rejects_after(&case, total, json!("180.00"), &case.reference);
}

// ---------- business/salesforce/open-pipeline-by-rep ----------

#[test]
fn salesforce_pipeline_credited_to_another_rep_earns_no_reward() {
    let case = Case::load("business/salesforce/open-pipeline-by-rep");
    rejects(
        &case,
        &["Marcus Bell has the largest open pipeline: $252,500."],
    );
}

#[test]
fn salesforce_pipeline_follows_an_opportunity_amount() {
    let case = Case::load("business/salesforce/open-pipeline-by-rep");
    let amount = "/services/salesforce/Opportunity/0065g00000FMcaOAAT/Amount";
    rejects_after(&case, amount, json!(120500), &case.reference);
}

// ---------- business/shopify/top-spender ----------

#[test]
fn shopify_top_spender_credited_to_another_customer_earns_no_reward() {
    let case = Case::load("business/shopify/top-spender");
    rejects(
        &case,
        &["Tom Achebe (tom.achebe@yahoo.com) has spent the most: $1,130.00."],
    );
}

#[test]
fn shopify_top_spender_follows_the_order_totals() {
    let case = Case::load("business/shopify/top-spender");
    let runner_up = "/services/shopify/orders/5105/totalPrice";
    rejects_after(&case, runner_up, json!("1200.00"), &case.reference);
}

// ---------- business/zendesk/find-invoice-attachment ----------

#[test]
fn zendesk_invoice_on_another_ticket_earns_no_reward() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    rejects(&case, &["Ticket 3105, file Lumen-Invoice-INV-20931.pdf."]);
}

#[test]
fn zendesk_invoice_follows_a_renamed_attachment() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    let file = "/services/zendesk/comments/3108~13/attachments/0/file_name";
    let renamed = json!("Lumen-Invoice-INV-30000.pdf");
    for stale in [
        "The invoice attachment is on ticket 3108: Lumen-Invoice-INV-20931.pdf.",
        "Ticket 3108, file Lumen-Invoice-INV-20931.pdf.",
        case.reference.as_str(),
    ] {
        rejects_after(&case, file, renamed.clone(), stale);
    }
    passes_after(
        &case,
        file,
        renamed,
        "The invoice attachment is on ticket 3108: Lumen-Invoice-INV-30000.pdf.",
    );
}

// ---------- business/stripe/customer-mrr ----------

#[test]
fn stripe_mrr_wrong_amount_earns_no_reward() {
    let case = Case::load("business/stripe/customer-mrr");
    rejects(&case, &["The MRR is $252.50.", "The MRR is $339.00."]);
}

// ---------- cloud/aws/find-unencrypted-bucket ----------

#[test]
fn aws_unencrypted_bucket_other_buckets_earn_no_reward() {
    let case = Case::load("cloud/aws/find-unencrypted-bucket");
    rejects(
        &case,
        &["northwind-legacy-exports-2019", "northwind-eu-backups"],
    );
}

#[test]
fn aws_unencrypted_bucket_needs_an_unencrypted_bucket_in_the_source() {
    let case = Case::load("cloud/aws/find-unencrypted-bucket");
    let algorithm = "/services/aws/buckets/northwind-legacy-exports/Encryption/SSEAlgorithm";
    assert_eq!(
        case.outcome_after(algorithm, json!("aws:kms"), &case.reference),
        Outcome::EnvironmentError
    );
}

// ---------- cloud/cloudflare/find-stale-origin ----------

#[test]
fn cloudflare_stale_origin_other_hostnames_earn_no_reward() {
    let case = Case::load("cloud/cloudflare/find-stale-origin");
    rejects(
        &case,
        &["legacy-reports.octo-corp.com", "cdn.octo-shop.com"],
    );
}

// ---------- cloud/mongodb/top-category-revenue ----------

#[test]
fn mongodb_top_category_revenue_on_another_category_earns_no_reward() {
    let case = Case::load("cloud/mongodb/top-category-revenue");
    rejects(&case, &["electronics, with $1,457.50 in revenue."]);
}

#[test]
fn mongodb_top_category_follows_the_order_status() {
    let case = Case::load("cloud/mongodb/top-category-revenue");
    let shipped = "/services/mongodb/db.shop.orders/65f1a000000000000000000b/status";
    rejects_after(&case, shipped, json!("delivered"), &case.reference);
}

// ---------- cloud/multi/mongodb-zendesk-order-tracking ----------

#[test]
fn mongodb_order_tracking_on_another_order_earns_no_reward() {
    let case = Case::load("cloud/multi/mongodb-zendesk-order-tracking");
    rejects(
        &case,
        &["Order 10603: shipped, FedEx, tracking 771234567890."],
    );
}

#[test]
fn mongodb_order_tracking_follows_the_order_status() {
    let case = Case::load("cloud/multi/mongodb-zendesk-order-tracking");
    let status = "/services/mongodb/db.shop.orders/65f1a0000000000000000013/status";
    rejects_after(&case, status, json!("delivered"), &case.reference);
}

// ---------- cloud/okta/failed-signins-report ----------

#[test]
fn okta_failed_signins_on_a_lookalike_user_earns_no_reward() {
    let case = Case::load("cloud/okta/failed-signins-report");
    rejects(
        &case,
        &["jordan.patel@octo-corp.example had 6 failures, every one from 203.0.113.77."],
    );
}

#[test]
fn okta_failed_signins_follows_the_log_outcomes() {
    let case = Case::load("cloud/okta/failed-signins-report");
    let result = "/services/okta/logs/9b7c0019-1f2e-4c3d-8a5b-000000000019/outcome/result";
    rejects_after(&case, result, json!("SUCCESS"), &case.reference);
}

// ---------- cloud/supabase/top-customer-march-revenue ----------

#[test]
fn supabase_march_revenue_credited_to_another_customer_earns_no_reward() {
    let case = Case::load("cloud/supabase/top-customer-march-revenue");
    rejects(
        &case,
        &["Felix Wagner (felix.wagner@example.de) brought in the most revenue: $1,234.50."],
    );
}

#[test]
fn supabase_march_revenue_follows_the_order_status() {
    let case = Case::load("cloud/supabase/top-customer-march-revenue");
    let refunded = "/services/supabase/db.qmvbxtzkplrnwdhyfaco.public.orders/5006/status";
    rejects_after(&case, refunded, json!("paid"), &case.reference);
}

// ---------- cloud/multi/supabase-slack-referral-signups ----------

#[test]
fn supabase_referral_signups_wrong_count_or_total_earns_no_reward() {
    let case = Case::load("cloud/multi/supabase-slack-referral-signups");
    rejects(
        &case,
        &[
            "6 customers have signed up, spending $410.50 in total.",
            "4 customers have signed up, spending $860.50 in total.",
        ],
    );
}

#[test]
fn supabase_referral_signups_follow_the_customer_list() {
    let case = Case::load("cloud/multi/supabase-slack-referral-signups");
    let email = "/services/supabase/db.qmvbxtzkplrnwdhyfaco.public.customers/1203/email";
    let counted = json!("real.user@example.com");
    rejects_after(
        &case,
        email,
        counted.clone(),
        "4 customers, $410.50 total spend.",
    );
    passes_after(&case, email, counted, "5 customers, $1,409.50 total spend.");
}
