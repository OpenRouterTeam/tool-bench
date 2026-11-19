//! The fairness admission check (`docs/fairness.md`, rules F3 to F5): what an instruction
//! says about the route to each service its task needs, and whether a task that names no
//! service can be done on every connected service that fits its wording.
//!
//! The words and the classes are `catalog/equivalence.toml`. Every match here is a whole
//! word of the instruction, so a hint given in other words is not seen.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Scenario;

/// What an instruction says about the route to a service, weakest first, so the least
/// of a task's services is the task's label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Route {
    /// It names neither the product nor a kind of thing only that product has.
    #[serde(rename = "route:unnamed")]
    Unnamed,
    /// It names a kind of thing only one connected service has.
    #[serde(rename = "route:kind")]
    Kind,
    /// It names the product.
    #[serde(rename = "route:named")]
    Named,
}

impl Route {
    /// The scenario tag and the report label.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Unnamed => "route:unnamed",
            Self::Kind => "route:kind",
            Self::Named => "route:named",
        }
    }
}

/// What an instruction calls one catalog service.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceWords {
    pub name: String,
    /// Product names, matched with their capitals.
    pub names: Vec<String>,
    /// Words for a kind of thing no other connected service has, matched in any case.
    #[serde(default)]
    pub only: Vec<String>,
}

/// One request to a member's mock that does the class's job (`{query}`, `{query_url}`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassRead {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub body: Option<String>,
}

/// Connected services that are interchangeable for one kind of job.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Class {
    pub id: String,
    pub job: String,
    pub kind_words: Vec<String>,
    pub members: Vec<String>,
    #[serde(default)]
    pub read: BTreeMap<String, ClassRead>,
}

/// Scored tasks of a frozen release that fail the check and cannot be edited there.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Known {
    pub release: String,
    pub tasks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equivalence {
    #[serde(rename = "service")]
    pub services: Vec<ServiceWords>,
    #[serde(rename = "class")]
    pub classes: Vec<Class>,
    #[serde(default)]
    pub known: Known,
}

/// Whether a scenario can be done on every member of one class.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassVerdict {
    pub class: String,
    /// The class's kind words found in the instruction. Empty when the class is listed
    /// only because the service belongs to it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub kind_words: Vec<String>,
    pub members: Vec<String>,
    pub accepted: bool,
    /// What keeps a member from being a route to a pass.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceRoute {
    pub service: String,
    pub route: Route,
    /// The words of the instruction that decided the route.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<String>,
    /// For an unnamed service, each class another member of which fits the wording.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<ClassVerdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskFairness {
    pub id: String,
    pub scored: bool,
    pub route: Route,
    /// A needed service is unnamed, shares a class, and some member of it cannot pass.
    pub flagged: bool,
    /// Flagged, and listed under `[known]` as frozen in a release.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub known: bool,
    pub services: Vec<ServiceRoute>,
}

impl TaskFairness {
    /// The classes this task accepts in full: each member is a route to a pass.
    pub fn accepted_classes(&self) -> BTreeSet<&str> {
        self.classes(true)
    }

    /// The classes that flag this task.
    pub fn refused_classes(&self) -> BTreeSet<&str> {
        self.classes(false)
    }

    fn classes(&self, accepted: bool) -> BTreeSet<&str> {
        self.services
            .iter()
            .flat_map(|service| &service.classes)
            .filter(|class| class.accepted == accepted)
            .map(|class| class.class.as_str())
            .collect()
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub scored_tasks: usize,
    pub scored_routes: BTreeMap<&'static str, usize>,
    pub scored_flagged: usize,
    pub scored_flagged_by_class: BTreeMap<String, usize>,
    /// Scored tasks that name no service and pass because every class member is accepted.
    pub scored_accepting_a_class: usize,
    pub unscored_tasks: usize,
    pub unscored_flagged: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub scope: &'static str,
    pub limits: Vec<&'static str>,
    pub known_release: String,
    pub summary: Summary,
    /// Scored tasks the check flags, each listed under `[known]`.
    pub flagged: Vec<String>,
    pub tasks: Vec<TaskFairness>,
}

/// `docs/coverage/route-labels.json`: the route label of every scenario.
#[derive(Debug, Serialize, Deserialize)]
pub struct RouteLabels {
    pub version: u32,
    pub description: String,
    pub labels: BTreeMap<String, Route>,
}

const LIMITS: [&str; 4] = [
    "Product names and kind words are matched as whole words of the instruction; a hint in other words is not seen.",
    "The classes are a reviewed judgment (catalog/equivalence.toml); a class that is missing hides the fault this check exists to catch.",
    "accepted means the scenario serves records on every member and no contract entry takes one member alone. That each member returns the same facts, that the reference reply then passes and that doing nothing fails is run by crates/world/tests/fairness_routes.rs.",
    "A flagged task listed under known is frozen in a release and is reported, not refused; any other flagged scored task fails taskgen.",
];

impl Equivalence {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        toml::from_str(
            &fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?,
        )
        .with_context(|| format!("parsing {}", path.display()))
    }

