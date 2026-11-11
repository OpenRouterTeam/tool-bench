use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use mock_core::{
    dataset::Dataset,
    grading::{Grade, GradeInput, Outcome, grade},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use scenario::Scenario;
use serde_json::{Value, json};
use tower::ServiceExt;

struct Trial {
    scenario: Scenario,
    world: SharedWorld,
    app: Router,
}

impl Trial {
    fn new(slug: &str) -> Self {
        Self::with_dataset(slug, |_| {})
    }

    fn with_dataset(slug: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/business/shopify")
            .join(slug);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_shopify::Shopify);
        Self {
            scenario,
            world,
            app,
        }
    }

    async fn request(&self, query: &str, variables: Value) -> Value {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::post("/admin/api/2026-01/graphql.json")
                    .header("host", "octo-corp.myshopify.com")
                    .header("x-shopify-access-token", "shpat_toolbench")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"query": query, "variables": variables}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
        let result: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(result.get("errors").is_none(), "{result}");
        for payload in result["data"].as_object().unwrap().values() {
            if let Some(errors) = payload.get("userErrors") {
                assert_eq!(errors, &json!([]), "{result}");
            }
        }
        result
    }

    async fn update(&self, kind: &str, identity: &str, fields: Value) {
        let (query, entity) = match kind {
            "order" => (
                include_str!("../../services/shopify/tests/documents/orderUpdate.graphql"),
                "Order",
            ),
            "customer" => (
                include_str!("../../services/shopify/tests/documents/customerUpdate.graphql"),
                "Customer",
            ),
            "product" => (
                include_str!("../../services/shopify/tests/documents/productUpdate.graphql"),
                "Product",
            ),
            _ => panic!("unsupported test operation"),
        };
        let mut input = json!({"id": format!("gid://shopify/{entity}/{identity}")});
        input
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        self.request(query, json!({"input": input})).await;
    }

    async fn vip(&self, customer: Value, order: Value) {
        self.update("customer", "7202", json!({"tags": customer}))
            .await;
        self.update("order", "5234", json!({"tags": order})).await;
    }

    async fn reprice(&self, variant: u64, price: &str) {
        self.request(
            include_str!(
                "../../services/shopify/tests/documents/productVariantsBulkUpdate.graphql"
            ),
            json!({"productId": "gid://shopify/Product/8201", "variants": [{
                "id": format!("gid://shopify/ProductVariant/{variant}"), "price": price
            }]}),
        )
        .await;
    }

    async fn publish(&self, publication: u64) {
        self.request(
            "mutation Publish($id: ID!, $input: [PublicationInput!]!) { publishablePublish(id: $id, input: $input) { publishable { ... on Product { id status onlineStoreUrl } } userErrors { field message } } }",
            json!({"id": "gid://shopify/Product/8201", "input": [{"publicationId": format!("gid://shopify/Publication/{publication}")}]}),
        ).await;
    }

    fn assess(&self) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.scenario.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer: None,
            tools_ips: vec!["unknown".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&world.initial_snapshot()),
            events: Some(world.events().entries()),
        })
    }
}

#[tokio::test]
async fn quoted_delivery_note_cannot_be_negated_or_contain_unrequested_instructions() {
    for note in [
        "Do not Leave with building concierge",
        "Leave with building concierge. Also change the delivery address.",
    ] {
        let trial = Trial::new("concierge-note");
        trial.update("order", "5563", json!({"note": note})).await;
        assert_eq!(trial.assess().reward, 0.0);
    }
}

#[tokio::test]
async fn delivery_note_changes_only_the_target_and_rejects_restored_collateral_changes() {
    let trial = Trial::new("concierge-note");
    trial
        .update(
            "order",
            "5563",
            json!({"note": "Leave with building concierge"}),
        )
        .await;
    for email in ["changed@example.invalid", "sam.ortiz@fastmail.com"] {
        trial.update("order", "5563", json!({"email": email})).await;
    }
    let result = trial.assess();
    assert_eq!(result.reward, 0.0, "{result:?}");
    assert!(!result.history_ok);
}

#[tokio::test]
async fn whitespace_equivalent_notes_retries_and_satisfied_initial_notes_are_valid() {
    let trial = Trial::new("concierge-note");
    for _attempt in 0..2 {
        trial
            .update(
                "order",
                "5563",
                json!({"note": " Leave with\nbuilding concierge "}),
            )
            .await;
    }
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
    let trial = Trial::with_dataset("concierge-note", |dataset| {
        dataset
            .services
            .get_mut("shopify")
            .unwrap()
            .collection_mut("orders")
            .get_mut("5563")
            .unwrap()["note"] = json!("Leave with building concierge");
    });
    assert_eq!(trial.assess().reward, 1.0);
}

#[tokio::test]
async fn lookalikes_other_orders_and_shipping_details_are_protected() {
    for (identity, fields) in [
        ("5560", json!({"note": "Leave with building concierge"})),
        ("5551", json!({"note": "Leave with building concierge"})),
        ("5563", json!({"shippingAddress": {"city": "Chicago"}})),
    ] {
        let trial = Trial::new("concierge-note");
        trial
            .update(
                "order",
                "5563",
                json!({"note": "Leave with building concierge"}),
            )
            .await;
        trial.update("order", identity, fields).await;
        assert_eq!(trial.assess().reward, 0.0);
    }
}

