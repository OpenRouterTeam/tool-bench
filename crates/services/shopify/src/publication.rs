use mock_core::{store::Store, world::ServiceState};
use serde_json::{Value, json};

use crate::model::{self, find, id_of, key_of, normalize_product, reference};

pub fn initialize(state: &mut ServiceState, _now: i64) {
    if ["products", "customers", "orders"].iter().all(|name| {
        state
            .store
            .collection(name)
            .is_none_or(|rows| rows.is_empty())
    }) {
        return;
    }
    if state.store.collection("publications").is_none() {
        for (identity, name, online) in
            [(3001, "Online Store", true), (3002, "Point of Sale", false)]
        {
            state.store.collection_mut("publications").insert(
                identity.to_string(),
                json!({
                    "id": identity, "name": name, "appTitle": name, "isOnlineStore": online,
                    "autoPublish": false
                }),
            );
        }
    }
    let online = online_store(&state.store);
    let products: Vec<_> = state
        .store
        .collection("products")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .map(|(key, product)| (key.to_owned(), product.clone()))
        .collect();
    for (key, mut product) in products {
        normalize_product(&mut product);
        let identity = id_of(&product);
        if let (Some(publication), Some(date)) = (online, product["publishedAt"].as_str()) {
            let link_key = format!("{identity}/{publication}");
            if !state
                .store
                .collection("resource_publications")
                .is_some_and(|rows| rows.contains(&link_key))
            {
                state.store.collection_mut("resource_publications").insert(
                    link_key,
                    json!({
                        "productId": identity, "publicationId": publication, "isPublished": true,
                        "publishDate": date
                    }),
                );
            }
        }
        state.store.collection_mut("products").insert(key, product);
        sync_visibility(&mut state.store, identity);
    }
}

fn online_store(store: &Store) -> Option<u64> {
    model::docs(store, "publications")
        .into_iter()
        .find(|publication| publication["isOnlineStore"] == true)
        .map(id_of)
}

pub fn sync_visibility(store: &mut Store, product: u64) {
    let date = online_store(store).and_then(|publication| {
        model::docs(store, "resource_publications")
            .into_iter()
            .find(|row| {
                row["productId"] == product
                    && row["publicationId"] == publication
                    && row["isPublished"] == true
            })
            .and_then(|row| row["publishDate"].as_str())
            .map(str::to_owned)
    });
    if let Some(key) = key_of(store, "products", product)
        && let Some(row) = store.collection_mut("products").get_mut(&key)
    {
        row["onlineStoreVisible"] = json!(row["status"] == "ACTIVE" && date.is_some());
        row["publishedAt"] = date.map_or(Value::Null, Value::String);
    }
}

pub fn publication(store: &Store, identity: u64) -> Value {
    find(store, "publications", identity).map_or(Value::Null, |row| {
        json!({
            "__typename": "Publication", "id": model::gid("Publication", identity),
            "name": row["name"], "autoPublish": row["autoPublish"],
            "app": {"__typename": "App", "title": row["appTitle"]}
        })
    })
}

pub fn resources(store: &Store, product: u64) -> Value {
    model::connection("ResourcePublication", model::docs(store, "resource_publications").into_iter()
        .filter(|row| row["productId"] == product)
        .map(|row| json!({
            "__typename": "ResourcePublication",
            "isPublished": row["isPublished"], "publishDate": row["publishDate"],
            "publication": reference("Publication", row["publicationId"].as_u64().unwrap_or(0))
        })).collect())
}

pub fn publish(store: &mut Store, args: &serde_json::Map<String, Value>, now: &str) -> Value {
    let payload = |product: Value, errors: Vec<Value>, store: &Store| {
        json!({
            "__typename": "PublishablePublishPayload", "publishable": product,
            "shop": model::shop(store), "userErrors": errors
        })
    };
    let product = args
        .get("id")
        .and_then(Value::as_str)
        .and_then(model::parse_gid)
        .filter(|(kind, _)| *kind == "Product")
        .map(|(_, identity)| identity);
    let Some(product) = product.filter(|identity| find(store, "products", *identity).is_some())
    else {
        return payload(
            Value::Null,
            vec![json!({
                "__typename": "UserError", "field": ["id"], "message": "Product does not exist"
            })],
            store,
        );
    };
    let inputs: Vec<_> = match args.get("input") {
        Some(Value::Array(inputs)) => inputs.iter().collect(),
        Some(input @ Value::Object(_)) => vec![input],
        _ => Vec::new(),
    };
    let mut errors = Vec::new();
    let mut publications = Vec::new();
    if inputs.is_empty() {
        errors.push(json!({"__typename": "UserError", "field": ["input"], "message": "A publication is required"}));
    }
    for (index, input) in inputs.iter().enumerate() {
        let publication = input["publicationId"]
            .as_str()
            .and_then(model::parse_gid)
            .filter(|(kind, _)| *kind == "Publication")
            .map(|(_, identity)| identity)
            .filter(|identity| find(store, "publications", *identity).is_some());
        if let Some(publication) = publication {
            publications.push(publication);
        } else {
            errors.push(json!({"__typename": "UserError", "field": ["input", index.to_string(), "publicationId"], "message": "Publication does not exist"}));
        }
    }
    if !errors.is_empty() {
        return payload(Value::Null, errors, store);
    }
    for publication in publications {
        let key = format!("{product}/{publication}");
        if !store
            .collection("resource_publications")
            .is_some_and(|rows| rows.contains(&key))
        {
            store.collection_mut("resource_publications").insert(
                key,
                json!({
                    "productId": product, "publicationId": publication,
                    "isPublished": true, "publishDate": now
                }),
            );
        }
    }
    sync_visibility(store, product);
    payload(reference("Product", product), Vec::new(), store)
}
