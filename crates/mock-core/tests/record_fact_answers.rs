//! Interface tests for `record_facts` answer grading on small authored
//! states.  Scenario-level counterexamples live in `statement_answers_*.rs`.

use mock_core::grading::{Expect, GradeInput, Outcome, grade};
use serde_json::{Value, json};

fn contract(answer: Value) -> Result<Expect, serde_json::Error> {
    serde_json::from_value(json!({"version": 2, "answer": {"record_facts": answer}}))
}

fn outcome_with(expect: &Expect, initial: Option<&Value>, answer: Option<&str>) -> Outcome {
    grade(&GradeInput {
        expect,
        state: initial.cloned().unwrap_or_else(|| json!({"services": {}})),
        initial_state: initial,
        journal: &[],
        events: Some(&[]),
        answer,
        tools_ips: vec![],
        context_overflow: false,
        startup: &[],
    })
    .outcome
}

#[track_caller]
fn all(expect: &Expect, state: &Value, expected: Outcome, answers: &[&str]) {
    for answer in answers {
        assert_eq!(
            outcome_with(expect, Some(state), Some(answer)),
            expected,
            "{answer}"
        );
    }
}

fn shop() -> Value {
    json!({"services": {"shop": {
        "orders": {
            "o1": {"customer": "c1", "status": "paid", "total": "0.10", "placed": "2026-03-02T10:00:00Z",
                   "items": [{"sku": "a", "price": "19.99", "qty": 3}, {"sku": "b", "price": "5", "qty": 1}]},
            "o2": {"customer": "c1", "status": "paid", "total": "0.20", "placed": "2026-03-09T10:00:00Z",
                   "items": [{"sku": "a", "price": "19.99", "qty": 1}]},
            "o3": {"customer": "c2", "status": "paid", "total": "0.25", "placed": "2026-03-11T10:00:00Z",
                   "items": [{"sku": "b", "price": "5", "qty": 2}]},
            "o4": {"customer": "c2", "status": "refunded", "total": "900", "placed": "2026-03-12T10:00:00Z",
                   "items": []}
        },
        "customers": {
            "c1": {"id": "c1", "first": "Ada", "last": "Lovelace", "login": "ada", "email": "ada@example.test", "tier": "GOLD"},
            "c2": {"id": "c2", "first": "Alan", "last": "Turing", "login": "alan", "email": "alan@example.test", "tier": "BASIC"}
        },
        "notes": {
            "o1/1": {"body": "Escalated by agent ada on January 15, 2027. Ref: case #4411."},
            "o3/1": {"body": "Escalated by agent alan on February 2, 2027. Ref: case #5522."}
        },
        "probes": {
            "p1": {"name": "checkout", "latency_ms": 1843, "at": 1793284500},
            "p2": {"name": "search", "latency_ms": 250, "at": 1793280000}
        }
    }}})
}

fn top_customer() -> Expect {
    contract(json!({
        "requirement": "name the customer with the largest paid total and that total",
        "selector": {"collection": "/services/shop/orders", "matching": {"status": "paid"}},
        "selection": {"group": {"by": "/customer", "measure": {"sum": "/total"}, "pick": "largest"}},
        "related": {"customer": {"collection": "/services/shop/customers", "on": {"/id": "/key"}}},
        "facts": {
            "name": {"record": "customer", "field": "/first", "append": ["/last"], "also": ["/login"], "kind": "text"},
            "total": {"field": "/value", "kind": "money"}
        }
    }))
    .unwrap()
}

#[test]
fn grouped_totals_use_exact_decimal_arithmetic_and_join_to_the_named_record() {
    let expect = top_customer();
    let state = shop();
    // 0.10 + 0.20 is exactly 0.30, which beats 0.25.
    all(
        &expect,
        &state,
        Outcome::Pass,
        &[
            "Ada Lovelace spent the most: $0.30.",
            "Top customer: ada (0.3 USD)",
            // A hedge still states the answer, and a ranking that leads with it
            // names the other customer as context.
            "Probably Ada Lovelace with $0.30.",
            "Ada Lovelace: $0.30; Alan Turing: $0.25.",
        ],
    );
    all(
        &expect,
        &state,
        Outcome::Fail,
        &[
            "",
            "It is not Ada Lovelace; the total is not $0.30.",
            "Alan Turing spent the most: $0.30.",
            "Ada Lovelace spent the most: $0.25.",
            "Ada Lovelace spent the most: $0.31.",
            "Ada Lovelace spent the most: €0.30.",
            "Ada Lovelace spent the most.",
        ],
    );
    all(
        &expect,
        &state,
        Outcome::Unresolved,
        &[
            "Lovelace spent the most: $0.30.",
            "Ada Lovelace spent thirty cents.",
        ],
    );
    assert_eq!(outcome_with(&expect, Some(&state), None), Outcome::Fail);
}

