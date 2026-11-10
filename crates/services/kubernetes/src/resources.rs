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