    /// One row per catalog service, and classes of at least two known services.
    pub fn validate(&self, catalog: &catalog::Catalog) -> anyhow::Result<()> {
        let listed: BTreeSet<&str> = self.services.iter().map(|s| s.name.as_str()).collect();
        ensure!(
            listed.len() == self.services.len(),
            "equivalence: a service is listed twice"
        );
        for service in &catalog.services {
            ensure!(
                listed.contains(service.name.as_str()),
                "equivalence: no [[service]] row for {}",
                service.name
            );
        }
        for words in &self.services {
            ensure!(
                catalog.services.iter().any(|s| s.name == words.name),
                "equivalence: unknown service {}",
                words.name
            );
            ensure!(
                !words.names.is_empty(),
                "equivalence: {} has no product name",
                words.name
            );
        }
        let mut ids = BTreeSet::new();
        for class in &self.classes {
            ensure!(
                ids.insert(class.id.as_str()),
                "equivalence: class {} is listed twice",
                class.id
            );
            let members: BTreeSet<&str> = class.members.iter().map(String::as_str).collect();
            ensure!(
                members.len() == class.members.len() && members.len() >= 2,
                "equivalence: class {} needs at least two distinct members",
                class.id
            );
            for name in &class.members {
                ensure!(
                    listed.contains(name.as_str()),
                    "equivalence: class {} names unknown service {name}",
                    class.id
                );
            }
            for name in class.read.keys() {
                ensure!(
                    members.contains(name.as_str()),
                    "equivalence: class {} has a read for {name}, which is not a member",
                    class.id
                );
            }
        }
        Ok(())
    }

    /// The route to each service of `scenario`, and whether the task is flagged. Fails
    /// when a `route:` tag disagrees with the instruction.
    pub fn assess(&self, scenario: &Scenario) -> anyhow::Result<TaskFairness> {
        let id = &scenario.meta.id;
        let text = squeeze(&scenario.instruction);
        let mut services = Vec::new();
        for name in &scenario.meta.services {
            let words = self
                .services
                .iter()
                .find(|words| &words.name == name)
                .with_context(|| format!("{id}: no equivalence row for service {name}"))?;
            let named = found(&text, &words.names, true);
            let only = found(&text, &words.only, false);
            let (route, words) = if !named.is_empty() {
                (Route::Named, named)
            } else if !only.is_empty() {
                (Route::Kind, only)
            } else {
                (Route::Unnamed, Vec::new())
            };
            let classes = if route == Route::Unnamed {
                self.candidates(name, &text)
                    .into_iter()
                    .map(|(class, kind_words)| verdict(class, kind_words, scenario))
                    .collect()
            } else {
                Vec::new()
            };
            services.push(ServiceRoute {
                service: name.clone(),
                route,
                words,
                classes,
            });
        }
        let route = services
            .iter()
            .map(|service| service.route)
            .min()
            .with_context(|| format!("{id}: a scenario needs a service"))?;
        let mut declared = scenario
            .meta
            .tags
            .iter()
            .filter(|tag| tag.starts_with("route:"));
        if let Some(tag) = declared.next() {
            ensure!(declared.next().is_none(), "{id}: more than one route tag");
            ensure!(
                tag == route.tag(),
                "{id}: tagged {tag}, but its instruction reads as {}",
                route.tag()
            );
        }
        let flagged = services
            .iter()
            .flat_map(|service| &service.classes)
            .any(|class| !class.accepted);
        Ok(TaskFairness {
            id: id.clone(),
            scored: scenario.scored(),
            route,
            flagged,
            known: flagged && self.known.tasks.contains(id),
            services,
        })
    }

