use std::sync::LazyLock;

use serde_json::Value;

use crate::{
    db,
    sql::{Db, SqlError},
};

static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    let contract: Value =
        serde_json::from_str(include_str!("../assets/native-introspection-v0_15_0.json"))
            .expect("native query contract parses");
    assert_eq!(contract["version"], "0.15.0");
    assert_eq!(contract["compiled_and_source_generators_match"], true);
    contract
});

pub fn execute(
    database: &Db,
    query: &str,
    parameters: &[Value],
) -> Result<Option<Vec<Value>>, SqlError> {
    if query
        == CONTRACT["extensions_query"]
            .as_str()
            .expect("extension query exists")
    {
        if !parameters.is_empty() {
            return Err(SqlError::new(
                "42P02",
                "unexpected query parameters".to_owned(),
            ));
        }
        return Ok(Some(db::introspect_extensions(database)));
    }
    let prefix = CONTRACT["tables_prefix"]
        .as_str()
        .expect("table prefix exists");
    let Some(suffix) = query.strip_prefix(prefix) else {
        return Ok(None);
    };
    let (bindings, exclude) = if let Some(bindings) = suffix.strip_prefix("where schema in (") {
        (bindings, false)
    } else if let Some(bindings) = suffix.strip_prefix("where schema not in (") {
        (bindings, true)
    } else {
        return Err(SqlError::unsupported());
    };
    let Some(bindings) = bindings.strip_suffix(')') else {
        return Err(SqlError::unsupported());
    };
    let count = bindings.split(", ").count();
    let placeholders = (1..=count)
        .map(|position| format!("${position}"))
        .collect::<Vec<_>>()
        .join(", ");
    if bindings != placeholders {
        return Err(SqlError::unsupported());
    }
    if parameters.len() != count {
        return Err(SqlError::new(
            "42P02",
            "query parameter count does not match".to_owned(),
        ));
    }
    if parameters.iter().any(|value| !value.is_string()) {
        return Err(SqlError::new(
            "22P02",
            "schema parameters must be strings".to_owned(),
        ));
    }
    Ok(Some(db::introspect_tables(database, parameters, exclude)))
}