#[test]
fn a_missing_or_ambiguous_source_is_never_scored() {
    let expect = top_customer();
    let answer = Some("Ada Lovelace spent the most: $0.30.");
    assert_eq!(
        outcome_with(&expect, None, answer),
        Outcome::EnvironmentError
    );
    let mut state = shop();
    state["services"]["shop"]["customers"] = json!({});
    assert_eq!(
        outcome_with(&expect, Some(&state), answer),
        Outcome::EnvironmentError
    );
    let mut state = shop();
    state["services"]["shop"]["orders"]["o1"]["total"] = json!("ten cents");
    assert_eq!(
        outcome_with(&expect, Some(&state), answer),
        Outcome::EnvironmentError
    );
    // A tie has no single requested group: adjudicate, do not guess.
    let mut state = shop();
    state["services"]["shop"]["orders"]["o3"]["total"] = json!("0.30");
    assert_eq!(
        outcome_with(&expect, Some(&state), answer),
        Outcome::Unresolved
    );
}

#[test]
fn unnested_products_listed_rows_and_rounded_averages_are_bounded_computations() {
    let state = shop();
    let revenue = contract(json!({
        "requirement": "state the best-selling SKU by revenue and its revenue",
        "selector": {"collection": "/services/shop/orders", "matching": {"status": "paid"}},
        "selection": {"group": {
            "unnest": "/items", "by": "/item/sku",
            "measure": {"sum_product": ["/item/price", "/item/qty"]}, "pick": "largest"
        }},
        "facts": {
            "sku": {"field": "/key", "kind": "identifier"},
            "revenue": {"field": "/value", "kind": "money"}
        }
    }))
    .unwrap();
    all(
        &revenue,
        &state,
        Outcome::Pass,
        &["SKU a brought in $79.96."],
    );
    all(
        &revenue,
        &state,
        Outcome::Fail,
        &["SKU b brought in $79.96.", "SKU a brought in $79.95."],
    );

    let orders = contract(json!({
        "requirement": "list every paid order of customer c1 and their average total",
        "selector": {"collection": "/services/shop/orders", "matching": {"status": "paid", "customer": "c1"}},
        "selection": {"group": {"measure": {"average": "/total"}}},
        "facts": {
            "placed": {"field": "/members/placed", "capture": "^(\\d{4}-\\d{2}-\\d{2})", "kind": "date"},
            "average": {"field": "/value", "kind": "money", "rounding": true}
        }
    }))
    .unwrap();
    all(
        &orders,
        &state,
        Outcome::Pass,
        &[
            "Orders on March 2, 2026 and 2026-03-09 average $0.15.",
            "Orders on 2 March 2026 and Mar 9 average about $0.2.",
        ],
    );
    all(
        &orders,
        &state,
        Outcome::Fail,
        &[
            "Only the order on March 2, 2026; the average is $0.15.",
            "Orders on March 2, 2026 and March 11, 2026 average $0.15.",
            "Orders on March 2, 2026 and March 9, 2026 average $0.25.",
        ],
    );
    all(
        &orders,
        &state,
        Outcome::Unresolved,
        &["Orders on 03/02/2026 and 03/09/2026 average $0.15."],
    );
}