    /// The classes of `service` whose kind words the instruction uses; all of its classes
    /// when it uses none, since the wording then gives no reason to prefer the service.
    fn candidates(&self, service: &str, text: &str) -> Vec<(&Class, Vec<String>)> {
        let own: Vec<(&Class, Vec<String>)> = self
            .classes
            .iter()
            .filter(|class| class.members.iter().any(|member| member == service))
            .map(|class| (class, found(text, &class.kind_words, false)))
            .collect();
        if own.iter().all(|(_, words)| words.is_empty()) {
            return own;
        }
        own.into_iter()
            .filter(|(_, words)| !words.is_empty())
            .collect()
    }

    /// The check over every scenario, sorted by id.
    pub fn report(&self, scenarios: &[Scenario]) -> anyhow::Result<Report> {
        let mut tasks = scenarios
            .iter()
            .map(|scenario| self.assess(scenario))
            .collect::<anyhow::Result<Vec<_>>>()?;
        tasks.sort_by(|left, right| left.id.cmp(&right.id));
        let mut summary = Summary::default();
        for task in &tasks {
            if !task.scored {
                summary.unscored_tasks += 1;
                summary.unscored_flagged += usize::from(task.flagged);
                continue;
            }
            summary.scored_tasks += 1;
            *summary.scored_routes.entry(task.route.tag()).or_default() += 1;
            summary.scored_flagged += usize::from(task.flagged);
            summary.scored_accepting_a_class +=
                usize::from(!task.flagged && !task.accepted_classes().is_empty());
            for class in task.refused_classes() {
                *summary
                    .scored_flagged_by_class
                    .entry(class.to_owned())
                    .or_default() += 1;
            }
        }
        Ok(Report {
            schema_version: 1,
            scope: "Fairness admission: the route each instruction gives to each service it needs, and the scored tasks an agent could as reasonably start on another connected service.",
            limits: LIMITS.to_vec(),
            known_release: self.known.release.clone(),
            summary,
            flagged: tasks
                .iter()
                .filter(|task| task.scored && task.flagged)
                .map(|task| task.id.clone())
                .collect(),
            tasks,
        })
    }
}

impl Report {
    /// Fails for a flagged scored task that is not frozen in a release: it is refused.
    pub fn admit(&self) -> anyhow::Result<()> {
        let refused: Vec<String> = self
            .tasks
            .iter()
            .filter(|task| task.scored && task.flagged && !task.known)
            .map(refusal)
            .collect();
        if !refused.is_empty() {
            bail!(
                "{} scored task(s) fail the fairness admission check (docs/fairness.md, F4 and F5):\n{}",
                refused.len(),
                refused.join("\n")
            );
        }
        Ok(())
    }

    pub fn route_labels(&self) -> RouteLabels {
        RouteLabels {
            version: 1,
            description: "What each task's instruction says about the route to the services it needs: route:named (it names each product), route:kind (it names a kind of thing only one connected service has), route:unnamed (it names neither). Generated by taskgen from catalog/equivalence.toml; see docs/fairness.md.".into(),
            labels: self
                .tasks
                .iter()
                .map(|task| (task.id.clone(), task.route))
                .collect(),
        }
    }
}

fn refusal(task: &TaskFairness) -> String {
    let reasons: Vec<String> = task
        .services
        .iter()
        .flat_map(|service| {
            service
                .classes
                .iter()
                .filter(|class| !class.accepted)
                .map(|class| {
                    format!(
                        "the instruction does not name {}, and {} ({}) fit its wording: {}",
                        service.service,
                        class.class,
                        class.members.join(", "),
                        class.gaps.join("; ")
                    )
                })
        })
        .collect();
    format!(
        "{}: {}. Name the product, or serve the records on each member and accept each.",
        task.id,
        reasons.join(" | ")
    )
}

