use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::{RecordCheck, RecordCheckError, ReviewedText};
use crate::monitor_schedule::{MonitorSchedule, ScheduleError};

fn failed(message: &str) -> RecordCheckError {
    RecordCheckError::Failed(format!("Firecrawl outcome: {message}"))
}

fn unresolved(message: &str) -> RecordCheckError {
    RecordCheckError::NeedsAdjudication(format!("Firecrawl outcome: {message}"))
}

fn resource(source: &str) -> Option<Url> {
    let mut parsed = Url::parse(source).ok()?;
    if !["https", "http"].contains(&parsed.scheme())
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    parsed.set_fragment(None);
    Some(parsed)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirecrawlCrawlGoal {
    pub origin: String,
    pub path_prefix: String,
}

impl FirecrawlCrawlGoal {
    pub(super) fn validate(&self) -> Result<(), String> {
        let origin = resource(&self.origin).ok_or("Crawl goals require an HTTP origin")?;
        if origin.path() != "/"
            || origin.query().is_some()
            || !self.path_prefix.starts_with('/')
            || !self.path_prefix.ends_with('/')
            || self.path_prefix.contains(['?', '#'])
        {
            return Err("Crawl goals require an origin and a literal section prefix".into());
        }
        Ok(())
    }

    pub(super) fn owns_field(pointer: &str) -> bool {
        [
            "id",
            "url",
            "status",
            "pages",
            "plannedPages",
            "total",
            "completed",
        ]
        .contains(&pointer.split('/').nth(1).unwrap_or_default())
    }

    pub(super) fn check(
        &self,
        initial: Option<&Value>,
        job: &Value,
    ) -> Result<(), RecordCheckError> {
        let initial = initial.ok_or_else(|| {
            RecordCheckError::MissingEvidence(
                "Crawl scope requires immutable initial page data".into(),
            )
        })?;
        let pages = initial
            .pointer("/services/firecrawl/pages")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                RecordCheckError::MissingEvidence("Authored crawl pages are missing".into())
            })?;
        let origin = resource(&self.origin)
            .ok_or_else(|| RecordCheckError::InvalidContract("Invalid crawl origin".into()))?;
        let desired: BTreeSet<_> = pages
            .keys()
            .filter_map(|key| resource(key))
            .filter(|url| {
                url.origin() == origin.origin()
                    && (url.path().starts_with(&self.path_prefix)
                        || url.path() == self.path_prefix.trim_end_matches('/'))
            })
            .map(|url| url.to_string())
            .collect();
        if desired.is_empty() {
            return Err(RecordCheckError::InvalidContract(
                "Authored crawl section is empty".into(),
            ));
        }
        let seed = job
            .get("url")
            .and_then(Value::as_str)
            .and_then(resource)
            .ok_or_else(|| failed("crawl seed is not an HTTP resource"))?;
        if seed.host_str() != origin.host_str() {
            return Err(failed("crawl seed belongs to another site"));
        }
        let status = job
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !["queued", "scraping", "completed"].contains(&status) {
            return Err(failed(
                "crawl admission is not active or successfully completed",
            ));
        }
        let planned = job
            .get("plannedPages")
            .or_else(|| (status == "completed").then(|| job.get("pages")).flatten())
            .and_then(Value::as_array)
            .ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "Accepted crawl has no auditable discovered scope".into(),
                )
            })?;
        let canonical = |values: &[Value]| -> Result<BTreeSet<String>, RecordCheckError> {
            let mut result = BTreeSet::new();
            for value in values {
                let url = value
                    .as_str()
                    .and_then(resource)
                    .ok_or_else(|| failed("crawl scope contains a non-resource value"))?
                    .to_string();
                if !result.insert(url) {
                    return Err(failed("crawl scope contains duplicate resources"));
                }
            }
            Ok(result)
        };
        if canonical(planned)? != desired {
            return Err(failed(
                "crawl does not collect exactly the authored API section",
            ));
        }
        let collected = job
            .get("pages")
            .and_then(Value::as_array)
            .ok_or_else(|| failed("crawl collected-page evidence is malformed"))?;
        let collected = canonical(collected)?;
        if !collected.is_subset(&desired) || status == "completed" && collected != desired {
            return Err(failed("collected pages violate the admitted API scope"));
        }
        if !job.get("total").is_some_and(Value::is_number)
            || !job.get("completed").is_some_and(Value::is_number)
            || crate::exact_integer(job.get("total").unwrap_or(&Value::Null))
                != i64::try_from(desired.len()).ok()
            || crate::exact_integer(job.get("completed").unwrap_or(&Value::Null))
                != i64::try_from(collected.len()).ok()
        {
            return Err(failed(
                "crawl counters disagree with actual planned and collected resources",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirecrawlMonitorGoal {
    pub name: String,
    pub url: String,
    pub recipient: String,
    pub judged_goal: ReviewedText,
}

impl FirecrawlMonitorGoal {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || resource(&self.url).is_none()
            || !self.recipient.is_ascii()
            || self.recipient.trim() != self.recipient
            || self.recipient.contains(char::is_whitespace)
            || self.recipient.split('@').count() != 2
            || self.recipient.split('@').any(str::is_empty)
            || self.judged_goal.optional
        {
            return Err(
                "Monitor goals require a resource, recipient and required judged criterion".into(),
            );
        }
        self.judged_goal.validate()
    }

    pub(super) fn owns_field(pointer: &str) -> bool {
        [
            "id",
            "name",
            "status",
            "targets",
            "schedule",
            "notification",
            "webhook",
            "goal",
            "judgeEnabled",
        ]
        .contains(&pointer.split('/').nth(1).unwrap_or_default())
    }

    pub(super) fn check(
        &self,
        initial: Option<&Value>,
        record: &Value,
        state: &Value,
        complete: bool,
    ) -> Result<(), RecordCheckError> {
        let status = record
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if record.get("name").and_then(Value::as_str) != Some(&self.name) {
            return Err(failed("monitor has the wrong requested name"));
        }
        if !["active", "paused"].contains(&status) || complete && status != "active" {
            return Err(failed("monitor is not in the requested active state"));
        }
        let targets = record
            .get("targets")
            .and_then(Value::as_array)
            .ok_or_else(|| failed("monitor targets are missing"))?;
        if targets.len() != 1 {
            return Err(failed("monitor has additional or missing targets"));
        }
        let target = &targets[0];
        if target.get("type").and_then(Value::as_str) != Some("scrape") {
            return Err(unresolved(
                "non-scrape target coverage needs independent qualification",
            ));
        }
        let urls = target
            .get("urls")
            .and_then(Value::as_array)
            .ok_or_else(|| failed("monitor target URLs are missing"))?;
        let expected = resource(&self.url)
            .ok_or_else(|| RecordCheckError::InvalidContract("Invalid monitor target".into()))?;
        if urls.is_empty()
            || urls
                .iter()
                .any(|value| value.as_str().and_then(resource).as_ref() != Some(&expected))
        {
            return Err(failed("monitor targets the wrong page or additional pages"));
        }
        if record.get("webhook").is_some_and(|value| !value.is_null()) {
            return Err(failed("unrequested webhook delivery is configured"));
        }
        for options in [
            target.get("scrapeOptions"),
            record.get("scrapeOptions"),
            record.get("changeTracking"),
        ] {
            if options.is_some_and(|value| !value.is_null() && value != &serde_json::json!({})) {
                return Err(unresolved(
                    "custom extraction/change-tracking coverage needs review",
                ));
            }
        }
        let parsed = MonitorSchedule::parse(record.get("schedule").unwrap_or(&Value::Null))
            .map_err(|error| match error {
                ScheduleError::Invalid(_) => failed("monitor schedule is invalid"),
                ScheduleError::Unsupported(_) => {
                    unresolved("monitor recurrence is outside the qualified parser")
                }
            })?;
        if (complete || status == "active")
            && !parsed
                .is_daily()
                .map_err(|_| unresolved("daily recurrence could not be evaluated"))?
        {
            return Err(failed("monitor is not calendar-daily"));
        }
        let notification = record
            .get("notification")
            .and_then(Value::as_object)
            .ok_or_else(|| failed("monitor notifications are malformed"))?;
        if notification
            .get("slack")
            .and_then(|value| value.get("enabled"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(failed("unrequested Slack delivery is configured"));
        }
        if notification
            .keys()
            .any(|key| !["email", "slack"].contains(&key.as_str()))
        {
            return Err(unresolved("unreviewed notification mechanism"));
        }
        let email = notification.get("email");
        let enabled = match email.and_then(|value| value.get("enabled")) {
            None => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err(failed("email enabled flag is not Boolean")),
        };
        let recipients = match email.and_then(|value| value.get("recipients")) {
            None => &[][..],
            Some(Value::Array(values)) => values.as_slice(),
            _ => return Err(failed("email recipients are not a list")),
        };
        if recipients.iter().any(|value| {
            value
                .as_str()
                .is_none_or(|value| !value.eq_ignore_ascii_case(&self.recipient))
        }) {
            return Err(failed("email has an unrequested recipient"));
        }
        if complete && (!enabled || recipients.is_empty()) {
            return Err(failed("requested email notification is not configured"));
        }
        let judged = match record.get("judgeEnabled") {
            Some(Value::Bool(value)) => *value,
            None => record
                .get("goal")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty()),
            _ => return Err(failed("judging flag is not Boolean")),
        };
        if judged && (complete || enabled && status == "active") {
            self.judged_goal
                .check(record.get("goal"), initial, state)
                .map_err(|error| match error {
                    RecordCheckError::Failed(reason) => failed(&format!("monitor goal: {reason}")),
                    RecordCheckError::NeedsAdjudication(reason) => {
                        unresolved(&format!("monitor goal: {reason}"))
                    }
                    other => other,
                })?;
        }
        match record.get("lastCheckId") {
            None | Some(Value::Null) => {
                if record
                    .get("lastRunAt")
                    .is_some_and(|value| !value.is_null())
                {
                    return Err(failed("monitor run metadata has no child check"));
                }
            }
            Some(Value::String(identity)) => {
                let child = state
                    .pointer("/services/firecrawl/monitor_checks")
                    .and_then(|checks| checks.get(identity))
                    .ok_or_else(|| failed("monitor run metadata points to a missing child"))?;
                if child.get("id").and_then(Value::as_str) != Some(identity)
                    || child.get("monitorId") != record.get("id")
                    || child.get("trigger").and_then(Value::as_str) != Some("manual")
                    || initial
                        .and_then(|initial| initial.pointer("/services/firecrawl/monitor_checks"))
                        .is_some_and(|checks| checks.get(identity).is_some())
                {
                    return Err(failed(
                        "monitor run metadata is not bound to its actual new check",
                    ));
                }
            }
            Some(_) => return Err(failed("monitor child-check identity is not text")),
        }
        Ok(())
    }
}

impl RecordCheck {
    pub(super) fn validate_firecrawl(&self) -> Result<(), String> {
        let (collection, owns): (&str, fn(&str) -> bool) =
            match (&self.firecrawl_crawl, &self.firecrawl_monitor) {
                (None, None) => return Ok(()),
                (Some(goal), None) => {
                    goal.validate()?;
                    ("/services/firecrawl/crawls", FirecrawlCrawlGoal::owns_field)
                }
                (None, Some(goal)) => {
                    goal.validate()?;
                    (
                        "/services/firecrawl/monitors",
                        FirecrawlMonitorGoal::owns_field,
                    )
                }
                _ => return Err("Firecrawl outcome goals must describe one resource kind".into()),
            };
        if self.selector.collection != collection
            || !self.selector.created
            || !self.selector.matching.is_empty()
            || !self.selector.field_patterns.is_empty()
            || !self.selector.references.is_empty()
            || self.selector.key_prefix.is_some()
            || self.selector.identity_field.is_some()
            || self.selector.latest_by.is_some()
            || self.equals != Some(1)
            || self.min_count.is_some()
            || self.max_count.is_some()
            || self.calendar_meeting.is_some()
            || self.metric_alarm.is_some()
            || self.tag_scope.is_some()
            || self.interval_coverage.is_some()
            || self.label_matchers.is_some()
        {
            return Err(
                "Firecrawl goals require exactly one complete created-resource collection".into(),
            );
        }
        let fields = self
            .fields
            .keys()
            .chain(self.optional_fields.keys())
            .chain(self.optional_choices.keys())
            .chain(self.null_fields.iter())
            .chain(self.numeric_fields.keys())
            .chain(self.timestamp_fields.keys())
            .chain(self.sheet_date_fields.keys())
            .chain(self.array_lengths.keys())
            .chain(self.quantity_fields.keys())
            .chain(self.json_fields.keys())
            .chain(self.value_sets.keys())
            .chain(self.integer_bounds.keys())
            .chain(self.totals.keys())
            .chain(self.html_text.keys())
            .chain(self.reviewed_text.keys())
            .chain(self.url_host_fields.iter())
            .chain(self.any_fields.iter().flat_map(|fields| fields.keys()))
            .chain(
                self.reviewed_fields
                    .iter()
                    .flat_map(|review| &review.accepted)
                    .flat_map(|fields| fields.keys()),
            );
        if fields.into_iter().any(|field| owns(field)) {
            return Err(
                "Firecrawl outcome semantics cannot compete with raw owned-field predicates".into(),
            );
        }
        Ok(())
    }
}
