//! Grade regressions for assertion coverage on business and cloud
//! record_facts contracts.
//!
//! Each scenario's assertions must:
//!   - pass the reference answer
//!   - reject an unsupported extra predicate appended to the reference
//!   - reject a role swap (where applicable)
//!   - track a source mutation

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

// ---------- business/hubspot/largest-open-deal ----------

#[test]
fn hubspot_largest_open_deal_assertions_pass_reference() {
    let case = Case::load("business/hubspot/largest-open-deal");
    case.reference_is_source_bound();
    case.passes(&["Northwind Logistics, $412,000."]);
}

#[test]
fn hubspot_largest_open_deal_unsupported_predicate_is_unresolved() {
    let case = Case::load("business/hubspot/largest-open-deal");
    case.unresolved(&["Northwind Logistics, $412,000. This deal is in the Contract Sent stage."]);
}

#[test]
fn hubspot_largest_open_deal_role_swap_fails() {
    let case = Case::load("business/hubspot/largest-open-deal");
    case.fails(&["Northwind Traders has the largest open deal, worth $412,000."]);
}

#[test]
fn hubspot_largest_open_deal_tracks_source_mutation() {
    let case = Case::load("business/hubspot/largest-open-deal");
    let path = "/services/hubspot/deals/9100107/properties/dealstage";
    assert_eq!(
        case.outcome_after(path, json!("contractsent"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            path,
            json!("contractsent"),
            "Northwind Traders has the largest open deal, worth $530,000."
        ),
        Outcome::Pass
    );
}

// ---------- business/multi/hubspot-largest-deal-salesforce-owner ----------

#[test]
fn hubspot_salesforce_owner_assertions_pass_reference() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    case.reference_is_source_bound();
}

#[test]
fn hubspot_salesforce_owner_unsupported_predicate_is_unresolved() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    case.unresolved(&[
        "Northwind Logistics; account owner Daniel Okafor; open cases 00002107 and 00002113. The deal is worth $412,000.",
    ]);
}

#[test]
fn hubspot_salesforce_owner_role_swap_fails() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    case.fails(&[
        "Northwind Traders; account owner Daniel Okafor; open cases 00002107 and 00002113.",
        "Northwind Logistics; account owner Elena Sokolova; open cases 00002107 and 00002113.",
    ]);
}

#[test]
fn hubspot_salesforce_owner_tracks_source_mutation() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    let owner = "/services/salesforce/Account/0015g00000NwLg1AAF/OwnerId";
    let reassigned = json!("0055g00000V8W5LAAV");
    assert_eq!(
        case.outcome_after(owner, reassigned, &case.reference),
        Outcome::Fail
    );
}

// ---------- business/multi/zendesk-order-status-shopify ----------

#[test]
fn zendesk_order_status_assertions_pass_reference() {
    let case = Case::load("business/multi/zendesk-order-status-shopify");
    case.reference_is_source_bound();
}

#[test]
fn zendesk_order_status_unsupported_predicate_is_unresolved() {
    let case = Case::load("business/multi/zendesk-order-status-shopify");
    case.unresolved(&[
        "Order #1063, not yet shipped, total $176.00. The order includes a sweater and a tote.",
    ]);
}

#[test]
fn zendesk_order_status_role_swap_fails() {
    let case = Case::load("business/multi/zendesk-order-status-shopify");
    case.fails(&["Order #1060, not yet shipped, total $176.00."]);
}

#[test]
fn zendesk_order_status_tracks_source_mutation() {
    let case = Case::load("business/multi/zendesk-order-status-shopify");
    let total = "/services/shopify/orders/5563/totalPrice";
    assert_eq!(
        case.outcome_after(total, json!("180.00"), &case.reference),
        Outcome::Fail
    );
}

// ---------- business/salesforce/open-pipeline-by-rep ----------

#[test]
fn salesforce_pipeline_assertions_pass_reference() {
    let case = Case::load("business/salesforce/open-pipeline-by-rep");
    case.reference_is_source_bound();
}

#[test]
fn salesforce_pipeline_unsupported_predicate_is_unresolved() {
    let case = Case::load("business/salesforce/open-pipeline-by-rep");
    case.unresolved(&[
        "Priya Raman has the largest open pipeline: $252,500. She has 3 open opportunities.",
    ]);
}

#[test]
fn salesforce_pipeline_role_swap_fails() {
    let case = Case::load("business/salesforce/open-pipeline-by-rep");
    case.fails(&["Marcus Bell has the largest open pipeline: $252,500."]);
}

#[test]
fn salesforce_pipeline_tracks_source_mutation() {
    let case = Case::load("business/salesforce/open-pipeline-by-rep");
    let amount = "/services/salesforce/Opportunity/0065g00000FMcaOAAT/Amount";
    assert_eq!(
        case.outcome_after(amount, json!(120500), &case.reference),
        Outcome::Fail
    );
}

