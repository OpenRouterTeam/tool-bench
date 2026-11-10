use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::{AnswerCheck, answer_check};
use crate::{
    json_pointer,
    relations::{
        IdentitySource, RecordCheckError, RecordSelector, ReviewedText, ReviewedTextFormat,
    },
};

mod oncall;
mod source_facts;
pub use oncall::OncallAnswer;
pub use source_facts::{FactColumn, FactKind, FactRows, SourceFacts};

static URLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)https?://[^\s<>"'`]+"#).expect("URL candidate pattern"));

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentryDsn {
    pub project: RecordSelector,
    pub keys: RecordSelector,
    pub ingestion_origin: IdentitySource,
    pub content: ReviewedText,
}

impl AnswerCheck {
    pub(super) fn check(
        &self,
        answer: Option<&str>,
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<(), RecordCheckError> {
        if !answer_check(answer, self) {
            return Err(RecordCheckError::Failed(
                "the final answer did not satisfy its declared checks".into(),
            ));
        }
        if let Some(check) = &self.sentry_dsn {
            check.check(answer.expect("an answer was checked"), initial, state)?;
        }
        if let Some(check) = &self.oncall {
            check.check(answer.expect("an answer was checked"), initial, state)?;
        }
        if let Some(check) = &self.source_facts {
            check.check(answer.expect("an answer was checked"), initial)?;
        }
        Ok(())
    }
}

fn identifier(value: &Value) -> Option<String> {
    match value {
        Value::String(text)
            if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            text.parse::<u64>().ok().filter(|identity| *identity > 0)
        }
        Value::Number(number) => number.as_u64().filter(|identity| *identity > 0),
        _ => None,
    }
    .map(|identity| identity.to_string())
}

fn ingestion_url(source: &str) -> Option<Url> {
    let origin = Url::parse(source).ok()?;
    (matches!(origin.scheme(), "http" | "https")
        && origin.has_host()
        && origin.username().is_empty()
        && origin.password().is_none()
        && origin.query().is_none()
        && origin.fragment().is_none())
    .then_some(origin)
}

impl SentryDsn {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.project.validate()?;
        self.keys.validate()?;
        self.ingestion_origin.selector.validate()?;
        let tokens = json_pointer::tokens(&self.ingestion_origin.field)?;
        if self.ingestion_origin.selector.created
            || self.ingestion_origin.separator.is_some()
            || tokens.is_empty()
            || tokens.iter().any(|token| token == "*")
        {
            return Err("DSN ingestion must select one literal baseline field".into());
        }
        self.content.validate()?;
        if self.content.optional
            || self.content.bound.is_some()
            || self.content.format != ReviewedTextFormat::Plain
            || self
                .content
                .accepted
                .iter()
                .chain(&self.content.rejected)
                .any(|template| {
                    template.matches("${dsn}").count() != 1
                        || template.replace("${dsn}", "").contains("${")
                })
        {
            return Err(
                "DSN content requires plain whole-text examples with one ${dsn} placeholder".into(),
            );
        }
        Ok(())
    }

    fn check(
        &self,
        answer: &str,
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        let initial = initial.ok_or_else(|| {
            RecordCheckError::MissingEvidence(
                "DSN binding requires authoritative initial business state".into(),
            )
        })?;
        let origins = self
            .ingestion_origin
            .selector
            .select(Some(initial), initial)?;
        if origins.len() != 1 {
            return Err(RecordCheckError::MissingEvidence(
                "DSN binding requires one authoritative baseline ingestion endpoint".into(),
            ));
        }
        let origin = origins[0]
            .1
            .pointer(&self.ingestion_origin.field)
            .and_then(Value::as_str)
            .and_then(ingestion_url)
            .ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "DSN ingestion endpoint is not a supported HTTP origin/path".into(),
                )
            })?;
        let projects = self.project.select(Some(initial), state)?;
        if projects.len() != 1 {
            return Err(RecordCheckError::Failed(
                "the DSN must identify exactly one selected project".into(),
            ));
        }
        let project = projects[0].1;
        let project_id = project.get("id").and_then(identifier).ok_or_else(|| {
            RecordCheckError::MissingEvidence("selected project lacks a numeric identity".into())
        })?;
        let slug = project["slug"]
            .as_str()
            .filter(|slug| !slug.is_empty())
            .ok_or_else(|| {
                RecordCheckError::MissingEvidence("selected project lacks its slug identity".into())
            })?;
        let keys = self.keys.select(Some(initial), state)?;
        let mut values = Vec::new();
        for candidate in URLS.find_iter(answer) {
            let source = candidate
                .as_str()
                .trim_end_matches([')', ']', '}', ',', ';', '.']);
            let Ok(candidate) = Url::parse(source) else {
                if source.contains('@') {
                    return Err(RecordCheckError::Failed(
                        "answer contains a malformed DSN URL".into(),
                    ));
                }
                continue;
            };
            if candidate.username().is_empty() {
                continue;
            }
            let matches = keys.iter().any(|(_, key)| {
                if key["isActive"] != true || key["project"].as_str() != Some(slug) {
                    return false;
                }
                let Some(public) = key["public"].as_str().filter(|public| !public.is_empty())
                else {
                    return false;
                };
                let mut expected = origin.clone();
                let Ok(mut segments) = expected.path_segments_mut() else {
                    return false;
                };
                segments.pop_if_empty().push(&project_id);
                drop(segments);
                if expected.set_username(public).is_err() {
                    return false;
                }
                if candidate.password().is_some() {
                    let Some(secret) = key["secret"].as_str().filter(|secret| !secret.is_empty())
                    else {
                        return false;
                    };
                    if expected.set_password(Some(secret)).is_err() {
                        return false;
                    }
                }
                candidate == expected
            });
            if !matches {
                return Err(RecordCheckError::Failed(
                    "answer DSN does not bind the selected project, active key and baseline ingestion endpoint".into(),
                ));
            }
            if !values.contains(&source) {
                values.push(source);
            }
        }
        if values.is_empty() {
            return Err(RecordCheckError::Failed(
                "answer contains no usable Sentry DSN".into(),
            ));
        }
        let mut content = self.content.clone();
        content.accepted = self
            .content
            .accepted
            .iter()
            .flat_map(|template| values.iter().map(|value| template.replace("${dsn}", value)))
            .collect();
        content.rejected = self
            .content
            .rejected
            .iter()
            .flat_map(|template| values.iter().map(|value| template.replace("${dsn}", value)))
            .collect();
        content.check(
            Some(&Value::String(answer.to_owned())),
            Some(initial),
            state,
        )
    }
}