#[test]
fn joins_may_keep_every_match_use_the_record_key_or_compare_a_captured_part() {
    let state = shop();
    let expect = contract(json!({
        "requirement": "for the refunded order's customer, list each paid order total, the case in the first paid order's note, and the note date",
        "selector": {"collection": "/services/shop/orders", "matching": {"status": "refunded"}},
        "related": {
            "paid": {
                "collection": "/services/shop/orders",
                "filter": {"matching": {"/status": "paid"}},
                "on": {"/customer": "/customer"},
                "pick": "all"
            },
            "first_paid": {
                "collection": "/services/shop/orders",
                "filter": {"matching": {"/status": "paid"}},
                "on": {"/customer": "/customer"}
            },
            "note": {
                "from": "first_paid",
                "collection": "/services/shop/notes",
                "on": {"/_key": "/_key"},
                "capture": {"/_key": "^([^/]+)/"}
            },
            "agent": {
                "from": "note",
                "collection": "/services/shop/customers",
                "on": {"/email": "/body"},
                "capture": {"/email": "^([^@]+)@"},
                "capture_from": {"/body": "by agent (\\w+)"}
            }
        },
        "facts": {
            "totals": {"record": "paid", "field": "/total", "kind": "money"},
            "case": {"record": "note", "field": "/body", "capture": "case #(\\d+)", "kind": "number", "units": ["case", "#"], "require_unit": true},
            "noted": {"record": "note", "field": "/body", "kind": "date"},
            "agent": {"record": "agent", "field": "/last", "kind": "text"}
        }
    }))
    .unwrap();
    all(
        &expect,
        &state,
        Outcome::Pass,
        &["Paid order total $0.25; case #5522 was noted by Turing on 2027-02-02."],
    );
    all(
        &expect,
        &state,
        Outcome::Fail,
        &[
            "Paid order total $0.25; case #4411 was noted by Turing on 2027-02-02.",
            "Paid order total $0.25; case #5522 was noted by Lovelace on 2027-02-02.",
            "Paid order total $0.25; case #5522 was noted by Turing on 2027-01-15.",
        ],
    );
    all(
        &expect,
        &state,
        Outcome::Unresolved,
        &["Paid order total $0.25; 5522 was noted by Turing on 2027-02-02."],
    );
}

#[test]
fn wordings_units_and_clock_times_follow_the_source_value() {
    let expect = contract(json!({
        "requirement": "state the slowest probe, its latency, when it ran (UTC) and the tier of customer c1",
        "selector": {"collection": "/services/shop/probes", "field_patterns": {"/name": "."}},
        "selection": {"largest": {"field": "/latency_ms"}},
        "related": {"gold": {"collection": "/services/shop/customers", "filter": {"matching": {"/id": "c1"}}}},
        "facts": {
            "probe": {"field": "/name", "kind": "text"},
            "latency": {"field": "/latency_ms", "kind": "number", "units": ["ms", "milliseconds"], "require_unit": true},
            "at": {"field": "/at", "kind": "text", "clock": true},
            "tier": {"record": "gold", "field": "/tier", "kind": "text", "wordings": {"GOLD": ["top tier"], "BASIC": ["entry tier"]}}
        }
    }))
    .unwrap();
    let state = shop();
    all(
        &expect,
        &state,
        Outcome::Pass,
        &[
            "checkout peaked at 1843 ms at 14:35 UTC; c1 is GOLD.",
            "checkout peaked at 1.843 s at 14:35; c1 is in the top tier.",
            "checkout peaked at 1843 ms at 2:35 PM; c1 is GOLD.",
        ],
    );
    all(
        &expect,
        &state,
        Outcome::Fail,
        &[
            // The right digits in the wrong unit, a near miss, and another probe.
            "checkout peaked at 1843 s at 14:35; c1 is GOLD.",
            "checkout peaked at 1834 ms at 14:35; c1 is GOLD.",
            "search peaked at 1843 ms at 14:35; c1 is GOLD.",
            "checkout peaked at 1843 ms at 14:53; c1 is GOLD.",
            "checkout peaked at 1843 ms at 14:35; c1 is in the entry tier.",
        ],
    );
    all(
        &expect,
        &state,
        Outcome::Unresolved,
        &["checkout peaked at 1843 at 14:35; c1 is GOLD."],
    );
    // A wording belongs to one source value: once the tier changes it no
    // longer states the fact.
    let mut state = shop();
    state["services"]["shop"]["customers"]["c1"]["tier"] = json!("BASIC");
    all(
        &expect,
        &state,
        Outcome::Fail,
        &["checkout peaked at 1843 ms at 14:35; c1 is in the top tier."],
    );
    all(
        &expect,
        &state,
        Outcome::Pass,
        &["checkout peaked at 1843 ms at 14:35; c1 is in the entry tier."],
    );
}