// ---------- business/shopify/top-spender ----------

#[test]
fn shopify_top_spender_assertions_pass_reference() {
    let case = Case::load("business/shopify/top-spender");
    case.reference_is_source_bound();
}

#[test]
fn shopify_top_spender_unsupported_predicate_is_unresolved() {
    let case = Case::load("business/shopify/top-spender");
    case.unresolved(&[
        "Priya Raman (priya.raman@outlook.com) has spent the most: $1,130.00. She placed 3 orders.",
    ]);
}

#[test]
fn shopify_top_spender_role_swap_fails() {
    let case = Case::load("business/shopify/top-spender");
    case.fails(&["Tom Achebe (tom.achebe@yahoo.com) has spent the most: $1,130.00."]);
}

#[test]
fn shopify_top_spender_tracks_source_mutation() {
    let case = Case::load("business/shopify/top-spender");
    let runner_up = "/services/shopify/orders/5105/totalPrice";
    assert_eq!(
        case.outcome_after(runner_up, json!("1200.00"), &case.reference),
        Outcome::Fail
    );
}

// ---------- business/zendesk/find-invoice-attachment ----------

#[test]
fn zendesk_invoice_assertions_pass_reference() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    case.reference_is_source_bound();
}

#[test]
fn zendesk_invoice_unsupported_predicate_is_unresolved() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    case.unresolved(&[
        "Ticket 3108, file Lumen-Invoice-INV-20931.pdf. The ticket is about the October bill.",
    ]);
}

#[test]
fn zendesk_invoice_role_swap_fails() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    case.fails(&["Ticket 3105, file Lumen-Invoice-INV-20931.pdf."]);
}

// ---------- business/stripe/customer-mrr ----------

#[test]
fn stripe_mrr_assertions_pass_reference() {
    let case = Case::load("business/stripe/customer-mrr");
    case.reference_is_source_bound();
}

#[test]
fn stripe_mrr_unsupported_predicate_is_unresolved() {
    let case = Case::load("business/stripe/customer-mrr");
    case.unresolved(&["The MRR is $439.00. This includes the Team plan and Analytics add-on."]);
}

#[test]
fn stripe_mrr_wrong_value_fails() {
    let case = Case::load("business/stripe/customer-mrr");
    case.fails(&["The MRR is $252.50.", "The MRR is $339.00."]);
}

// ---------- cloud/aws/find-unencrypted-bucket ----------

#[test]
fn aws_unencrypted_bucket_assertions_pass_reference() {
    let case = Case::load("cloud/aws/find-unencrypted-bucket");
    case.reference_is_source_bound();
}

#[test]
fn aws_unencrypted_bucket_unsupported_predicate_is_unresolved() {
    let case = Case::load("cloud/aws/find-unencrypted-bucket");
    case.unresolved(&["The bucket is northwind-legacy-exports. It was created in 2019."]);
}

#[test]
fn aws_unencrypted_bucket_role_swap_fails() {
    let case = Case::load("cloud/aws/find-unencrypted-bucket");
    case.fails(&["northwind-legacy-exports-2019", "northwind-eu-backups"]);
}

#[test]
fn aws_unencrypted_bucket_tracks_source_mutation() {
    let case = Case::load("cloud/aws/find-unencrypted-bucket");
    let algorithm = "/services/aws/buckets/northwind-legacy-exports/Encryption/SSEAlgorithm";
    assert_eq!(
        case.outcome_after(algorithm, json!("aws:kms"), &case.reference),
        Outcome::EnvironmentError
    );
}

// ---------- cloud/cloudflare/find-stale-origin ----------

#[test]
fn cloudflare_stale_origin_assertions_pass_reference() {
    let case = Case::load("cloud/cloudflare/find-stale-origin");
    case.reference_is_source_bound();
}

#[test]
fn cloudflare_stale_origin_unsupported_predicate_is_unresolved() {
    let case = Case::load("cloud/cloudflare/find-stale-origin");
    case.unresolved(&["The hostname is checkout.octo-shop.com. It is an A record."]);
}

#[test]
fn cloudflare_stale_origin_role_swap_fails() {
    let case = Case::load("cloud/cloudflare/find-stale-origin");
    case.fails(&["legacy-reports.octo-corp.com", "cdn.octo-shop.com"]);
}

// ---------- cloud/mongodb/top-category-revenue ----------

#[test]
fn mongodb_top_category_assertions_pass_reference() {
    let case = Case::load("cloud/mongodb/top-category-revenue");
    case.reference_is_source_bound();
}

#[test]
fn mongodb_top_category_unsupported_predicate_is_unresolved() {
    let case = Case::load("cloud/mongodb/top-category-revenue");
    case.unresolved(&["outdoor: $1,457.50. Electronics was second at $1,309.00."]);
}

#[test]
fn mongodb_top_category_role_swap_fails() {
    let case = Case::load("cloud/mongodb/top-category-revenue");
    case.fails(&["electronics, with $1,457.50 in revenue."]);
}

