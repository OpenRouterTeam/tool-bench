use serde_json::Value;

pub(super) fn dependent_of(object: &Value, owner: &Value) -> bool {
    matches(object, owner, false)
}

pub(super) fn controlled_by(object: &Value, owner: &Value) -> bool {
    matches(object, owner, true)
}

fn group(version: &str) -> Option<&str> {
    if version.is_empty() {
        return None;
    }
    match version.split_once('/') {
        Some((group, version)) if !group.is_empty() && !version.is_empty() => Some(group),
        Some(_) => None,
        None => Some(""),
    }
}

fn matches(object: &Value, owner: &Value, controller: bool) -> bool {
    let Some(uid) = owner["metadata"]["uid"]
        .as_str()
        .filter(|uid| !uid.is_empty())
    else {
        return false;
    };
    let (Some(kind), Some(name), Some(version)) = (
        owner["kind"].as_str(),
        owner["metadata"]["name"].as_str(),
        owner["apiVersion"].as_str(),
    ) else {
        return false;
    };
    let Some(owner_group) = group(version) else {
        return false;
    };
    if owner["metadata"]["namespace"]
        .as_str()
        .is_some_and(|namespace| object["metadata"]["namespace"].as_str() != Some(namespace))
    {
        return false;
    }
    object["metadata"]["ownerReferences"]
        .as_array()
        .is_some_and(|references| {
            references.iter().any(|reference| {
                reference["uid"].as_str() == Some(uid)
                    && reference["kind"].as_str() == Some(kind)
                    && reference["name"].as_str() == Some(name)
                    && reference["apiVersion"].as_str().and_then(group) == Some(owner_group)
                    && (!controller || reference["controller"] == true)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn owner() -> Value {
        json!({"apiVersion":"apps/v1","kind":"Deployment",
            "metadata":{"name":"checkout","namespace":"production","uid":"current-owner"}})
    }

    fn dependent() -> Value {
        json!({"metadata":{"namespace":"production","ownerReferences":[{
            "apiVersion":"apps/v1","kind":"Deployment","name":"checkout",
            "uid":"current-owner","controller":true
        }]}})
    }

    #[test]
    fn ownership_uses_identity_not_name_or_selector_coincidence() {
        assert!(controlled_by(&dependent(), &owner()));
        for (field, value) in [
            ("uid", json!("previous-owner")),
            ("kind", json!("ReplicaSet")),
            ("name", json!("another-workload")),
            ("apiVersion", json!("another.group/v1")),
            ("apiVersion", Value::Null),
        ] {
            let mut object = dependent();
            object["metadata"]["ownerReferences"][0][field] = value;
            assert!(!dependent_of(&object, &owner()), "{object}");
        }
        let mut object = dependent();
        object["metadata"]["namespace"] = json!("staging");
        assert!(!dependent_of(&object, &owner()));
    }

    #[test]
    fn a_dependency_is_not_necessarily_a_controlling_reference() {
        for controller in [json!(false), Value::Null] {
            let mut object = dependent();
            object["metadata"]["ownerReferences"][0]["controller"] = controller;
            assert!(dependent_of(&object, &owner()));
            assert!(!controlled_by(&object, &owner()));
        }
    }

    #[test]
    fn identity_uses_api_group_and_uid_not_an_api_version_spelling() {
        let mut object = dependent();
        object["metadata"]["ownerReferences"][0]["apiVersion"] = json!("apps/v1beta2");
        assert!(controlled_by(&object, &owner()));
    }

    #[test]
    fn absent_identities_and_references_do_not_match_each_other() {
        let mut owner = owner();
        owner["metadata"]["uid"] = Value::Null;
        let mut object = dependent();
        object["metadata"]["ownerReferences"][0]["uid"] = Value::Null;
        assert!(!dependent_of(&object, &owner));
        assert!(!dependent_of(&json!({"metadata":{}}), &owner));
    }
}
