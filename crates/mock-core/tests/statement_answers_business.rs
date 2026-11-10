//! Counterexamples for statement-mode answer contracts: business scenarios.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

#[test]
fn hubspot_largest_open_deal_binds_the_company_to_its_own_amount() {
    let case = Case::load("business/hubspot/largest-open-deal");
    case.reference_is_source_bound();
    case.passes(&[
        "Northwind Logistics, $412,000.",
        "Company: Northwind Logistics\nDeal amount: 412000 USD",
        "The biggest open deal belongs to **Northwind Logistics** and is worth $412k.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "It is not Northwind Logistics; their $412,000 deal is not the largest.",
        "The largest open deal is not Northwind Logistics ($412,000) but Umbrella Health.",
        "Rather than Northwind Logistics ($412,000), the largest open deal is Hooli's.",
        // Wrong entity carrying the right number, and the right entity with
        // another deal's amount.
        "Globex Corporation has the largest open deal, worth $412,000.",
        "Northwind Traders has the largest open deal, worth $412,000.",
        "Northwind Logistics has the largest open deal, worth $530,000.",
        // Near-miss numbers and a different currency.
        "Northwind Logistics has the largest open deal, worth $412,500.",
        "Northwind Logistics has the largest open deal, worth $41,200.",
        "Northwind Logistics has the largest open deal, worth €412,000.",
        "Northwind Logistics has the largest open deal.",
    ]);
    case.unresolved(&[
        "Northwind Logistics has the largest open deal, worth roughly four hundred thousand dollars.",
        "I think it may be Northwind Logistics at $412,000, but I could not confirm.",
        "Northwind has the largest open deal, worth $412,000.",
        "Northwind Logistics ($412,000) and Northwind Traders ($530,000) are the two largest.",
    ]);
}

#[test]
fn hubspot_facts_follow_the_authored_deals_not_a_retyped_answer() {
    let case = Case::load("business/hubspot/largest-open-deal");
    let reference = case.reference.clone();
    // Re-open the larger Northwind Traders deal: the authored data now names
    // a different company and amount, so the old reference no longer holds.
    let reopened = "/services/hubspot/deals/9100107/properties/dealstage";
    assert_eq!(
        case.outcome_after(reopened, json!("contractsent"), &reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            reopened,
            json!("contractsent"),
            "Northwind Traders has the largest open deal, worth $530,000."
        ),
        Outcome::Pass
    );
    // A tie has no single requested record.
    assert_eq!(
        case.outcome_after(
            "/services/hubspot/deals/9100108/properties/amount",
            json!("412000"),
            &reference
        ),
        Outcome::Unresolved
    );
}

