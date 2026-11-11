use std::collections::BTreeSet;

use regex::Regex;

use super::{CallMatch, Expect};
use crate::json_pointer::tokens as pointer_tokens;

impl Expect {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.version, 1 | 2) {
            return Err(format!("unsupported contract version {}", self.version));
        }
        for (index, check) in self.state.iter().enumerate() {
            pointer_tokens(&check.pointer)?;
            let has_value_check = check.equals.is_some()
                || check.contains.is_some()
                || check.any.is_some()
                || check.is_null.is_some()
                || check.text_equals.is_some()
                || check.member_ascii_ci.is_some();
            if !has_value_check && check.exists.is_none() {
                return Err(format!("state[{index}] has no predicate"));
            }
            if check.exists == Some(false) && has_value_check {
                return Err(format!(
                    "state[{index}] cannot require absence and a value simultaneously"
                ));
            }
            if check.key_prefix.is_some() && check.any.is_none() {
                return Err(format!(
                    "state[{index}].key_prefix requires an any predicate"
                ));
            }
            if check.is_null == Some(true)
                && (check.equals.as_ref().is_some_and(|value| !value.is_null())
                    || check.contains.is_some()
                    || check.any.is_some()
                    || check.text_equals.is_some()
                    || check.member_ascii_ci.is_some())
            {
                return Err(format!("state[{index}] has conflicting null predicates"));
            }
            if check
                .text_equals
                .as_ref()
                .is_some_and(|text| text.trim().is_empty())
                || check
                    .member_ascii_ci
                    .as_ref()
                    .is_some_and(|text| text.trim().is_empty())
            {
                return Err(format!(
                    "state[{index}] has an empty text or member predicate"
                ));
            }
            if check.text_equals.is_some() && check.member_ascii_ci.is_some() {
                return Err(format!(
                    "state[{index}] requires incompatible string and array types"
                ));
            }
        }
        let mut fault_codes = BTreeSet::new();
        for fault in &self.environment_faults {
            if fault.code.is_empty()
                || !fault
                    .code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                || fault.reason.trim().is_empty()
                || !fault_codes.insert(&fault.code)
            {
                return Err(
                    "declared environment faults need distinct codes and nonempty reasons".into(),
                );
            }
        }
        for (index, check) in self.records.iter().enumerate() {
            if (!check.sheet_date_fields.is_empty() || check.calendar_meeting.is_some())
                && (self.version < 2 || !self.audit_history)
            {
                return Err("spreadsheet date and calendar business goals require audited version-2 contracts".into());
            }
            check
                .validate()
                .map_err(|error| format!("records[{index}]: {error}"))?;
        }
        for (index, check) in self.relations.iter().enumerate() {
            check
                .validate()
                .map_err(|error| format!("relations[{index}]: {error}"))?;
        }
        for (index, check) in self.ordering.iter().enumerate() {
            check
                .validate()
                .map_err(|error| format!("ordering[{index}]: {error}"))?;
        }
        for (index, check) in self.information.iter().enumerate() {
            check
                .validate()
                .map_err(|error| format!("information[{index}]: {error}"))?;
        }
        if !self.information.is_empty() && (self.version < 2 || !self.audit_history) {
            return Err(
                "information prerequisites require version 2 and audited operation history".into(),
            );
        }
        if self.version < 2 && (self.audit_history || !self.ordering.is_empty()) {
            return Err("history and ordering checks require a version 2 contract".into());
        }
        for (index, matcher) in self.required_calls.iter().enumerate() {
            validate_call(matcher, &format!("required_calls[{index}]"))?;
        }
        for (index, matcher) in self.forbidden_calls.iter().enumerate() {
            let location = format!("forbidden_calls[{index}]");
            validate_call(matcher, &location)?;
            if matcher.min_count != 1 {
                return Err(format!("{location}.min_count must be one"));
            }
            if matcher.allow_tool_error {
                return Err(format!(
                    "{location}.allow_tool_error applies only to required calls"
                ));
            }
        }
        if let Some(answer) = &self.answer {
            if answer.source == super::AnswerSource::FinalResponse && self.version < 2 {
                return Err(
                    "native final-response collection requires a version-two contract".into(),
                );
            }
            if answer.contains.is_empty()
                && answer.regex.is_none()
                && answer.sentry_dsn.is_none()
                && answer.oncall.is_none()
                && answer.source_facts.is_none()
                && answer.record_facts.is_none()
            {
                return Err("answer has no predicate".into());
            }
            if answer.contains.iter().any(|text| text.trim().is_empty()) {
                return Err("answer.contains must not contain empty text".into());
            }
            if let Some(pattern) = &answer.regex {
                let expression =
                    Regex::new(pattern).map_err(|error| format!("answer.regex: {error}"))?;
                if expression.is_match("") {
                    return Err("answer.regex must not accept an empty answer".into());
                }
            }
            if let Some(check) = &answer.sentry_dsn {
                if self.version < 2 {
                    return Err("source-bound DSN checks require a version 2 contract".into());
                }
                check.validate()?;
            }
            if let Some(check) = &answer.oncall {
                if self.version < 2 || answer.sentry_dsn.is_some() {
                    return Err("on-call answer checks require a version-2 contract without a competing DSN predicate".into());
                }
                check.validate()?;
            }
            if let Some(check) = &answer.source_facts {
                if self.version < 2 || answer.sentry_dsn.is_some() || answer.oncall.is_some() {
                    return Err("source fact answer checks require a version-2 contract without a competing source-bound answer predicate".into());
                }
                check.validate()?;
            }
            if let Some(check) = &answer.record_facts {
                if self.version < 2
                    || answer.sentry_dsn.is_some()
                    || answer.oncall.is_some()
                    || answer.source_facts.is_some()
                {
                    return Err("record fact answer checks require a version-2 contract without a competing source-bound answer predicate".into());
                }
                check.validate()?;
            }
        }
        if let Some(policy) = &self.preservation {
            policy.validate()?;
            if policy.allow.iter().any(|rule| rule.cause.is_some()) && !self.audit_history {
                return Err("cause-scoped permissions require audited mutation history".into());
            }
            if policy
                .allow
                .iter()
                .any(|rule| rule.kind == crate::preservation::ChangeKind::Append)
                && !(self.audit_history || !self.ordering.is_empty())
            {
                return Err("append policies require audited mutation history".into());
            }
            if policy
                .allow
                .iter()
                .any(|rule| rule.count_scope == crate::preservation::CountScope::PerCommit)
                && !(self.audit_history || !self.ordering.is_empty())
            {
                return Err("per_commit limits require audited mutation history".into());
            }
        }
        if let Some(check) = &self.interaction {
            check.validate(self)?;
        }
        if self.state.is_empty()
            && self.records.is_empty()
            && self.relations.is_empty()
            && self.required_calls.is_empty()
            && self.answer.is_none()
        {
            return Err("contracts require a positive outcome predicate".into());
        }
        Ok(())
    }
}

