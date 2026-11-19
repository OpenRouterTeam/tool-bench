//! A forbidden call says what it forbids: a path, a query, a body or a header beside the
//! method. A ban on a whole method also bans the reads a server makes with it; GitHub's
//! GraphQL reads, for one, are POSTs.

use std::path::Path;

use scenario::discover;

#[test]
fn no_forbidden_call_bans_a_whole_method() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let unscoped: Vec<String> = discover(&root)
        .unwrap()
        .iter()
        .flat_map(|scenario| {
            scenario
                .expect
                .forbidden_calls
                .iter()
                .enumerate()
                .filter(|(_, call)| {
                    call.path.is_none()
                        && call.path_regex.is_none()
                        && call.query_contains.is_none()
                        && call.query_fields.is_empty()
                        && call.body.is_none()
                        && call.body_contains.is_none()
                        && call.headers.is_empty()
                })
                .map(|(index, call)| {
                    format!(
                        "{} forbidden_calls/{index}: {} {}",
                        scenario.meta.id,
                        call.service,
                        call.method.as_deref().unwrap_or("any method")
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(unscoped.is_empty(), "{unscoped:#?}");
}