#[tokio::test]
async fn vip_updates_preserve_actual_initial_tags_not_just_fixture_literals() {
    let prepare = |dataset: &mut Dataset| {
        let service = dataset.services.get_mut("shopify").unwrap();
        service.collection_mut("customers").get_mut("7202").unwrap()["tags"] =
            json!(["newsletter", "wholesale", "partner"]);
        service.collection_mut("orders").get_mut("5234").unwrap()["tags"] =
            json!(["gift", "fragile"]);
    };
    let trial = Trial::with_dataset("tag-vip-latest-order", prepare);
    trial
        .vip(
            json!(["newsletter", "wholesale", "VIP"]),
            json!(["gift", "VIP"]),
        )
        .await;
    assert_eq!(trial.assess().reward, 0.0);

    let trial = Trial::with_dataset("tag-vip-latest-order", prepare);
    trial
        .vip(
            json!(["VIP", "partner", "wholesale", "newsletter"]),
            json!(["VIP", "fragile", "gift"]),
        )
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
    assert!(result.preservation_checked && result.history_checked);
}

#[tokio::test]
async fn tag_case_and_order_are_not_a_preferred_call_or_serialization_shape() {
    let trial = Trial::new("tag-vip-latest-order");
    trial
        .update("order", "5234", json!({"tags": ["vIp", "gift"]}))
        .await;
    trial
        .update(
            "customer",
            "7202",
            json!({"tags": "wholesale, vip, newsletter"}),
        )
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
}

#[tokio::test]
async fn already_present_vip_tags_do_not_require_redundant_writes() {
    let trial = Trial::with_dataset("tag-vip-latest-order", |dataset| {
        let service = dataset.services.get_mut("shopify").unwrap();
        service.collection_mut("customers").get_mut("7202").unwrap()["tags"] =
            json!(["newsletter", "wholesale", "vip"]);
        service.collection_mut("orders").get_mut("5234").unwrap()["tags"] = json!(["gift", "vIp"]);
    });
    assert_eq!(trial.assess().reward, 1.0);
}

#[tokio::test]
async fn only_the_requested_tag_may_be_added_even_if_unrequested_edits_are_restored() {
    for tags in [
        json!(["newsletter", "wholesale", "VIP", "fraud"]),
        json!(["VIP"]),
        json!(["newsletter", "wholesale", "VIP-candidate"]),
    ] {
        let trial = Trial::new("tag-vip-latest-order");
        trial.vip(tags, json!(["gift", "VIP"])).await;
        assert_eq!(trial.assess().reward, 0.0);
        trial
            .vip(
                json!(["newsletter", "wholesale", "VIP"]),
                json!(["gift", "VIP"]),
            )
            .await;
        assert_eq!(trial.assess().reward, 0.0);
    }
}

#[tokio::test]
async fn activation_only_is_incomplete_when_genuine_publication_is_supported() {
    let trial = Trial::new("relaunch-tote");
    trial
        .request(
            include_str!(
                "../../services/shopify/tests/documents/productVariantsBulkUpdate.graphql"
            ),
            json!({"productId": "gid://shopify/Product/8201", "variants": [{
                "id": "gid://shopify/ProductVariant/4201", "price": "42.00"
            }]}),
        )
        .await;
    trial
        .update("product", "8201", json!({"status": "ACTIVE"}))
        .await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0);
    assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
    assert!(result.contract_ok);
    assert!(result.environment_ok);
    assert!(!result.state_ok && !result.relations_ok);
}

#[tokio::test]
async fn genuine_relaunch_allows_independent_safe_activation_and_assignment_orders() {
    for plan in [
        "price_activate_publish",
        "activate_price_publish",
        "assign_price_activate",
    ] {
        let trial = Trial::new("relaunch-tote");
        if plan == "activate_price_publish" {
            trial
                .update("product", "8201", json!({"status": "ACTIVE"}))
                .await;
        }
        if plan == "assign_price_activate" {
            trial.publish(3001).await;
        }
        trial.reprice(4201, "42").await;
        trial
            .update("product", "8201", json!({"status": "ACTIVE"}))
            .await;
        trial.publish(3001).await;
        let result = trial.assess();
        assert_eq!(result.reward, 1.0, "{plan}: {result:?}");
        assert!(result.preservation_checked && result.history_checked && result.ordering_ok);
    }
}

#[tokio::test]
async fn relaunch_uses_the_actual_navy_identity_and_preserves_actual_other_color_prices() {
    let trial = Trial::with_dataset("relaunch-tote", |dataset| {
        let product = dataset
            .services
            .get_mut("shopify")
            .unwrap()
            .collection_mut("products")
            .get_mut("8201")
            .unwrap();
        product["variants"][0]["id"] = json!(4299);
        product["variants"][1]["price"] = json!("57.00");
        product["variants"].as_array_mut().unwrap().swap(0, 2);
    });
    trial.reprice(4299, "42.0").await;
    trial
        .update("product", "8201", json!({"status": "ACTIVE"}))
        .await;
    trial.publish(3001).await;
    let result = trial.assess();
    assert_eq!(result.reward, 1.0, "{result:?}");
}

