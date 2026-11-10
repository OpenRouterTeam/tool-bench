//! Counterexamples for statement-mode answer contracts: cloud, example and
//! search scenarios.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::{Value, json};
use statement_support::Case;

/// Grades `answer` after several edits to the authored source.
fn outcome_after_all(case: &Case, edits: &[(&str, Value)], answer: &str) -> Outcome {
    let mut edited = case.initial.clone();
    for (pointer, value) in edits {
        *edited
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("no authored value at {pointer}")) = value.clone();
    }
    case.grade_with(&edited, answer).outcome
}

#[test]
fn cloudflare_stale_origin_names_only_the_proxied_record() {
    let case = Case::load("cloud/cloudflare/find-stale-origin");
    case.reference_is_source_bound();
    case.passes(&[
        "`checkout.octo-shop.com`",
        "The hostname is checkout.octo-shop.com.",
        "Hostname: **checkout.octo-shop.com** (A record, proxied, content 203.0.113.40)",
    ]);
    case.fails(&[
        "It is not checkout.octo-shop.com.",
        "It is not hostname checkout.octo-shop.com.",
        "Rather than checkout.octo-shop.com, the stale record is cdn.octo-shop.com.",
        // Same address but DNS only, a neighbouring address, and a prefix of
        // the address.
        "legacy-reports.octo-corp.com",
        "cdn.octo-shop.com",
        "preview.octo-corp.dev",
        "_acme-challenge.octo-corp.dev",
    ]);
    case.unresolved(&[
        "It might be checkout.octo-shop.com.",
        // The right record cleared instead of named as the stale one.
        "checkout.octo-shop.com is fine; the stale one is elsewhere.",
        // The legacy predicate failed an answer that also lists look-alikes;
        // here it is never a pass.
        "checkout.octo-shop.com and legacy-reports.octo-corp.com",
        "checkout.octo-shop.com (zone octo-shop.com)",
        // A truncated hostname is an unreviewed partial name.
        "checkout.octo-shop.co",
    ]);
    // Renaming the record renames the answer; moving the proxy flag to the
    // DNS-only look-alike moves the answer there.
    let record = "/services/cloudflare/dns_records/9f6b1e5d4c7a2f0b8d3e6c5a1f4b7d9c";
    let name = format!("{record}/name");
    assert_eq!(
        case.outcome_after(&name, json!("pay.octo-shop.com"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&name, json!("pay.octo-shop.com"), "pay.octo-shop.com"),
        Outcome::Pass
    );
    let moved = [
        (&*format!("{record}/proxied"), json!(false)),
        (
            "/services/cloudflare/dns_records/6c3e8b2a1f4d9c7e5a0b3f2d8c1e4a69/proxied",
            json!(true),
        ),
    ];
    assert_eq!(
        outcome_after_all(&case, &moved, &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        outcome_after_all(&case, &moved, "legacy-reports.octo-corp.com"),
        Outcome::Pass
    );
    // Two proxied records on the address leave no single answer.
    assert_eq!(
        outcome_after_all(&case, &moved[1..], &case.reference),
        Outcome::EnvironmentError
    );
}

#[test]
fn aws_unencrypted_bucket_is_the_one_with_sse_s3_default_encryption() {
    let case = Case::load("cloud/aws/find-unencrypted-bucket");
    case.reference_is_source_bound();
    case.passes(&[
        "`northwind-legacy-exports`",
        "The bucket is northwind-legacy-exports (default encryption SSE-S3, AES256).",
        "northwind-legacy-exports defaults to SSE-S3 (AES256) instead of the required SSE-KMS.",
    ]);
    case.fails(&[
        "It is not northwind-legacy-exports.",
        "It is not bucket northwind-legacy-exports.",
        "Rather than northwind-legacy-exports, it is northwind-data-lake-raw.",
        // The KMS-encrypted look-alike whose name extends the answer.
        "northwind-legacy-exports-2019",
        "northwind-eu-backups",
        "northwind-legacy-export",
    ]);
    case.unresolved(&[
        "Probably northwind-legacy-exports.",
        // The right bucket described as already meeting the policy.
        "northwind-legacy-exports uses KMS; no bucket is on SSE-S3.",
        "northwind-legacy-exports and northwind-legacy-exports-2019",
        "The legacy exports bucket.",
    ]);
    let algorithm =
        |bucket: &str| format!("/services/aws/buckets/{bucket}/Encryption/SSEAlgorithm");
    // Once the bucket uses KMS there is no SSE-S3 bucket left to name.
    assert_eq!(
        case.outcome_after(
            &algorithm("northwind-legacy-exports"),
            json!("aws:kms"),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
    let moved = [
        (&*algorithm("northwind-legacy-exports"), json!("aws:kms")),
        (&*algorithm("northwind-data-lake-raw"), json!("AES256")),
    ];
    assert_eq!(
        outcome_after_all(&case, &moved, &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        outcome_after_all(&case, &moved, "northwind-data-lake-raw"),
        Outcome::Pass
    );
}

#[test]
fn mongodb_category_revenue_is_summed_over_delivered_february_lines() {
    let case = Case::load("cloud/mongodb/top-category-revenue");
    case.reference_is_source_bound();
    case.passes(&[
        "Category: outdoor\nFebruary revenue: 1457.5 USD",
        "The winner is **outdoor** at $1,457.50.",
    ]);
    case.fails(&[
        "The top category is not outdoor ($1,457.50).",
        "It is not category outdoor ($1,457.50).",
        "Rather than outdoor ($1,457.50), electronics brought in the most.",
        // Another category with the winner's revenue, the winner with
        // another category's revenue, the answer when undelivered orders
        // count, and the winner with its refunded order included.
        "electronics, with $1,457.50 in revenue.",
        "outdoor, with $1,309.00 in revenue.",
        "electronics, with $2,208.00 in revenue.",
        "outdoor, with $1,576.50 in revenue.",
        "outdoor, with $1,457.05 in revenue.",
        "outdoor, with €1,457.50 in revenue.",
        "outdoor brought in the most revenue.",
    ]);
    case.unresolved(&[
        "outdoor, with roughly fifteen hundred dollars in revenue.",
        "Probably outdoor with $1,457.50.",
        // The right category and value with the opposite claim.
        "The outdoor category had the least revenue, $1,457.50.",
        "outdoor $1,457.50, electronics $1,309.00, kitchen $798.00, apparel $436.00.",
    ]);
    // Delivering the shipped tent order raises outdoor's revenue; delivering
    // the cancelled TV order makes electronics the winner.
    let shipped = "/services/mongodb/db.shop.orders/65f1a000000000000000000b/status";
    assert_eq!(
        case.outcome_after(shipped, json!("delivered"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(shipped, json!("delivered"), "outdoor: $1,886.50"),
        Outcome::Pass
    );
    let cancelled = "/services/mongodb/db.shop.orders/65f1a0000000000000000005/status";
    assert_eq!(
        case.outcome_after(cancelled, json!("delivered"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(cancelled, json!("delivered"), "electronics: $2,208.00"),
        Outcome::Pass
    );
}

#[test]
fn mongodb_order_tracking_is_read_from_the_requesters_production_order() {
    let case = Case::load("cloud/multi/mongodb-zendesk-order-tracking");
    case.reference_is_source_bound();
    case.passes(&[
        "Order number: 10601\nStatus: shipped\nCarrier: UPS\nTracking number: 1Z4X97W80391827364",
        "Order 10601 has shipped with UPS (tracking 1Z4X97W80391827364) but has not yet been delivered.",
        "Order 10601 was shipped on Oct 23 via UPS (1Z4X97W80391827364), ETA Oct 31; not delivered yet.",
    ]);
    case.fails(&[
        "Order 10601 has not shipped; carrier UPS, tracking 1Z4X97W80391827364.",
        "Order 10601 hasn't shipped yet; carrier UPS, tracking 1Z4X97W80391827364.",
        "Order 10601 has not yet shipped; carrier UPS, tracking 1Z4X97W80391827364.",
        "It is not order 10601 (shipped, UPS, 1Z4X97W80391827364).",
        "Order 10601 won't be shipped by UPS; tracking 1Z4X97W80391827364.",
        "Order 10601 shipped, not via UPS; tracking 1Z4X97W80391827364.",
        "Rather than 10601 (shipped via UPS, 1Z4X97W80391827364), she means 10605.",
        // Another customer's headphones order, crossed carrier and tracking
        // values, the staging copy, and her earlier cable order.
        "Order 10603: shipped, FedEx, tracking 771234567890.",
        "Order 10601: shipped, FedEx, tracking 1Z4X97W80391827364.",
        "Order 10601: shipped, UPS, tracking 771234567890.",
        "Order 10601: shipped. Carrier: UPS. No tracking number 1Z4X97W80391827364 exists.",
        "Order 10601: delivered, UPS, tracking 1Z999AA10123456784.",
        "Order 10598: delivered, USPS, tracking 9400111899223344556677.",
        // Near misses.
        "Order 10601: shipped, UPS, tracking 1Z4X97W80391827346.",
        "Order 10610: shipped, UPS, tracking 1Z4X97W80391827364.",
    ]);
    case.unresolved(&[
        "I think it's order 10601: shipped, UPS, tracking 1Z4X97W80391827364.",
        "Order 10601: shipped, UPS, tracking 1Z4X97W80391827364. Her other order 10605 is still processing.",
    ]);
    // The status and tracking number are read from the order document.
    let order = "/services/mongodb/db.shop.orders/65f1a0000000000000000013";
    let status = format!("{order}/status");
    assert_eq!(
        case.outcome_after(&status, json!("delivered"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            &status,
            json!("delivered"),
            "Order 10601: delivered, UPS, tracking 1Z4X97W80391827364."
        ),
        Outcome::Pass
    );
    let tracking = format!("{order}/shipping/trackingNumber");
    assert_eq!(
        case.outcome_after(&tracking, json!("1Z4X97W80391800000"), &case.reference),
        Outcome::Fail
    );
}

#[test]
fn okta_failed_signins_are_counted_per_account_for_the_day() {
    let case = Case::load("cloud/okta/failed-signins-report");
    case.reference_is_source_bound();
    case.passes(&[
        "Username: jordyn.patel@octo-corp.example\nFailed attempts: 6\nIP address: 203.0.113.77",
        "jordyn.patel@octo-corp.example had 6 failures, every one from 203.0.113.77.",
    ]);
    case.fails(&[
        "It was not jordyn.patel@octo-corp.example (6 failed attempts from 203.0.113.77).",
        "It is not account jordyn.patel@octo-corp.example; 6 failed attempts from 203.0.113.77.",
        "Rather than jordyn.patel@octo-corp.example (6 failed attempts, 203.0.113.77), it was lee.wong@octo-corp.example.",
        // The look-alike login, another account's count or address, the
        // account that failed more on another day, and the one that failed
        // more just after midnight.
        "jordan.patel@octo-corp.example had 6 failed attempts from 203.0.113.77.",
        "jordyn.patel@octo-corp.example had 3 failed attempts from 203.0.113.77.",
        "jordyn.patel@octo-corp.example had 6 failed attempts from 203.0.113.42.",
        "jordyn.patel@octo-corp.example had 6 failed attempts, none from 203.0.113.77.",
        "lee.wong@octo-corp.example had 8 failed attempts from 198.51.100.23.",
        "alex.kim@octo-corp.example had 7 failed attempts from 198.51.100.90.",
        // Near misses.
        "jordyn.patel@octo-corp.example had 7 failed attempts from 203.0.113.77.",
        "jordyn.patel@octo-corp.example had 6 failed attempts from 203.0.113.7.",
    ]);
    case.unresolved(&[
        "jordyn.patel@octo-corp.example had six failed attempts from 203.0.113.77.",
        // A bare number is not tied to the attempts.
        "jordyn.patel@octo-corp.example: 6, from 203.0.113.77.",
        "Jordyn Patel had 6 failed attempts from 203.0.113.77.",
        "Apparently jordyn.patel@octo-corp.example had 6 failed attempts from 203.0.113.77.",
    ]);
    // One fewer failure for the account lowers the stated count.
    let result = "/services/okta/logs/9b7c0019-1f2e-4c3d-8a5b-000000000019/outcome/result";
    assert_eq!(
        case.outcome_after(result, json!("SUCCESS"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            result,
            json!("SUCCESS"),
            "jordyn.patel@octo-corp.example had 5 failed attempts from 203.0.113.77."
        ),
        Outcome::Pass
    );
}

#[test]
fn supabase_march_revenue_is_summed_per_customer_over_paid_and_shipped_orders() {
    let case = Case::load("cloud/supabase/top-customer-march-revenue");
    case.reference_is_source_bound();
    case.passes(&[
        "Customer: Grace Kim\nEmail: grace.kim@example.kr\nMarch total: 1234.5 USD",
        "Grace Kim <grace.kim@example.kr> leads March with $1,234.50.",
    ]);
    case.fails(&[
        "It is not Grace Kim (grace.kim@example.kr, $1,234.50).",
        "It is not customer Grace Kim (grace.kim@example.kr, $1,234.50).",
        "Rather than Grace Kim (grace.kim@example.kr, $1,234.50), the top customer is Felix Wagner.",
        // The runner-up with the winner's total, the winner with the
        // runner-up's total or email, and the runner-up's total when his
        // refunded order is counted.
        "Felix Wagner (felix.wagner@example.de) brought in the most: $1,234.50.",
        "Grace Kim (grace.kim@example.kr) brought in the most: $1,100.00.",
        "Grace Kim (felix.wagner@example.de) brought in the most: $1,234.50.",
        "Felix Wagner (felix.wagner@example.de) brought in the most: $1,700.00.",
        "Grace Kim (grace.kim@example.kr) brought in the most: $1,243.50.",
        "Grace Kim (grace.kim@example.kr) brought in the most: €1,234.50.",
    ]);
    case.unresolved(&[
        "Grace (grace.kim@example.kr) brought in the most: $1,234.50.",
        // The right customer ranked anywhere but first.
        "Grace Kim (grace.kim@example.kr) was second with $1,234.50.",
        "I believe it is Grace Kim (grace.kim@example.kr) with $1,234.50.",
        "Grace Kim (grace.kim@example.kr) brought in about twelve hundred dollars.",
    ]);
    // Un-refunding the runner-up's order makes him the top customer.
    let refunded = "/services/supabase/db.qmvbxtzkplrnwdhyfaco.public.orders/5006/status";
    assert_eq!(
        case.outcome_after(refunded, json!("paid"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            refunded,
            json!("paid"),
            "Felix Wagner (felix.wagner@example.de): $1,700.00"
        ),
        Outcome::Pass
    );
}

#[test]
fn elasticsearch_key_deletion_names_the_actor_and_time_of_the_successful_event() {
    let case = Case::load("search/elasticsearch/deleted-api-key-audit");
    case.reference_is_source_bound();
    case.passes(&[
        "- User: m.okafor\n- Deleted at: 2026-10-29T16:42:57Z",
        "The key was deleted by m.okafor at 16:42 UTC on October 29.",
    ]);
    case.fails(&[
        "It was not m.okafor at 16:42 UTC.",
        "It was not user m.okafor; the deletion was at 16:42 UTC.",
        "Rather than m.okafor (16:42 UTC), the key was deleted by r.chen.",
        // The denied attempt's actor or time, the staging key's deletion,
        // the legacy cluster's deletion, and the replacement key's creation.
        "j.doe deleted the key at 16:42 UTC.",
        "m.okafor deleted the key at 14:05 UTC.",
        "r.chen deleted the key at 17:30 UTC.",
        "a.santos deleted the key at 11:00 UTC.",
        "m.okafor deleted the key at 16:44 UTC.",
        "m.okafor deleted the key at 16:43 UTC.",
    ]);
    case.unresolved(&[
        "I think m.okafor deleted it at 16:42 UTC.",
        "Okafor deleted it at 16:42 UTC.",
        // A 12-hour time is not a reviewed notation.
        "m.okafor deleted the key at 4:42 PM UTC.",
        // A negated verb after the right actor is never an affirmation.
        "m.okafor did not delete the key at 16:42 UTC.",
        "m.okafor deleted it at 16:42 UTC; r.chen deleted the staging key at 17:30 UTC.",
    ]);
    // The actor and the time follow the audit event.
    let event = "/services/elasticsearch/docs/audit-security-2026.10~1a4";
    let actor = format!("{event}/actor");
    assert_eq!(
        case.outcome_after(&actor, json!("r.chen"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&actor, json!("r.chen"), "r.chen deleted it at 16:42 UTC."),
        Outcome::Pass
    );
    let timestamp = format!("{event}/@timestamp");
    assert_eq!(
        case.outcome_after(
            &timestamp,
            json!("2026-10-29T18:07:31Z"),
            "m.okafor deleted it at 18:07 UTC."
        ),
        Outcome::Pass
    );
}

#[test]
fn elasticsearch_duplicate_charge_lists_both_orders_of_the_repeated_total() {
    let case = Case::load("search/elasticsearch/duplicate-charge-lookup");
    case.reference_is_source_bound();
    case.passes(&[
        "Duplicate orders: ORD-109002, ORD-109003\nAmount charged twice: $86.40",
        "ORD-109003 duplicates ORD-109002; each was charged 86.4 USD.",
    ]);
    case.fails(&[
        "The duplicate is not ORD-109002 and ORD-109003 ($86.40).",
        "It is not orders ORD-109002 and ORD-109003 ($86.40).",
        "Rather than ORD-109002 and ORD-109003 ($86.40), the double charge is ORD-108815 and ORD-108816.",
        // Another customer's same-amount order, another customer's own
        // duplicate pair, another order's amount, and another of his orders.
        "ORD-109002 and ORD-109010, both $86.40.",
        "ORD-108815 and ORD-108816, both $129.99.",
        "ORD-109002 and ORD-109003, both $64.00.",
        "ORD-109002 and ORD-109311, both $86.40.",
        // Near misses and half an answer.
        "ORD-109002 and ORD-109004, both $86.40.",
        "ORD-109002 and ORD-109003, both $86.04.",
        "ORD-109002 was charged twice for $86.40.",
    ]);
    case.unresolved(&[
        "I suspect ORD-109002 and ORD-109003, $86.40 each.",
        "ORD-109002 and ORD-109003 are not duplicates; $86.40 was charged once.",
        "ORD-109002 and ORD-109003, about eighty-six dollars each.",
        "ORD-109002 and ORD-109003 ($86.40 each), plus ORD-109777 ($64.00).",
    ]);
    // Changing one total moves the duplicate to another pair of his orders;
    // a unique total leaves no duplicate to report.
    let total = "/services/elasticsearch/docs/orders-2026~1ORD-109003/total";
    assert_eq!(
        case.outcome_after(total, json!(64.0), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            total,
            json!(64.0),
            "ORD-109003 and ORD-109777, $64.00 each."
        ),
        Outcome::Pass
    );
    assert_eq!(
        case.outcome_after(total, json!(86.5), &case.reference),
        Outcome::Unresolved
    );
}

#[test]
fn elasticsearch_latency_average_is_computed_per_region_over_successful_requests() {
    let case = Case::load("search/elasticsearch/latency-by-region");
    case.reference_is_source_bound();
    case.passes(&[
        "Slowest region: eu-west-1\nAverage duration: 412.5 ms",
        "eu-west-1 is the slowest, averaging 412.5ms (0.4125 s) per successful request.",
    ]);
    case.fails(&[
        "The slowest region is not eu-west-1 (412.5 ms).",
        "It is not region eu-west-1 (412.5 ms).",
        "Rather than eu-west-1 (412.5 ms), the slowest region is us-east-1.",
        // Another region with the winner's average, the winner with another
        // region's average, and the answers reached by counting error
        // responses or another endpoint.
        "us-east-1 is the slowest, averaging 412.5 ms.",
        "eu-west-1 is the slowest, averaging 305 ms.",
        "us-east-1 is the slowest, averaging 9134 ms.",
        "ap-southeast-2 is the slowest, averaging 575 ms.",
        // Near misses: a rounded value and the wrong unit.
        "eu-west-1 is the slowest, averaging 412 ms.",
        "eu-west-1 is the slowest, averaging 412.5 seconds.",
    ]);
    case.unresolved(&[
        // A bare number is not tied to the duration.
        "eu-west-1, 412.5",
        "eu-west-1 averages about four hundred milliseconds.",
        "It seems eu-west-1 is the slowest at 412.5 ms.",
        // The right region and value with the opposite claim.
        "eu-west-1 is the fastest at 412.5 ms.",
        "eu-west-1: 412.5 ms; ap-southeast-2: 305 ms; us-east-1: 210 ms.",
    ]);
    // A slower request raises the region's average; counting a timeout as a
    // success makes another region the slowest.
    let duration = "/services/elasticsearch/docs/requests-checkout-2026.10~1r0005/duration_ms";
    assert_eq!(
        case.outcome_after(duration, json!(500), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(duration, json!(500), "eu-west-1: 422.5 ms"),
        Outcome::Pass
    );
    assert_eq!(
        case.outcome_after(
            "/services/elasticsearch/docs/requests-checkout-2026.10~1r0018/status_code",
            json!(200),
            "us-east-1: 3933.75 ms"
        ),
        Outcome::Pass
    );
}

#[test]
fn elasticsearch_key_deletion_is_followed_to_the_github_account_and_its_pull_request() {
    let case = Case::load("search/multi/api-key-deletion-audit-github");
    case.reference_is_source_bound();
    case.passes(&[
        "User: m.okafor\nDeleted at: 2026-10-29T16:42:57Z\nGitHub username: mokafor\nPull request: #58 (open, not merged)",
        "m.okafor deleted the key at 16:42 UTC. On GitHub she is mokafor, and her PR #58 has not been merged yet.",
        "| User | Time | GitHub | PR | Status |\n|---|---|---|---|---|\n| m.okafor | 16:42 UTC | mokafor | #58 | Open (not merged) |",
        "m.okafor (16:42 UTC) is GitHub user mokafor (not sam); pull request number 58; merged: no.",
    ]);
    case.fails(&[
        "It was not m.okafor (16:42 UTC); GitHub user mokafor; PR #58 is open.",
        "Rather than m.okafor (16:42 UTC, GitHub mokafor, PR #58, open), the key was deleted by r.chen.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR #58 is not open.",
        // The same-surname GitHub account, the staging key's rotation, and
        // crossed pull-request numbers and states.
        "m.okafor (16:42 UTC) is GitHub user sam; PR #58 is open.",
        "r.chen (17:30 UTC) is GitHub user rchen; PR #57 was merged.",
        "j.doe (14:05 UTC) is GitHub user jdoe; PR #58 is open.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR #57 is open.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR #58 was merged.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR #58 is closed.",
        // Near misses.
        "m.okafor (16:43 UTC) is GitHub user mokafor; PR #58 is open.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR #85 is open.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR #58.",
    ]);
    case.unresolved(&[
        "I think m.okafor (16:42 UTC) is GitHub user mokafor; PR #58 is open.",
        "Okafor (16:42 UTC) is GitHub user mokafor; PR #58 is open.",
        "m.okafor (4:42 PM UTC) is GitHub user mokafor; PR #58 is open.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR fifty-eight is open.",
        // A bare number is not tied to the pull request.
        "m.okafor (16:42 UTC) is GitHub user mokafor; 58 is open.",
        "m.okafor (16:42 UTC) is not GitHub user mokafor; PR #58 is open.",
        "m.okafor (16:42 UTC) is GitHub user mokafor; PR #58 is open. rchen's PR #57 was merged.",
    ]);
    // The GitHub account follows the email that carries the actor's name,
    // the number follows the pull request, and so does its merged state.
    let other = [
        (
            "/services/github/users/mokafor/email",
            json!("mira@octo-org.example"),
        ),
        (
            "/services/github/users/sam/email",
            json!("m.okafor@octo-org.example"),
        ),
    ];
    assert_eq!(
        outcome_after_all(&case, &other, &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        outcome_after_all(
            &case,
            &other,
            "m.okafor (16:42 UTC) is GitHub user sam; PR #56 is open."
        ),
        Outcome::Pass
    );
    let pull = "/services/github/pulls/octo-org~1deploy-pipeline#58";
    let number = format!("{pull}/number");
    assert_eq!(
        case.outcome_after(&number, json!(61), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            &number,
            json!(61),
            "m.okafor (16:42 UTC) is GitHub user mokafor; PR #61 is open."
        ),
        Outcome::Pass
    );
    let merged = [
        (&*format!("{pull}/merged"), json!(true)),
        (&*format!("{pull}/state"), json!("closed")),
    ];
    assert_eq!(
        outcome_after_all(&case, &merged, &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        outcome_after_all(
            &case,
            &merged,
            "m.okafor (16:42 UTC) is GitHub user mokafor; PR #58 has been merged."
        ),
        Outcome::Pass
    );
}