#[test]
fn salesforce_pipeline_total_is_summed_per_owner_from_the_opportunities() {
    let case = Case::load("business/salesforce/open-pipeline-by-rep");
    case.reference_is_source_bound();
    case.passes(&[
        "Priya Raman - $252,500.00",
        "Rep: Priya Raman\nOpen Q2 pipeline: 252500 USD",
        "Priya Raman leads with $252.5k of open pipeline closing in Q2.",
    ]);
    case.fails(&[
        "Priya Raman does not have the largest pipeline; it is not $252,500.",
        "Not Priya Raman ($252,500): the largest open pipeline belongs to Marcus Bell.",
        // Another rep with the leader's total, and the leader with another
        // rep's total or with her closed-won deal included.
        "Marcus Bell has the largest open pipeline closing this quarter: $252,500.",
        "Priya Raman has the largest open pipeline closing this quarter: $240,000.",
        "Priya Raman has the largest open pipeline closing this quarter: $402,500.",
        "Priya Raman has the largest open pipeline closing this quarter: $252,000.",
        "Priya Raman has the largest open pipeline closing this quarter: 252,500 EUR.",
    ]);
    case.unresolved(&[
        "Priya has about a quarter million dollars of open pipeline.",
        "Priya Raman: $252,500. Marcus Bell: $240,000. Sofia Alvarez: $135,000.",
        "It is probably Priya Raman with $252,500.",
    ]);
    // The total follows the authored amounts.
    let amount = "/services/salesforce/Opportunity/0065g00000FMcaOAAT/Amount";
    assert_eq!(
        case.outcome_after(amount, json!(120500), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(amount, json!(120500), "Priya Raman: $253,000"),
        Outcome::Pass
    );
}

#[test]
fn hubspot_deal_company_is_followed_to_its_salesforce_owner_and_open_cases() {
    let case = Case::load("business/multi/hubspot-largest-deal-salesforce-owner");
    case.reference_is_source_bound();
    case.passes(&[
        "Company: Northwind Logistics\nAccount owner: Daniel Okafor\nOpen cases: 00002107, 00002113",
        "The largest open deal is with Northwind Logistics. Its Salesforce account is owned by Daniel Okafor and has two open cases: 00002113 (Escalated) and 00002107 (Working).",
    ]);
    case.fails(&[
        "The largest open deal is not with Northwind Logistics; the account owner is Daniel Okafor and the open cases are 00002107 and 00002113.",
        "It is not company Northwind Logistics. Owner Daniel Okafor; cases 00002107 and 00002113.",
        "Northwind Logistics is owned by Daniel Okafor and has no open cases (00002107 and 00002113 are closed).",
        "Rather than Northwind Logistics (owner Daniel Okafor, cases 00002107 and 00002113), the largest open deal is with Umbrella Health.",
        // The closed-won look-alike company, HubSpot's deal owner instead of
        // the Salesforce account owner, and a look-alike account's case.
        "Northwind Traders has the largest open deal; its Salesforce account is owned by Daniel Okafor, with open cases 00002107 and 00002113.",
        "Northwind Logistics has the largest open deal; its Salesforce account is owned by Elena Sokolova, with open cases 00002107 and 00002113.",
        "Northwind Logistics, owned by Daniel Okafor. Open cases: 00002107 and 00002109.",
        // One open case missing, and a near-miss case number.
        "Northwind Logistics, owned by Daniel Okafor. Open cases: 00002107.",
        "Northwind Logistics, owned by Daniel Okafor. Open cases: 00002107 and 00002131.",
    ]);
    case.unresolved(&[
        "I think the company is Northwind Logistics, owned by Daniel Okafor, with open cases 00002107 and 00002113.",
        "Northwind (owner: Daniel Okafor) has open cases 00002107 and 00002113.",
        "Northwind Logistics is owned by D. Okafor; open cases 00002107 and 00002113.",
        // The closed case listed as if it were open.
        "Northwind Logistics, owned by Daniel Okafor. Open cases: 00002101, 00002107, 00002113.",
    ]);
    // The owner follows the Salesforce account, and the case list follows
    // the cases' own open/closed state.
    let owner = "/services/salesforce/Account/0015g00000NwLg1AAF/OwnerId";
    let reassigned = json!("0055g00000V8W5LAAV");
    assert_eq!(
        case.outcome_after(owner, reassigned.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            owner,
            reassigned,
            "Northwind Logistics; account owner Tom Brandt; open cases 00002107 and 00002113."
        ),
        Outcome::Pass
    );
    let closed = "/services/salesforce/Case/5005g00000NwC13AAF/IsClosed";
    assert_eq!(
        case.outcome_after(closed, json!(true), &case.reference),
        Outcome::Unresolved
    );
    assert_eq!(
        case.outcome_after(
            closed,
            json!(true),
            "Northwind Logistics; account owner Daniel Okafor; one open case, 00002107."
        ),
        Outcome::Pass
    );
}

#[test]
fn zendesk_ticket_order_is_read_from_the_requesters_shopify_order() {
    let case = Case::load("business/multi/zendesk-order-status-shopify");
    case.reference_is_source_bound();
    case.passes(&[
        "Order #1063 — total $176.00 — UNFULFILLED (not shipped yet).",
        "- Order number: 1063\n- Status: has not shipped yet\n- Order total: 176 USD",
        "**Order number:** #1063\n**Shipped:** No — fulfillment status is UNFULFILLED\n**Order total:** $176.00 USD",
        "The order is #1063, awaiting fulfillment, total $176.00.",
    ]);
    case.fails(&[
        "Order #1063 ($176.00) is not unfulfilled.",
        "Order #1063 is no longer unfulfilled. Total $176.00.",
        "It is not #1063: that order is unfulfilled and totals $176.00.",
        "It is not order #1063. That order is unfulfilled and totals $176.00.",
        "Rather than #1063 (unfulfilled, $176.00), Sam is asking about #1051.",
        "Order #1063 ($176.00) has shipped; it was fulfilled.",
        // The other Sam Ortiz's order, and this order with another order's total.
        "Order #1060 has not shipped yet; the total is $176.00.",
        "Order #1063 has not shipped yet; the total is $505.00.",
        // Near misses.
        "Order #1036 has not shipped yet; the total is $176.00.",
        "Order #1063 has not shipped yet; the total is $167.00.",
        "Order #1063 has not shipped yet; the total is €176.00.",
    ]);
    case.unresolved(&[
        "I believe it is order #1063 (unfulfilled, $176.00).",
        "Order #1063 has not shipped; the total is one hundred seventy-six dollars.",
        "Order #1063 is unfulfilled ($176.00); order #1051 was fulfilled.",
    ]);
    // The total and the status are read from the order record.
    let total = "/services/shopify/orders/5563/totalPrice";
    assert_eq!(
        case.outcome_after(total, json!("180.00"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            total,
            json!("180.00"),
            "Order #1063 is unfulfilled; total $180.00."
        ),
        Outcome::Pass
    );
    let status = "/services/shopify/orders/5563/fulfillmentStatus";
    assert_ne!(
        case.outcome_after(status, json!("FULFILLED"), &case.reference),
        Outcome::Pass
    );
    assert_eq!(
        case.outcome_after(
            status,
            json!("FULFILLED"),
            "Order #1063 is unfulfilled; total $176.00."
        ),
        Outcome::Fail
    );
    // The reviewed wordings are bound to the coded value they describe.
    assert_eq!(
        case.outcome_after(
            status,
            json!("FULFILLED"),
            "Order #1063 has not shipped; total $176.00."
        ),
        Outcome::Fail
    );
    // A different requester on yesterday's ticket leaves no May 10 order.
    assert_eq!(
        case.outcome_after(
            "/services/zendesk/tickets/6621/requester_id",
            json!(9030103),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
}

#[test]
fn shopify_top_spender_total_excludes_refunded_and_cancelled_orders() {
    let case = Case::load("business/shopify/top-spender");
    case.reference_is_source_bound();
    case.passes(&[
        "Name: Priya Raman\nEmail: priya.raman@outlook.com\nTotal spent: 1130 USD",
        "**Priya Raman** <priya.raman@outlook.com> has spent $1.13k with us.",
    ]);
    case.fails(&[
        "The top spender is not Priya Raman (priya.raman@outlook.com, $1,130.00).",
        "It is not customer Priya Raman (priya.raman@outlook.com, $1,130.00).",
        "Rather than Priya Raman (priya.raman@outlook.com, $1,130.00), the top spender is Tom Achebe.",
        // The runner-up with the winner's total, the winner with the
        // runner-up's total or another customer's email, and the customer
        // who only leads when her refunded order is counted.
        "Tom Achebe (tom.achebe@yahoo.com) has spent the most: $1,130.00.",
        "Priya Raman (priya.raman@outlook.com) has spent the most: $1,050.00.",
        "Priya Raman (marcus.bell@gmail.com) has spent the most: $1,130.00.",
        "Elena Novak (elena.novak@protonmail.com) has spent the most: $1,822.00.",
        "Priya Raman (priya.raman@outlook.com) has spent the most: $1,103.00.",
        "Priya Raman (priya.raman@outlook.com) has spent the most: €1,130.00.",
    ]);
    case.unresolved(&[
        "Priya (priya.raman@outlook.com) has spent the most: $1,130.00.",
        // The right customer ranked anywhere but first.
        "Priya Raman (priya.raman@outlook.com) is second with $1,130.00.",
        "Priya Raman (priya.raman@outlook.com) has spent about eleven hundred dollars.",
        "It is probably Priya Raman (priya.raman@outlook.com) with $1,130.00.",
    ]);
    // A larger order for the runner-up, and an un-refunded order, change
    // who the top spender is.
    let runner_up = "/services/shopify/orders/5105/totalPrice";
    assert_eq!(
        case.outcome_after(runner_up, json!("1200.00"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            runner_up,
            json!("1200.00"),
            "Tom Achebe (tom.achebe@yahoo.com): $1,200.00"
        ),
        Outcome::Pass
    );
    assert_eq!(
        case.outcome_after(
            "/services/shopify/orders/5103/financialStatus",
            json!("PAID"),
            "Elena Novak (elena.novak@protonmail.com) has spent the most: $1,822.00."
        ),
        Outcome::Pass
    );
    // Cancelling one of the leader's orders takes it out of her total.
    let cancelled = "/services/shopify/orders/5108/cancelledAt";
    assert_eq!(
        case.outcome_after(cancelled, json!("2026-02-01T10:00:00Z"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            cancelled,
            json!("2026-02-01T10:00:00Z"),
            "Tom Achebe (tom.achebe@yahoo.com) has spent the most: $1,050.00."
        ),
        Outcome::Pass
    );
}

#[test]
fn zendesk_invoice_attachment_is_bound_to_its_author_and_ticket() {
    let case = Case::load("business/zendesk/find-invoice-attachment");
    case.reference_is_source_bound();
    case.passes(&[
        "Ticket: 3108\nFile: Lumen-Invoice-INV-20931.pdf",
        "It's on ticket 3108; the attachment is `Lumen-Invoice-INV-20931.pdf`.",
    ]);
    case.fails(&[
        "It is not #3108, and the file is not Lumen-Invoice-INV-20931.pdf.",
        "It is not ticket 3108. The file is Lumen-Invoice-INV-20931.pdf.",
        "Rather than #3108 (Lumen-Invoice-INV-20931.pdf), the invoice is on #3110.",
        // The agent-posted invoice on her colleague's ticket, the other
        // Priya's invoice, and crossed ticket/file pairs.
        "The invoice is on ticket 3110: Lumen-Invoice-INV-20877.pdf.",
        "Ticket 3105, file invoice_oct.pdf.",
        "Ticket 3108, file Lumen-Invoice-INV-20877.pdf.",
        "Ticket 3110, file Lumen-Invoice-INV-20931.pdf.",
        // Near misses.
        "Ticket 3108, file Lumen-Invoice-INV-20913.pdf.",
        "Ticket 3018, file Lumen-Invoice-INV-20931.pdf.",
    ]);
    case.unresolved(&[
        "Probably ticket 3108, file Lumen-Invoice-INV-20931.pdf.",
        "Ticket 3108 has the invoice PDF (INV-20931).",
        "Tickets 3108 (Lumen-Invoice-INV-20931.pdf) and 3110 (Lumen-Invoice-INV-20877.pdf) both hold invoices.",
    ]);
    // The file name is read from the comment's attachment.  The reference
    // prose also says "invoice", a word of the new name, so it is left to
    // review; the bare old name fails.
    let file = "/services/zendesk/comments/3108~13/attachments/0/file_name";
    assert_eq!(
        case.outcome_after(file, json!("Lumen-Invoice-INV-30000.pdf"), &case.reference),
        Outcome::Unresolved
    );
    assert_eq!(
        case.outcome_after(
            file,
            json!("Lumen-Invoice-INV-30000.pdf"),
            "Ticket 3108, file Lumen-Invoice-INV-20931.pdf."
        ),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            file,
            json!("Lumen-Invoice-INV-30000.pdf"),
            "Ticket 3108, file Lumen-Invoice-INV-30000.pdf."
        ),
        Outcome::Pass
    );
    // An invoice posted by someone else on the ticket is not hers.
    assert_eq!(
        case.outcome_after(
            "/services/zendesk/comments/3108~13/author_id",
            json!(9010002),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
}