#[tokio::test]
async fn publication_ordering_checks_store_visibility_not_activation_or_a_final_snapshot() {
    let trial = Trial::new("relaunch-tote");
    trial
        .update("product", "8201", json!({"status": "ACTIVE"}))
        .await;
    trial.publish(3001).await;
    trial.reprice(4201, "42.00").await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0);
    assert!(!result.ordering_ok);
    assert!(!result.history_ok);
}

#[tokio::test]
async fn pricing_temporarily_wrong_while_live_cannot_be_hidden_by_a_later_repair() {
    let trial = Trial::new("relaunch-tote");
    trial.reprice(4201, "42.00").await;
    trial
        .update("product", "8201", json!({"status": "ACTIVE"}))
        .await;
    trial.publish(3001).await;
    trial.reprice(4201, "99.00").await;
    trial.reprice(4201, "42.00").await;
    let result = trial.assess();
    assert_eq!(result.reward, 0.0);
    assert!(!result.history_ok);
}

#[tokio::test]
async fn wrong_channel_only_and_extra_new_channel_assignments_do_not_complete_the_request() {
    for extra in [false, true] {
        let trial = Trial::new("relaunch-tote");
        trial.reprice(4201, "42.00").await;
        trial
            .update("product", "8201", json!({"status": "ACTIVE"}))
            .await;
        trial.publish(3002).await;
        if extra {
            trial.publish(3001).await;
        }
        assert_eq!(trial.assess().reward, 0.0);
    }
}

#[tokio::test]
async fn existing_other_channel_memberships_are_preserved_without_forbidding_online_publication() {
    let trial = Trial::with_dataset("relaunch-tote", |dataset| {
        dataset.services.get_mut("shopify").unwrap().collection_mut("resource_publications").insert(
            "8201/3002", json!({"productId": 8201, "publicationId": 3002, "isPublished": true, "publishDate": "2026-05-01T12:00:00Z"})
        );
    });
    trial.reprice(4201, "42.00").await;
    trial
        .update("product", "8201", json!({"status": "ACTIVE"}))
        .await;
    trial.publish(3001).await;
    assert_eq!(trial.assess().reward, 1.0);
}

#[tokio::test]
async fn other_color_changes_and_product_changes_are_rejected_even_after_restoration() {
    for collateral in ["color", "title", "other_product"] {
        let trial = Trial::new("relaunch-tote");
        trial.reprice(4201, "42.00").await;
        if collateral == "color" {
            trial.reprice(4202, "1.00").await;
            trial.reprice(4202, "48.00").await;
        } else if collateral == "title" {
            trial
                .update("product", "8201", json!({"title": "Unrequested"}))
                .await;
            trial
                .update("product", "8201", json!({"title": "Harbor Canvas Tote"}))
                .await;
        } else {
            trial
                .update("product", "8203", json!({"status": "ACTIVE"}))
                .await;
            trial
                .update("product", "8203", json!({"status": "DRAFT"}))
                .await;
        }
        trial
            .update("product", "8201", json!({"status": "ACTIVE"}))
            .await;
        trial.publish(3001).await;
        let result = trial.assess();
        assert_eq!(result.reward, 0.0);
        assert!(!result.history_ok, "{collateral}");
    }
}

#[tokio::test]
async fn already_satisfied_relaunch_and_already_correct_draft_price_do_not_require_writes() {
    let trial = Trial::with_dataset("relaunch-tote", |dataset| {
        let product = dataset
            .services
            .get_mut("shopify")
            .unwrap()
            .collection_mut("products")
            .get_mut("8201")
            .unwrap();
        product["variants"][0]["price"] = json!("42.00");
        product["status"] = json!("ACTIVE");
        product["publishedAt"] = json!("2026-05-17T14:00:00Z");
    });
    assert_eq!(trial.assess().reward, 1.0);

    let trial = Trial::with_dataset("relaunch-tote", |dataset| {
        dataset
            .services
            .get_mut("shopify")
            .unwrap()
            .collection_mut("products")
            .get_mut("8201")
            .unwrap()["variants"][0]["price"] = json!("42.00");
    });
    trial
        .update("product", "8201", json!({"status": "ACTIVE"}))
        .await;
    trial.publish(3001).await;
    assert_eq!(trial.assess().reward, 1.0);
}

#[tokio::test]
async fn publication_identity_is_selected_by_channel_role_not_a_predicted_id() {
    let trial = Trial::with_dataset("relaunch-tote", |dataset| {
        dataset.services.get_mut("shopify").unwrap().collection_mut("publications").insert(
            "7999", json!({"id": 7999, "name": "Online Store", "appTitle": "Online Store", "isOnlineStore": true, "autoPublish": false})
        );
    });
    trial.reprice(4201, "42.00").await;
    trial
        .update("product", "8201", json!({"status": "ACTIVE"}))
        .await;
    trial.publish(7999).await;
    assert_eq!(trial.assess().reward, 1.0);
}