#[test]
fn mongodb_top_category_tracks_source_mutation() {
    let case = Case::load("cloud/mongodb/top-category-revenue");
    let shipped = "/services/mongodb/db.shop.orders/65f1a000000000000000000b/status";
    assert_eq!(
        case.outcome_after(shipped, json!("delivered"), &case.reference),
        Outcome::Fail
    );
}

// ---------- cloud/multi/mongodb-zendesk-order-tracking ----------

#[test]
fn mongodb_order_tracking_assertions_pass_reference() {
    let case = Case::load("cloud/multi/mongodb-zendesk-order-tracking");
    case.reference_is_source_bound();
}

#[test]
fn mongodb_order_tracking_unsupported_predicate_is_unresolved() {
    let case = Case::load("cloud/multi/mongodb-zendesk-order-tracking");
    case.unresolved(&[
        "Order 10601: shipped, UPS, tracking 1Z4X97W80391827364. She ordered Studio X2 Headphones.",
    ]);
}

#[test]
fn mongodb_order_tracking_role_swap_fails() {
    let case = Case::load("cloud/multi/mongodb-zendesk-order-tracking");
    case.fails(&["Order 10603: shipped, FedEx, tracking 771234567890."]);
}

#[test]
fn mongodb_order_tracking_tracks_source_mutation() {
    let case = Case::load("cloud/multi/mongodb-zendesk-order-tracking");
    let status = "/services/mongodb/db.shop.orders/65f1a0000000000000000013/status";
    assert_eq!(
        case.outcome_after(status, json!("delivered"), &case.reference),
        Outcome::Fail
    );
}

// ---------- cloud/okta/failed-signins-report ----------

#[test]
fn okta_failed_signins_assertions_pass_reference() {
    let case = Case::load("cloud/okta/failed-signins-report");
    case.reference_is_source_bound();
}

#[test]
fn okta_failed_signins_unsupported_predicate_is_unresolved() {
    let case = Case::load("cloud/okta/failed-signins-report");
    case.unresolved(&[
        "jordyn.patel@octo-corp.example had 6 failures, every one from 203.0.113.77. All were password failures.",
    ]);
}

#[test]
fn okta_failed_signins_role_swap_fails() {
    let case = Case::load("cloud/okta/failed-signins-report");
    case.fails(&["jordan.patel@octo-corp.example had 6 failures, every one from 203.0.113.77."]);
}

#[test]
fn okta_failed_signins_tracks_source_mutation() {
    let case = Case::load("cloud/okta/failed-signins-report");
    let result = "/services/okta/logs/9b7c0019-1f2e-4c3d-8a5b-000000000019/outcome/result";
    assert_eq!(
        case.outcome_after(result, json!("SUCCESS"), &case.reference),
        Outcome::Fail
    );
}

// ---------- cloud/supabase/top-customer-march-revenue ----------

#[test]
fn supabase_march_revenue_assertions_pass_reference() {
    let case = Case::load("cloud/supabase/top-customer-march-revenue");
    case.reference_is_source_bound();
}

#[test]
fn supabase_march_revenue_unsupported_predicate_is_unresolved() {
    let case = Case::load("cloud/supabase/top-customer-march-revenue");
    case.unresolved(&[
        "Grace Kim (grace.kim@example.kr) brought in the most revenue: $1,234.50. She placed three orders.",
    ]);
}

#[test]
fn supabase_march_revenue_role_swap_fails() {
    let case = Case::load("cloud/supabase/top-customer-march-revenue");
    case.fails(&["Felix Wagner (felix.wagner@example.de) brought in the most revenue: $1,234.50."]);
}

#[test]
fn supabase_march_revenue_tracks_source_mutation() {
    let case = Case::load("cloud/supabase/top-customer-march-revenue");
    let refunded = "/services/supabase/db.qmvbxtzkplrnwdhyfaco.public.orders/5006/status";
    assert_eq!(
        case.outcome_after(refunded, json!("paid"), &case.reference),
        Outcome::Fail
    );
}

// ---------- cloud/multi/supabase-slack-referral-signups ----------

#[test]
fn supabase_referral_signups_assertions_pass_reference() {
    let case = Case::load("cloud/multi/supabase-slack-referral-signups");
    case.reference_is_source_bound();
}

#[test]
fn supabase_referral_signups_unsupported_predicate_is_unresolved() {
    let case = Case::load("cloud/multi/supabase-slack-referral-signups");
    case.unresolved(&[
        "4 customers have signed up, spending $410.50 in total. The FALL26 campaign is active.",
    ]);
}

#[test]
fn supabase_referral_signups_wrong_count_fails() {
    let case = Case::load("cloud/multi/supabase-slack-referral-signups");
    case.fails(&[
        "6 customers have signed up, spending $410.50 in total.",
        "4 customers have signed up, spending $860.50 in total.",
    ]);
}