pub(super) fn validate_call(matcher: &CallMatch, location: &str) -> Result<(), String> {
    if matcher.service.trim().is_empty() {
        return Err(format!("{location}.service must not be empty"));
    }
    if matcher.min_count == 0 {
        return Err(format!("{location}.min_count must be positive"));
    }
    if matcher.query_fields.keys().any(String::is_empty) {
        return Err(format!(
            "{location}.query_fields requires nonempty decoded parameter names"
        ));
    }
    for (name, value) in &matcher.headers {
        if axum::http::HeaderName::from_bytes(name.as_bytes())
            .map_or(true, |header| header.as_str() != name)
            || value.is_empty()
            || axum::http::HeaderValue::from_str(value).is_err()
        {
            return Err(format!(
                "{location}.headers requires canonical lowercase names and nonempty valid values"
            ));
        }
    }
    if let Some(method) = &matcher.method {
        axum::http::Method::from_bytes(method.as_bytes())
            .map_err(|error| format!("{location}.method: {error}"))?;
    }
    if matcher
        .path
        .as_ref()
        .is_some_and(|path| !path.starts_with('/'))
    {
        return Err(format!("{location}.path must start with /"));
    }
    if let Some(pattern) = &matcher.path_regex {
        Regex::new(pattern).map_err(|error| format!("{location}.path_regex: {error}"))?;
    }
    if matcher
        .query_contains
        .as_ref()
        .is_some_and(String::is_empty)
        || matcher.body_contains.as_ref().is_some_and(String::is_empty)
    {
        return Err(format!("{location} contains an empty text matcher"));
    }
    if matcher
        .response_status
        .iter()
        .any(|status| !(100..600).contains(status))
        || matcher
            .response_status
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != matcher.response_status.len()
    {
        return Err(format!(
            "{location}.response_status must contain unique HTTP status codes"
        ));
    }
    if matcher.allow_tool_error && matcher.response_status.is_empty() {
        return Err(format!(
            "{location}.allow_tool_error requires explicit response_status"
        ));
    }
    Ok(())
}
