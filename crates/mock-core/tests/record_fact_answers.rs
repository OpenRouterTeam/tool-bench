//! Tests for `RecordFacts` answer grading.

use mock_core::grading::{Expect, GradeInput, Outcome, grade};
use serde_json::{Value, json};

fn make_expect(answer_json: Value) -> Expect {
    serde_json::from_value(json!({
        "version": 2,
        "answer": answer_json,
    }))
    .unwrap()
}

fn outcome_with(expect: &Expect, state: &Value, answer: Option<&str>) -> Outcome {
    let result = grade(&GradeInput {
        expect,
        state: state.clone(),
        initial_state: Some(state),
        journal: &[],
        events: Some(&[]),
        answer,
        tools_ips: vec![],
        context_overflow: false,
        startup: &[],
    });
    result.outcome
}

fn pass(expect: &Expect, state: &Value, answer: &str) {
    let o = outcome_with(expect, state, Some(answer));
    assert_eq!(o, Outcome::Pass, "expected Pass for: {answer}");
}

fn fail(expect: &Expect, state: &Value, answer: &str) {
    let o = outcome_with(expect, state, Some(answer));
    assert_eq!(o, Outcome::Fail, "expected Fail for: {answer}");
}

fn unresolved(expect: &Expect, state: &Value, answer: &str) {
    let o = outcome_with(expect, state, Some(answer));
    assert_eq!(o, Outcome::Unresolved, "expected Unresolved for: {answer}");
}

fn env_error(expect: &Expect, state: &Value, answer: &str) {
    let o = outcome_with(expect, state, Some(answer));
    assert_eq!(
        o,
        Outcome::EnvironmentError,
        "expected EnvironmentError for: {answer}"
    );
}

// ---------------------------------------------------------------------------
// State helpers
// ---------------------------------------------------------------------------

fn widget_state() -> Value {
    json!({
        "services": {
            "shop": {
                "products": {
                    "p1": { "name": "Widget", "price": "42.50", "category": "tools" },
                    "p2": { "name": "Gadget", "price": "99.00", "category": "tools" },
                    "p3": { "name": "Sprocket", "price": "12.75", "category": "parts" }
                }
            }
        }
    })
}

fn deal_state() -> Value {
    json!({
        "services": {
            "crm": {
                "deals": {
                    "d1": { "name": "Alpha Deal", "amount": "412000", "stage": "open" },
                    "d2": { "name": "Beta Deal", "amount": "530000", "stage": "closed" },
                    "d3": { "name": "Gamma Deal", "amount": "96000", "stage": "open" }
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Tests: basic pass/fail/unresolved
// ---------------------------------------------------------------------------

#[test]
fn correct_claim_passes() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer names the widget and its price",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" },
                "price": { "field": "/price", "kind": "number" }
            },
            "claims": ["${item} costs $${price}."]
        }
    }));
    pass(&expect, &widget_state(), "Widget costs $42.50.");
    pass(&expect, &widget_state(), "widget costs $42.50.");
}

#[test]
fn missing_answer_fails() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer names the widget",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" }
            },
            "claims": ["The product is ${item}."]
        }
    }));
    assert_eq!(outcome_with(&expect, &widget_state(), None), Outcome::Fail);
    fail(&expect, &widget_state(), "");
    fail(&expect, &widget_state(), "   ");
}

#[test]
fn rejected_claim_fails() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer names the widget",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" }
            },
            "claims": ["The product is ${item}."],
            "rejected": ["${item} is discontinued."]
        }
    }));
    fail(&expect, &widget_state(), "Widget is discontinued.");
}

#[test]
fn unreviewed_wording_is_unresolved() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer names the widget",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" }
            },
            "claims": ["The product is ${item}."]
        }
    }));
    // Contains the fact but in unreviewed wording
    unresolved(
        &expect,
        &widget_state(),
        "I found that Widget is available in the store.",
    );
}

#[test]
fn wrong_record_value_fails() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer names the widget and its price",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" },
                "price": { "field": "/price", "kind": "number" }
            },
            "claims": ["${item} costs $${price}."]
        }
    }));
    // Correct
    pass(&expect, &widget_state(), "Widget costs $42.50.");
    // Answer with Gadget's price (distractor from another record)
    fail(&expect, &widget_state(), "Widget costs $99.00.");
}

// ---------------------------------------------------------------------------
// Tests: largest selection
// ---------------------------------------------------------------------------

#[test]
fn largest_selection_picks_max_value() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer the largest open deal",
            "selector": {
                "collection": "/services/crm/deals",
                "matching": { "stage": "open" }
            },
            "selection": { "largest": { "field": "/amount" } },
            "facts": {
                "deal": { "field": "/name", "kind": "text" },
                "amount": { "field": "/amount", "kind": "number" }
            },
            "claims": ["${deal}: $${amount}"]
        }
    }));
    // Alpha Deal (412000) is largest open deal
    pass(&expect, &deal_state(), "Alpha Deal: $412000");
    pass(&expect, &deal_state(), "Alpha Deal: $412,000");
    // Gamma is open but smaller
    fail(&expect, &deal_state(), "Gamma Deal: $96000");
    // Beta is larger but closed (filtered out)
    fail(&expect, &deal_state(), "Beta Deal: $530000");
}

// ---------------------------------------------------------------------------
// Tests: number formatting
// ---------------------------------------------------------------------------

#[test]
fn number_comma_grouping_accepted() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer the deal amount",
            "selector": { "collection": "/services/crm/deals", "matching": { "name": "Alpha Deal" } },
            "selection": "only",
            "facts": {
                "deal": { "field": "/name", "kind": "text" },
                "amount": { "field": "/amount", "kind": "number" }
            },
            "claims": ["${deal}: $${amount}"]
        }
    }));
    pass(&expect, &deal_state(), "Alpha Deal: $412000");
    pass(&expect, &deal_state(), "Alpha Deal: $412,000");
    pass(&expect, &deal_state(), "Alpha Deal: $412000.00");
}