#[test]
fn reviewed_forms_predicates_and_contradictions_shape_the_reading() {
    let expect = contract(json!({
        "requirement": "name the slowest probe and its latency",
        "selector": {"collection": "/services/shop/probes", "field_patterns": {"/name": "."}},
        "selection": {"largest": {"field": "/latency_ms"}},
        "facts": {
            "probe": {"field": "/name", "kind": "text"},
            "latency": {"field": "/latency_ms", "kind": "number", "units": ["ms"], "require_unit": true}
        },
        "rejected": ["${probe} is healthy"],
        "context": ["(search stays at 250 ms)"],
        "predicates": ["did not recover"],
        "contradictions": ["fastest", "quickest"]
    }))
    .unwrap();
    let state = shop();
    all(
        &expect,
        &state,
        Outcome::Pass,
        &[
            "checkout is the slowest at 1843 ms (search stays at 250 ms).",
            "checkout did not recover: 1843 ms.",
            // The other probe, named after the answer, is context without a reviewed form.
            "checkout is the slowest at 1843 ms; search stays at 250 ms.",
        ],
    );
    all(
        &expect,
        &state,
        Outcome::Fail,
        &["checkout is healthy at 1843 ms."],
    );
    all(
        &expect,
        &state,
        Outcome::Unresolved,
        &[
            "checkout is the fastest at 1843 ms.",
            "checkout did not degrade: 1843 ms.",
        ],
    );
}

#[test]
fn malformed_and_legacy_contracts_are_rejected_at_authoring() {
    let base = || {
        json!({
            "requirement": "name the slowest probe",
            "selector": {"collection": "/services/shop/probes", "field_patterns": {"/name": "."}},
            "selection": {"largest": {"field": "/latency_ms"}},
            "facts": {"probe": {"field": "/name", "kind": "text"}}
        })
    };
    // The substring and whole-text modes no longer exist.
    for (field, value) in [
        ("mode", json!("contains")),
        ("mode", json!("claim")),
        ("claims", json!(["${probe}"])),
    ] {
        let mut legacy = base();
        legacy[field] = value;
        assert!(contract(legacy).is_err(), "{field}");
    }
    let mut presentations = base();
    presentations["facts"]["probe"]["presentations"] = json!(["checkout"]);
    assert!(contract(presentations).is_err());

    let state = shop();
    let invalid = |edit: &dyn Fn(&mut Value)| {
        let mut answer = base();
        edit(&mut answer);
        let expect = contract(answer).expect("the contract parses");
        outcome_with(&expect, Some(&state), Some("checkout"))
    };
    assert_eq!(invalid(&|_| {}), Outcome::Pass);
    let cases: [&dyn Fn(&mut Value); 9] = [
        &|answer| answer["requirement"] = json!(" "),
        &|answer| answer["selector"]["created"] = json!(true),
        &|answer| answer["facts"]["probe"]["record"] = json!("missing"),
        &|answer| answer["facts"]["probe"]["capture"] = json!("(a)(b)"),
        &|answer| answer["facts"]["probe"]["units"] = json!(["ms"]),
        &|answer| answer["facts"]["probe"]["min_prefix"] = json!(3),
        &|answer| answer["rejected"] = json!(["${other} is healthy"]),
        &|answer| answer["related"] = json!({"a": {"collection": "/services/shop/customers"}}),
        &|answer| {
            answer["selection"] = json!({"group": {"measure": "count", "pick": "largest"}});
        },
    ];
    for (index, edit) in cases.iter().enumerate() {
        assert_eq!(invalid(edit), Outcome::InvalidContract, "case {index}");
    }
    // A join cycle has no starting record.
    assert_eq!(
        invalid(&|answer| {
            answer["related"] = json!({
                "a": {"from": "b", "collection": "/services/shop/customers", "on": {"/id": "/id"}},
                "b": {"from": "a", "collection": "/services/shop/customers", "on": {"/id": "/id"}}
            });
        }),
        Outcome::InvalidContract
    );
}
