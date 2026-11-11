use mock_core::quantity::Quantity;
use serde_json::Value;

pub(super) fn validate(spec: &Value, prefix: &str) -> Option<(String, String)> {
    for list in ["containers", "initContainers"] {
        let mut names = std::collections::BTreeSet::new();
        for (index, container) in spec[list].as_array().into_iter().flatten().enumerate() {
            if let Some(name) = container["name"].as_str()
                && !names.insert(name)
            {
                return Some((
                    format!("{prefix}.{list}[{index}].name"),
                    "duplicate container name".into(),
                ));
            }
            for resource in ["cpu", "memory"] {
                let mut parsed = Vec::new();
                for kind in ["requests", "limits"] {
                    let Some(value) = container["resources"][kind].get(resource) else {
                        parsed.push(None);
                        continue;
                    };
                    let field = format!("{prefix}.{list}[{index}].resources.{kind}.{resource}");
                    let quantity = match Quantity::parse(value) {
                        Ok(quantity) if !quantity.is_negative() => quantity,
                        _ => {
                            return Some((
                                field,
                                "must be a supported nonnegative resource quantity".into(),
                            ));
                        }
                    };
                    parsed.push(Some(quantity));
                }
                if let [Some(request), Some(limit)] = parsed.as_slice()
                    && request > limit
                {
                    return Some((
                        format!("{prefix}.{list}[{index}].resources.requests.{resource}"),
                        "must be less than or equal to the limit".into(),
                    ));
                }
            }
        }
    }
    None
}

/// Whether two pod templates are one template to the API server. It stores every resource
/// quantity in canonical form, so `1024Mi` and `1Gi` do not differ and start no rollout.
pub(super) fn same_template(left: &Value, right: &Value) -> bool {
    let mut left = left.clone();
    for list in ["containers", "initContainers"] {
        let Some(containers) = left
            .pointer_mut(&format!("/spec/{list}"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for (index, container) in containers.iter_mut().enumerate() {
            for kind in ["requests", "limits"] {
                let other = &right["spec"][list][index]["resources"][kind];
                let Some(quantities) = container
                    .pointer_mut(&format!("/resources/{kind}"))
                    .and_then(Value::as_object_mut)
                else {
                    continue;
                };
                for (resource, value) in quantities {
                    if let Some(spelling) = other.get(resource)
                        && let (Ok(ours), Ok(theirs)) =
                            (Quantity::parse(value), Quantity::parse(spelling))
                        && ours.cmp(&theirs).is_eq()
                    {
                        *value = spelling.clone();
                    }
                }
            }
        }
    }
    left == *right
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn template(memory: &str, image: &str) -> Value {
        json!({"metadata":{"labels":{"app":"report-builder"}},"spec":{"containers":[{
            "name":"report-builder","image":image,
            "resources":{"requests":{"cpu":"500m"},"limits":{"memory":memory}}
        }]}})
    }

    #[test]
    fn a_respelled_quantity_is_the_same_template_and_a_changed_one_is_not() {
        let stored = template("1Gi", "report-builder:3.2.0");
        for spelling in ["1024Mi", "1073741824", " 1Gi "] {
            assert!(same_template(
                &template(spelling, "report-builder:3.2.0"),
                &stored
            ));
        }
        assert!(!same_template(
            &template("1G", "report-builder:3.2.0"),
            &stored
        ));
        assert!(!same_template(
            &template("512Mi", "report-builder:3.2.0"),
            &stored
        ));
        assert!(!same_template(
            &template("1Gi", "report-builder:3.3.0"),
            &stored
        ));
    }
}
