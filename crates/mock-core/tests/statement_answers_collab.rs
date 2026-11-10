//! Counterexamples for statement-mode answer contracts: collaboration scenarios.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

#[test]
fn notion_launch_reads_the_live_row_of_the_tracker_not_its_trashed_duplicate() {
    let case = Case::load("collab/notion/launch-date-owner");
    case.reference_is_source_bound();
    case.passes(&[
        "Launch date: November 12, 2026\nOwner: Lena Fischer",
        "Lena Fischer owns Payments v2 GA, which launches on Nov 12.",
        "| Launch | Date | Owner |\n| Payments v2 GA | 12 November 2026 | Lena Fischer |",
        // A polite hedge still states the answer.
        "Probably 2026-11-12, owner Lena Fischer.",
        // Other authored records named beside or after the answer are context.
        "The live row says 2026-11-12 (Lena Fischer); a trashed duplicate says 2026-11-05 (Priya Natarajan).",
    ]);
    case.fails(&[
        "It is not 2026-11-12; the owner is Lena Fischer.",
        "Payments v2 GA does not launch on 2026-11-12. Owner: Lena Fischer.",
        "Rather than 2026-11-12, the tracker says 2026-11-05; the owner is Lena Fischer.",
        "It launches on 2026-11-12. The owner isn't Lena Fischer.",
        // The trashed duplicate row, and each of its values on the live row.
        "Payments v2 GA launches on 2026-11-05 and is owned by Priya Natarajan.",
        "Payments v2 GA launches on 2026-11-12 and is owned by Priya Natarajan.",
        "Payments v2 GA launches on 2026-11-05 and is owned by Lena Fischer.",
        // The partner beta row and a near-miss date.
        "Payments v2 GA launches on 2026-10-20 and is owned by Lena Fischer.",
        "Payments v2 GA launches on 2026-11-13 and is owned by Lena Fischer.",
        "Payments v2 GA is owned by Lena Fischer.",
    ]);
    case.unresolved(&[
        "Payments v2 GA launches on 11/12/2026 and is owned by Lena Fischer.",
        "Payments v2 GA launches in mid-November and is owned by Lena Fischer.",
        "Payments v2 GA launches on 2026-11-12 and is owned by Lena.",
    ]);
    // The date follows the authored row.
    let start = "/services/notion/pages/71cdb3b7-f42a-472b-9b30-488942c27a9e/properties/Launch date/date/start";
    assert_eq!(
        case.outcome_after(start, json!("2026-11-19"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            start,
            json!("2026-11-19"),
            "Payments v2 GA launches on Nov 19, 2026 and is owned by Lena Fischer."
        ),
        Outcome::Pass
    );
    // Restoring the duplicate leaves no single live row.
    assert_eq!(
        case.outcome_after(
            "/services/notion/pages/ef5f79a6-f4be-4171-ba7f-7f448512f603/in_trash",
            json!(false),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
}

#[test]
fn slack_decision_reads_the_vendor_and_start_date_from_the_decision_message() {
    let case = Case::load("collab/slack/observability-decision");
    case.reference_is_source_bound();
    case.passes(&[
        "Vendor: Grafana Cloud\nContract start: November 15, 2026",
        "We picked Grafana Cloud; the new contract begins Nov 15, 2026 (3-year term).",
        "It is Grafana Cloud, not Datadog or Honeycomb, and the contract starts 15 November 2026.",
        // A polite hedge still states the answer.
        "I think we went with Grafana Cloud, starting 2026-11-15.",
    ]);
    case.fails(&[
        "It is not Grafana Cloud; the contract starts 2026-11-15.",
        "Rather than Grafana Cloud, we went with Datadog; the contract starts 2026-11-15.",
        "We did not pick Grafana Cloud; the contract starts 2026-11-15.",
        "We didn't choose Grafana Cloud; the contract starts 2026-11-15.",
        "Grafana Cloud was not chosen; the contract starts 2026-11-15.",
        "Grafana Cloud, but the contract does not start on 2026-11-15.",
        // The speculated vendors, with and without the right vendor nearby.
        "We chose Datadog; the contract starts 2026-11-15.",
        "We went with Honeycomb; the contract starts 2026-11-15.",
        "We picked Datadog. Grafana Cloud was the runner-up. The contract starts 2026-11-15.",
        // The other date of the decision message, alone and in swapped roles.
        "We chose Grafana Cloud; the contract starts 2027-01-31.",
        "Grafana Cloud. The contract starts on 2027-01-31 and the migration completes on 2026-11-15.",
        "We chose Grafana Cloud; the contract starts 2026-11-16.",
        "We chose Grafana Cloud.",
    ]);
    case.unresolved(&[
        // A negator across a verb that no reviewed incorrect form lists.
        "We did not end up with Grafana Cloud; the contract starts 2026-11-15.",
        "We chose Grafana, starting 2026-11-15.",
        "We chose Grafana Cloud, starting 11/15/2026.",
        "We chose Grafana Cloud; the contract starts in mid-November.",
    ]);
    // Both facts follow the authored decision message.
    let text = "/services/slack/messages/C03INFRA9QX~11793131800.099001/text";
    let edited = json!(
        "Decision: we're going with Lightstep. Contract signed today; it starts 2026-12-01 on a 2-year term."
    );
    assert_eq!(
        case.outcome_after(text, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            text,
            edited,
            "We signed with Lightstep; the contract starts 2026-12-01."
        ),
        Outcome::Pass
    );
}

#[test]
fn jira_blocked_item_binds_the_blocked_story_its_blocker_and_the_blockers_assignee() {
    let case = Case::load("collab/jira/blocked-sprint-item");
    case.reference_is_source_bound();
    case.passes(&[
        "- Blocked: PAY-119\n- Blocked by: PAY-117\n- Blocker assignee: Sam Ortiz",
        "PAY-117 (owned by sam.ortiz) blocks PAY-119.",
        "| Blocked story | Blocker | Blocker assignee |\n| PAY-119 | PAY-117 | Sam Ortiz |",
        // A polite hedge still states the answer.
        "PAY-119 seems to be blocked by PAY-117, assigned to Sam Ortiz.",
        // Other authored records named beside or after the answer are context.
        "PAY-119 (assigned to Alex Kim) is blocked by PAY-117 (assigned to Sam Ortiz).",
    ]);
    case.fails(&[
        // The two issues in swapped roles.
        "PAY-117 is blocked by PAY-119; Sam Ortiz is assigned.",
        "PAY-119 blocks PAY-117 (Sam Ortiz).",
        "Blocked issue: PAY-117. Blocker: PAY-119. Assignee: Sam Ortiz.",
        // Negated and rejected-alternative answers.
        "PAY-119 is not blocked by PAY-117; Sam Ortiz.",
        "PAY-117 does not block PAY-119; Sam Ortiz.",
        "PAY-119 is blocked by PAY-117, which is not assigned to Sam Ortiz.",
        "Rather than PAY-119, the blocked story is PAY-122, blocked by PAY-117 (Sam Ortiz).",
        // The blocked story of the future sprint, the merely related pair, and
        // the blocked story's own assignee in place of the blocker's.
        "PAY-122 is blocked by PAY-121, assigned to Alex Kim.",
        "PAY-120 is blocked by PAY-116, assigned to Sam Ortiz.",
        "PAY-119 is blocked by PAY-117, assigned to Alex Kim.",
        "PAY-119 is blocked by PAY-116, assigned to Sam Ortiz.",
        // Near-miss identifiers and name.
        "PAY-1190 is blocked by PAY-117, assigned to Sam Ortiz.",
        "PAY-119 is blocked by PAY-11, assigned to Sam Ortiz.",
        "PAY-119 is blocked by PAY-117, assigned to Sam Ortega.",
    ]);
    case.unresolved(&["PAY-119 is blocked by PAY-117, assigned to Ortiz."]);
    // The assignee follows the authored blocker.
    let assignee = "/services/jira/issues/PAY-117/fields/assignee";
    assert_eq!(
        case.outcome_after(assignee, json!("lena.fischer"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            assignee,
            json!("lena.fischer"),
            "PAY-119 is blocked by PAY-117, assigned to Lena Fischer."
        ),
        Outcome::Pass
    );
    // The blocker follows the authored link.
    let outward = "/services/jira/issuelinks/10500/outward";
    assert_eq!(
        case.outcome_after(outward, json!("PAY-116"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            outward,
            json!("PAY-116"),
            "PAY-119 is blocked by PAY-116, assigned to Sam Ortiz."
        ),
        Outcome::Pass
    );
    // The typed sprint is only accepted while it is the active sprint.
    assert_eq!(
        case.outcome_after(
            "/services/jira/sprints/7/state",
            json!("closed"),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
}

#[test]
fn linear_urgent_bug_selects_the_most_urgent_open_bug_of_the_project() {
    let case = Case::load("collab/linear/urgent-bug-owner");
    case.reference_is_source_bound();
    case.passes(&[
        "PAY-118 - owner: Priya - state: In Progress",
        "| Issue | Owner | State |\n| PAY-118 | Priya Natarajan | In Progress |",
        "PAY-118 is assigned to Priya Natarajan; state: in-progress.",
        // A polite hedge still states the answer.
        "It is likely PAY-118, owned by Priya Natarajan, In Progress.",
        // Other authored records named beside or after the answer are context.
        "PAY-118 (Priya Natarajan, In Progress); PAY-119 is the next most urgent.",
    ]);
    case.fails(&[
        "The most urgent open bug is not PAY-118; Priya Natarajan, In Progress.",
        "Rather than PAY-118, it is PAY-119 (Priya Natarajan, In Progress).",
        "PAY-118 is not assigned to Priya Natarajan; it is In Progress.",
        "PAY-118 (Priya Natarajan) is not In Progress.",
        "PAY-118 (Priya Natarajan) is no longer In Progress.",
        // The closed urgent bug, the urgent bug of another project, and the
        // open high-priority bug.
        "PAY-104 is owned by Priya Natarajan and is In Progress.",
        "PLAT-233, Sam Ortiz, In Progress.",
        "PAY-119, Lena Fischer, Todo.",
        // Another issue's owner or state on the right issue.
        "PAY-118 is owned by Lena Fischer and is In Progress.",
        "PAY-118 is owned by Priya Natarajan and is Done.",
        "PAY-118 is owned by Priya Natarajan and is In Review.",
        // Near-miss identifier.
        "PAY-1118 is owned by Priya Natarajan and is In Progress.",
        "PAY-118 is owned by Priya Natarajan.",
    ]);
    case.unresolved(&["PAY-118 is owned by Natarajan and is In Progress."]);
    // The selection follows the authored priorities and states.
    let priority = "/services/linear/issues/PAY-118/priority";
    assert_eq!(
        case.outcome_after(priority, json!(3), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(priority, json!(3), "PAY-119, Lena Fischer, Todo."),
        Outcome::Pass
    );
    // Two equally urgent open bugs: no single requested issue.
    assert_eq!(
        case.outcome_after(priority, json!(2), &case.reference),
        Outcome::Unresolved
    );
    // Closing the urgent bug makes the next open bug the answer.
    let state = "/services/linear/issues/PAY-118/stateId";
    let done = json!("464b7d47-51c4-50ce-aa96-03bed89ef6af");
    assert_eq!(
        case.outcome_after(state, done.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(state, done, "PAY-119, Lena Fischer, Todo."),
        Outcome::Pass
    );
}

#[test]
fn jira_sentry_impact_follows_the_bugs_link_and_binds_each_count_to_its_unit() {
    let case = Case::load("collab/multi/jira-sentry-crash-impact");
    case.reference_is_source_bound();
    case.passes(&[
        "- Sentry issue: STOREFRONT-WEB-8D\n- Events: 2,172\n- Affected users: 1,318\n- First release: storefront@5.3.0",
        "STOREFRONT-WEB-8D: 2172 events, 1318 users, first seen in 5.3.0.",
        "STOREFRONT-WEB-8D has 2,172 events affecting 1,318 users; first release storefront@5.3.0, last seen in storefront@5.3.1.",
        // The look-alike named only to set it apart.
        "STOREFRONT-WEB-8D (2,172 events, 1,318 users, first seen in 5.3.0), unlike STOREFRONT-WEB-7Q with 2,600 events.",
        // A polite hedge still states the answer.
        "It seems to be STOREFRONT-WEB-8D: 2,172 events and 1,318 users, first seen in 5.3.0.",
        // Other authored records named beside or after the answer are context.
        "STOREFRONT-WEB-8D: 2,172 events, 1,318 users, first seen in 5.3.0. STOREFRONT-WEB-7Q has 2,600 events.",
    ]);
    case.fails(&[
        "It is not STOREFRONT-WEB-8D: 2,172 events and 1,318 users, first seen in 5.3.0.",
        "Rather than STOREFRONT-WEB-8D, the linked issue is STOREFRONT-WEB-7Q: 2,172 events and 1,318 users, first seen in 5.3.0.",
        // The two counts under each other's unit.
        "STOREFRONT-WEB-8D: 1,318 events and 2,172 users, first seen in 5.3.0.",
        // The look-alike linked from the Done bug, and each of its values on
        // the right issue.
        "STOREFRONT-WEB-7Q: 2,600 events and 1,904 users, first seen in 5.2.0.",
        "STOREFRONT-WEB-7Q: 2,172 events and 1,318 users, first seen in 5.3.0.",
        "STOREFRONT-WEB-8D: 2,600 events and 1,904 users, first seen in 5.3.0.",
        "STOREFRONT-WEB-8D: 2,172 events and 1,318 users, first seen in 5.2.0.",
        "ADMIN-DASHBOARD-2M: 96 events and 14 users, first seen in 2.8.0.",
        // The last release offered as the first, and near-miss numbers.
        "STOREFRONT-WEB-8D: 2,172 events and 1,318 users, first seen in 5.3.1.",
        "STOREFRONT-WEB-8D: 2,173 events and 1,318 users, first seen in 5.3.0.",
        "STOREFRONT-WEB-8D: 2,172 events and 1,381 users, first seen in 5.3.0.",
        "STOREFRONT-WEB-8: 2,172 events and 1,318 users, first seen in 5.3.0.",
        "STOREFRONT-WEB-8D: 2,172 events, first seen in 5.3.0.",
    ]);
    case.unresolved(&[
        // Bare numbers, a table whose units sit in the header row, and
        // number words carry no reviewed unit.
        "STOREFRONT-WEB-8D: 2,172 and 1,318, first seen in 5.3.0.",
        "| Short ID | Events | Users affected | First release |\n| STOREFRONT-WEB-8D | 2,172 | 1,318 | storefront@5.3.0 |",
        "STOREFRONT-WEB-8D: about two thousand events and 1,318 users, first seen in 5.3.0.",
    ]);
    // The issue is the one the bug's description links.
    let description = "/services/jira/issues/WEB-56/fields/description";
    let relinked = json!(
        "Category pages render a blank grid.\n\nSentry: https://lumen-labs.sentry.io/issues/6184420122/"
    );
    assert_eq!(
        case.outcome_after(description, relinked.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            description,
            relinked,
            "STOREFRONT-WEB-7Q: 2,600 events and 1,904 users, first seen in 5.2.0."
        ),
        Outcome::Pass
    );
    // The counts follow the authored Sentry issue.
    let count = "/services/sentry/issues/STOREFRONT-WEB-8D/count";
    assert_eq!(
        case.outcome_after(count, json!("2180"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            count,
            json!("2180"),
            "STOREFRONT-WEB-8D: 2,180 events and 1,318 users, first seen in 5.3.0."
        ),
        Outcome::Pass
    );
}

#[test]
fn google_invoice_reads_po_amount_and_due_date_from_the_october_invoice_email() {
    let case = Case::load("collab/google/northwind-invoice-lookup");
    case.reference_is_source_bound();
    case.passes(&[
        "- PO: PO-48213\n- Amount due: $12,480.00\n- Due: 2026-11-20",
        "PO-48213, USD 12,480, due Nov 20, 2026.",
        "The invoice references PO-48213 for $12,480.00 (this replaces the earlier draft figure of $11,900.00), due 20 November 2026.",
        // A polite hedge still states the answer.
        "I think it is PO-48213, $12,480.00, due November 20, 2026.",
        // Other authored records named beside or after the answer are context.
        "PO-48213, $12,480.00, due November 20, 2026; the September invoice was PO-47790.",
    ]);
    case.fails(&[
        "The PO is not PO-48213; $12,480.00, due November 20, 2026.",
        "Rather than PO-48213, the invoice references PO-47790: $12,480.00, due November 20, 2026.",
        "PO-48213, $12,480.00, but it is not due on November 20, 2026.",
        // The superseded draft figure mentioned on the call.
        "PO-48213, $11,900.00, due November 20, 2026.",
        // Northwood Logistics' October invoice and the paid September
        // invoice, whole and one value at a time.
        "PO-48899, $3,200.00, due November 25, 2026.",
        "PO-47790, $9,150.00, due October 20, 2026.",
        "PO-48899, $12,480.00, due November 20, 2026.",
        "PO-48213, $3,200.00, due November 20, 2026.",
        "PO-48213, $12,480.00, due November 25, 2026.",
        // Near-miss values and another currency.
        "PO-48231, $12,480.00, due November 20, 2026.",
        "PO-48213, $12,840.00, due November 20, 2026.",
        "PO-48213, $12,480.00, due November 21, 2026.",
        "PO-48213, €12,480.00, due November 20, 2026.",
        "PO-48213, $12,480.00.",
    ]);
    case.unresolved(&[
        "PO-48213, $12,480.00, due 11/20/2026.",
        "PO-48213, about twelve and a half thousand dollars, due November 20, 2026.",
    ]);
    // All three facts follow the authored body of the invoice email.
    let body = "/services/google/gmail_messages/b26a427d1034a022/body_text";
    let edited = json!(
        case.initial
            .pointer(body)
            .and_then(|text| text.as_str())
            .expect("the invoice body")
            .replace("November 20, 2026", "December 4, 2026")
            .replace("$12,480.00", "$12,610.50")
    );
    assert_eq!(
        case.outcome_after(body, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(body, edited, "PO-48213, $12,610.50, due Dec 4, 2026."),
        Outcome::Pass
    );
}