/// Whether `scenario` can be done on every member of `class`.
fn verdict(class: &Class, kind_words: Vec<String>, scenario: &Scenario) -> ClassVerdict {
    let mut gaps = Vec::new();
    let unserved: Vec<&str> = class
        .members
        .iter()
        .filter(|member| {
            !scenario.meta.services.contains(member) || !holds_records(&scenario.dir, member)
        })
        .map(String::as_str)
        .collect();
    if !unserved.is_empty() {
        gaps.push(format!(
            "the scenario serves no records on {}",
            unserved.join(", ")
        ));
    }
    let member = |service: &String| class.members.contains(service);
    for (index, call) in scenario.expect.required_calls.iter().enumerate() {
        if member(&call.service) {
            gaps.push(format!(
                "required_calls[{index}] takes {} alone",
                call.service
            ));
        }
    }
    for (index, check) in scenario.expect.information.iter().enumerate() {
        let read: BTreeSet<&String> = std::iter::once(&check.prerequisite.service)
            .chain(
                check
                    .alternatives
                    .iter()
                    .map(|read| &read.prerequisite.service),
            )
            .collect();
        if read.iter().any(|service| member(service))
            && !class.members.iter().all(|member| read.contains(member))
        {
            gaps.push(format!(
                "information[{index}] accepts a read of {} only",
                read.iter()
                    .map(|service| service.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let changes = json!([
        scenario.expect.state,
        scenario.expect.records,
        scenario.expect.relations
    ]);
    for member in &class.members {
        if mentions(&changes, &format!("/services/{member}/")) {
            gaps.push(format!(
                "a state, record or relation check reads {member}'s records, so the same change on another member fails"
            ));
        }
    }
    ClassVerdict {
        class: class.id.clone(),
        kind_words,
        members: class.members.clone(),
        accepted: gaps.is_empty(),
        gaps,
    }
}

fn holds_records(scenario: &Path, service: &str) -> bool {
    fs::read_dir(scenario.join("dataset").join(service))
        .is_ok_and(|mut entries| entries.next().is_some())
}

/// Whether any string or key in `value` starts with `prefix`.
fn mentions(value: &Value, prefix: &str) -> bool {
    match value {
        Value::String(text) => text.starts_with(prefix),
        Value::Array(items) => items.iter().any(|item| mentions(item, prefix)),
        Value::Object(fields) => fields
            .iter()
            .any(|(key, item)| key.starts_with(prefix) || mentions(item, prefix)),
        _ => false,
    }
}

/// The instruction on one line, so a phrase that wraps still matches.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `words` that occur in `text` as whole words.
fn found(text: &str, words: &[String], exact_case: bool) -> Vec<String> {
    let lowered;
    let haystack = if exact_case {
        text
    } else {
        lowered = text.to_lowercase();
        &lowered
    };
    words
        .iter()
        .filter(|word| {
            let needle = if exact_case {
                (*word).clone()
            } else {
                word.to_lowercase()
            };
            has_word(haystack, &needle)
        })
        .cloned()
        .collect()
}

fn has_word(text: &str, word: &str) -> bool {
    let inside = |character: Option<char>| character.is_some_and(char::is_alphanumeric);
    !word.is_empty()
        && text.match_indices(word).any(|(start, _)| {
            !inside(text[..start].chars().next_back())
                && !inside(text[start + word.len()..].chars().next())
        })
}

#[cfg(test)]
mod tests {
    use super::{found, has_word, squeeze};

    #[test]
    fn a_word_matches_whole_and_not_inside_another() {
        assert!(has_word("check the payments in Stripe, and", "Stripe"));
        assert!(has_word("(HubSpot)", "HubSpot"));
        assert!(!has_word("a linear search", "Linear"));
        assert!(!has_word("the Stripes", "Stripe"));
        assert!(!has_word("restore the store2 copy", "store"));
        assert!(has_word("in the `payments` namespace", "namespace"));
    }

    #[test]
    fn product_names_keep_their_capitals_and_kind_words_do_not() {
        let names = ["Linear".to_owned(), "Notion".to_owned()];
        assert!(found("a linear notion", &names, true).is_empty());
        assert_eq!(found("file it in Linear", &names, true), ["Linear"]);
        let kinds = ["crm".to_owned(), "support ticket".to_owned()];
        assert_eq!(found("In our CRM, which", &kinds, false), ["crm"]);
        assert!(found("support tickets", &kinds, false).is_empty());
    }

    #[test]
    fn a_phrase_that_wraps_a_line_still_matches() {
        let text = squeeze("current local business\nlisting data");
        assert!(has_word(&text, "local business listing"));
    }
}
