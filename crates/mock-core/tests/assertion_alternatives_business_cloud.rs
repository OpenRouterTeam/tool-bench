//! Mutation regressions for assertion alternatives added to business and cloud
//! scenario contracts.
//!
//! Each test mutates an authored source field and verifies that stale literal
//! quantities or entity names in the old (pre-mutation) answer form never
//! produce Pass, while the truthful post-mutation wording does produce Pass.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

// ---------- business/multi/hubspot-largest-deal-salesforce-owner ----------
// The "two open cases" and "one open case" assertion variants carry literal
// source counts.  Closing one case must make "two" stale (never Pass) and
// "one" the truthful form.

#[test]
fn hubspot_salesforce_owner_two_open_cases_never_passes_after_closing_one() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    let close_case = "/services/salesforce/Case/5005g00000NwC13AAF/IsClosed";
    let stale = case.outcome_after(
        close_case,
        json!(true),
        "The largest open deal is with Northwind Logistics. Its Salesforce account is owned by Daniel Okafor and has two open cases: 00002107 and 00002113.",
    );
    assert_ne!(
        stale,
        Outcome::Pass,
        "stale 'two open cases' must not pass after closing one"
    );

    assert_eq!(
        case.outcome_after(
            close_case,
            json!(true),
            "Northwind Logistics; account owner Daniel Okafor; one open case, 00002107.",
        ),
        Outcome::Pass,
    );
}

#[test]
fn hubspot_salesforce_owner_one_open_case_is_stale_when_both_are_open() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    case.fails(&["Northwind Logistics; account owner Daniel Okafor; one open case, 00002107."]);
}

// ---------- business/zendesk/find-invoice-attachment ----------
// The enriched reference contains "invoice" as prose.  Mutating the file name
// to a new invoice number must never let the old file name pass.

#[test]
fn zendesk_invoice_enriched_reference_never_passes_with_stale_filename() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    let file = "/services/zendesk/comments/3108~13/attachments/0/file_name";

    let stale = case.outcome_after(
        file,
        json!("Lumen-Invoice-INV-30000.pdf"),
        "The invoice attachment is on ticket 3108: Lumen-Invoice-INV-20931.pdf.",
    );
    assert_ne!(
        stale,
        Outcome::Pass,
        "old filename must not pass after mutation"
    );

    assert_eq!(
        case.outcome_after(
            file,
            json!("Lumen-Invoice-INV-30000.pdf"),
            "The invoice attachment is on ticket 3108: Lumen-Invoice-INV-30000.pdf.",
        ),
        Outcome::Pass,
    );

    assert_eq!(
        case.outcome_after(
            file,
            json!("Lumen-Invoice-INV-30000.pdf"),
            "Ticket 3108, file Lumen-Invoice-INV-20931.pdf.",
        ),
        Outcome::Fail,
        "bare old filename without 'invoice' prose must fail outright",
    );
}

#[test]
fn zendesk_invoice_enriched_reference_is_unresolved_after_filename_mutation() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    let file = "/services/zendesk/comments/3108~13/attachments/0/file_name";
    assert_eq!(
        case.outcome_after(file, json!("Lumen-Invoice-INV-30000.pdf"), &case.reference),
        Outcome::Unresolved,
    );
}

// ---------- business/stripe/customer-mrr ----------
// The customer name alternative must never let a stale name pass.

#[test]
fn stripe_mrr_customer_name_stale_after_rename_never_passes() {
    let case = Case::load("business/stripe/customer-mrr");
    let rename = "/services/stripe/customers/cus_mxxxdqMeoA1GEc/name";

    let stale = case.outcome_after(
        rename,
        json!("Harbor & Pine Roasters"),
        "Harbor & Pine Coffee Co. has a monthly recurring revenue of $439.",
    );
    assert_ne!(
        stale,
        Outcome::Pass,
        "stale customer name must not pass after rename"
    );

    assert_eq!(
        case.outcome_after(
            rename,
            json!("Harbor & Pine Roasters"),
            "Harbor & Pine Roasters has a monthly recurring revenue of $439.",
        ),
        Outcome::Pass,
    );
}

// ---------- cloud/multi/supabase-slack-referral-signups ----------
// The concise "N customers, $X total spend" alternative must never let a
// stale count pass.

#[test]
fn supabase_referral_concise_alternative_stale_count_never_passes() {
    let case = Case::load("cloud/multi/supabase-slack-referral-signups");
    let qa_email = "/services/supabase/db.qmvbxtzkplrnwdhyfaco.public.customers/1203/email";

    let stale = case.outcome_after(
        qa_email,
        json!("real.user@example.com"),
        "4 customers, $410.50 total spend.",
    );
    assert_ne!(
        stale,
        Outcome::Pass,
        "stale count must not pass after un-QA-ing a customer"
    );

    assert_eq!(
        case.outcome_after(
            qa_email,
            json!("real.user@example.com"),
            "5 customers, $1,409.50 total spend.",
        ),
        Outcome::Pass,
    );
}