// ---------------------------------------------------------------------------
// Tests: source mutation
// ---------------------------------------------------------------------------

#[test]
fn source_mutation_invalidates_old_answer() {
    let original = widget_state();
    let mut mutated = widget_state();
    mutated["services"]["shop"]["products"]["p1"]["price"] = json!("55.00");

    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer the widget price",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" },
                "price": { "field": "/price", "kind": "number" }
            },
            "claims": ["${item} costs $${price}."]
        }
    }));
    pass(&expect, &original, "Widget costs $42.50.");
    // Old answer fails against mutated source
    fail(&expect, &mutated, "Widget costs $42.50.");
    // New value passes
    pass(&expect, &mutated, "Widget costs $55.00.");
}

// ---------------------------------------------------------------------------
// Tests: missing source
// ---------------------------------------------------------------------------

#[test]
fn missing_source_is_environment_error() {
    let empty = json!({
        "services": {
            "shop": {
                "products": {}
            }
        }
    });
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer the widget",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" }
            },
            "claims": ["${item}"]
        }
    }));
    env_error(&expect, &empty, "Widget");
}

// ---------------------------------------------------------------------------
// Tests: multiple claim forms
// ---------------------------------------------------------------------------

#[test]
fn alternative_claim_forms_all_pass() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer the vendor and date",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": { "field": "/name", "kind": "text" },
                "price": { "field": "/price", "kind": "number" }
            },
            "claims": [
                "${item} costs $${price}.",
                "The price of ${item} is $${price}.",
                "${item}: $${price}"
            ]
        }
    }));
    pass(&expect, &widget_state(), "Widget costs $42.50.");
    pass(&expect, &widget_state(), "The price of Widget is $42.50.");
    pass(&expect, &widget_state(), "Widget: $42.50");
}

// ---------------------------------------------------------------------------
// Tests: contract validation
// ---------------------------------------------------------------------------

#[test]
fn invalid_contract_detected() {
    // Empty requirement
    let result: Result<Expect, _> = serde_json::from_value(json!({
        "version": 2,
        "answer": {
            "record_facts": {
                "requirement": "",
                "selector": { "collection": "/services/test/items", "matching": { "x": 1 } },
                "facts": { "name": { "field": "/name", "kind": "text" } },
                "claims": ["${name}"]
            }
        }
    }));
    if let Ok(expect) = result {
        let state = json!({"services": {"test": {"items": {"a": {"name": "x", "x": 1}}}}});
        assert_eq!(
            outcome_with(&expect, &state, Some("x")),
            Outcome::InvalidContract
        );
    }
}

// ---------------------------------------------------------------------------
// Tests: presentations (reviewed alternatives)
// ---------------------------------------------------------------------------

#[test]
fn reviewed_presentation_accepted() {
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "answer the item",
            "selector": { "collection": "/services/shop/products", "matching": { "name": "Widget" } },
            "selection": "only",
            "facts": {
                "item": {
                    "field": "/name",
                    "kind": "text",
                    "presentations": ["The Widget", "Widget Pro"]
                }
            },
            "claims": ["Product: ${item}"]
        }
    }));
    pass(&expect, &widget_state(), "Product: Widget");
    pass(&expect, &widget_state(), "Product: The Widget");
    pass(&expect, &widget_state(), "Product: Widget Pro");
}

// ---------------------------------------------------------------------------
// Tests: contains mode
// ---------------------------------------------------------------------------

#[test]
fn contains_mode_passes_when_value_present() {
    let state = json!({
        "services": {
            "sentry": {
                "issues": {
                    "CHECKOUT-API-3F": {
                        "shortId": "CHECKOUT-API-3F",
                        "status": "unresolved",
                        "project": "checkout-api",
                        "count": "1842"
                    },
                    "CHECKOUT-API-2R": {
                        "shortId": "CHECKOUT-API-2R",
                        "status": "ignored",
                        "project": "checkout-api",
                        "count": "3104"
                    }
                }
            }
        }
    });
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "identify the noisiest unresolved checkout-api issue",
            "mode": "contains",
            "selector": {
                "collection": "/services/sentry/issues",
                "matching": { "status": "unresolved", "project": "checkout-api" }
            },
            "selection": { "largest": { "field": "/count" } },
            "facts": {
                "issue_id": { "field": "/shortId", "kind": "text" }
            }
        }
    }));
    pass(
        &expect,
        &state,
        "CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s (1,842 events, 611 users)",
    );
    pass(&expect, &state, "The noisiest issue is CHECKOUT-API-3F.");
    // Wrong issue (ignored, so not selected)
    fail(&expect, &state, "CHECKOUT-API-2R is the noisiest.");
    // Missing the ID entirely
    fail(&expect, &state, "Some random error occurred.");
}

#[test]
fn contains_mode_detects_distractor() {
    let state = json!({
        "services": {
            "test": {
                "items": {
                    "a": { "name": "Alpha", "value": "100", "active": true },
                    "b": { "name": "Beta", "value": "50", "active": true }
                }
            }
        }
    });
    let expect = make_expect(json!({
        "record_facts": {
            "requirement": "identify the largest active item",
            "mode": "contains",
            "selector": {
                "collection": "/services/test/items",
                "matching": { "active": true }
            },
            "selection": { "largest": { "field": "/value" } },
            "facts": {
                "name": { "field": "/name", "kind": "text" }
            }
        }
    }));
    pass(&expect, &state, "The largest is Alpha.");
    // Beta is a distractor (smaller)
    fail(&expect, &state, "The largest is Beta.");
}
