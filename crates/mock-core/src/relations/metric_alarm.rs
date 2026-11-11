use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::RecordCheckError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricAlarmGoal {
    pub account: String,
    pub namespace: String,
    pub metric: String,
    pub dimensions: BTreeMap<String, String>,
    pub statistic: String,
    pub period: u64,
    pub evaluation_periods: u64,
    pub threshold: Value,
    pub comparison: String,
    pub unit: String,
}

fn failed(reason: &str) -> RecordCheckError {
    RecordCheckError::Failed(format!("metric alarm: {reason}"))
}

fn unresolved(reason: &str) -> RecordCheckError {
    RecordCheckError::NeedsAdjudication(format!("metric alarm: {reason}"))
}

impl MetricAlarmGoal {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.account.len() != 12
            || !self.account.bytes().all(|byte| byte.is_ascii_digit())
            || [&self.namespace, &self.metric, &self.statistic, &self.unit]
                .iter()
                .any(|value| value.trim().is_empty())
            || self.period == 0
            || self.evaluation_periods == 0
            || !self.threshold.is_number()
            || ![
                "GreaterThanThreshold",
                "GreaterThanOrEqualToThreshold",
                "LessThanThreshold",
                "LessThanOrEqualToThreshold",
            ]
            .contains(&self.comparison.as_str())
            || self
                .dimensions
                .iter()
                .any(|(name, value)| name.is_empty() || value.is_empty())
        {
            return Err("metric-alarm goals require an account, metric, exact dimensions, positive windows, numeric threshold and static comparison".into());
        }
        Ok(())
    }

    pub(super) fn owns_field(pointer: &str) -> bool {
        [
            "Metrics",
            "Namespace",
            "MetricName",
            "Dimensions",
            "Statistic",
            "ExtendedStatistic",
            "Period",
            "Unit",
            "EvaluationPeriods",
            "DatapointsToAlarm",
            "Threshold",
            "ThresholdMetricId",
            "ComparisonOperator",
            "TreatMissingData",
        ]
        .contains(&pointer.split('/').nth(1).unwrap_or_default())
    }

    pub(super) fn check(&self, alarm: &Value) -> Result<(), RecordCheckError> {
        if let Some(queries) = alarm.get("Metrics") {
            if [
                "Namespace",
                "MetricName",
                "Dimensions",
                "Statistic",
                "ExtendedStatistic",
                "Period",
                "Unit",
            ]
            .iter()
            .any(|field| alarm.get(field).is_some())
            {
                return Err(failed("query and direct metric forms are mixed"));
            }
            self.check_queries(queries)?;
        } else {
            if alarm.get("ExtendedStatistic").is_some() {
                return Err(failed(
                    "the requested ordinary statistic is not established",
                ));
            }
            self.check_series(
                alarm,
                alarm.get("Statistic").and_then(Value::as_str),
                alarm.get("Period"),
                alarm.get("Unit"),
                None,
            )?;
        }
        if alarm.get("ThresholdMetricId").is_some() {
            return Err(unresolved("dynamic threshold semantics require review"));
        }
        if alarm.get("EvaluationPeriods").and_then(Value::as_u64) != Some(self.evaluation_periods)
            || alarm
                .get("DatapointsToAlarm")
                .unwrap_or(&alarm["EvaluationPeriods"])
                .as_u64()
                != Some(self.evaluation_periods)
            || !alarm.get("Threshold").is_some_and(|threshold| {
                threshold.is_number() && crate::decimal::equivalent(&self.threshold, threshold)
            })
            || alarm.get("ComparisonOperator").and_then(Value::as_str)
                != Some(self.comparison.as_str())
        {
            return Err(failed("the threshold, comparison or M-of-N window differs"));
        }
        // `notBreaching` and `ignore` still need real breaching datapoints, so the alarm fires
        // on the requested condition. `breaching` also fires when the metric stops reporting.
        match alarm.get("TreatMissingData").map(Value::as_str) {
            None | Some(Some("missing" | "notBreaching" | "ignore")) => Ok(()),
            _ => Err(failed(
                "missing data counts as breaching, so the alarm also fires without the requested condition",
            )),
        }
    }

    fn check_queries(&self, source: &Value) -> Result<(), RecordCheckError> {
        let queries = source
            .as_array()
            .filter(|queries| !queries.is_empty())
            .ok_or_else(|| failed("Metrics must be a nonempty query list"))?;
        let mut by_id = BTreeMap::new();
        let mut selected = Vec::new();
        for query in queries {
            let identity = query
                .get("Id")
                .and_then(Value::as_str)
                .filter(|identity| !identity.is_empty())
                .ok_or_else(|| failed("a metric query lacks its identity"))?;
            if by_id.insert(identity, query).is_some() {
                return Err(failed("metric query identities are not unique"));
            }
            match query.get("ReturnData").and_then(Value::as_bool) {
                Some(true) => selected.push(identity),
                Some(false) => {}
                None => {
                    return Err(unresolved(
                        "output selection has no proved explicit default",
                    ));
                }
            }
        }
        if selected.len() != 1 {
            return Err(unresolved("the watched query is not uniquely established"));
        }
        let mut identity = selected[0];
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(identity) {
                return Err(unresolved("cyclic metric query semantics require review"));
            }
            let query = by_id
                .get(identity)
                .ok_or_else(|| unresolved("the selected series cannot be resolved"))?;
            if query
                .get("Period")
                .is_some_and(|period| period.as_u64() != Some(self.period))
            {
                return Err(unresolved("query-period overrides require review"));
            }
            match (query.get("MetricStat"), query.get("Expression")) {
                (Some(stat), None) => {
                    return self.check_series(
                        &stat["Metric"],
                        stat.get("Stat").and_then(Value::as_str),
                        stat.get("Period"),
                        stat.get("Unit"),
                        query.get("AccountId"),
                    );
                }
                (None, Some(Value::String(expression))) => {
                    if query
                        .get("AccountId")
                        .is_some_and(|account| account != self.account.as_str())
                    {
                        return Err(unresolved(
                            "account-qualified expression semantics require review",
                        ));
                    }
                    let alias = expression.trim();
                    if !by_id.contains_key(alias) {
                        return Err(unresolved(
                            "unevaluated metric math requires semantic review",
                        ));
                    }
                    identity = alias;
                }
                _ => {
                    return Err(failed(
                        "a watched query must identify a metric or expression",
                    ));
                }
            }
        }
    }

    fn check_series(
        &self,
        metric: &Value,
        statistic: Option<&str>,
        period: Option<&Value>,
        unit: Option<&Value>,
        account: Option<&Value>,
    ) -> Result<(), RecordCheckError> {
        let dimensions = metric
            .get("Dimensions")
            .and_then(Value::as_array)
            .map(|dimensions| {
                let mut values = BTreeMap::new();
                for dimension in dimensions {
                    let name = dimension.get("Name")?.as_str()?;
                    let value = dimension.get("Value")?.as_str()?;
                    if values.insert(name.to_owned(), value.to_owned()).is_some() {
                        return None;
                    }
                }
                Some(values)
            })
            .unwrap_or_else(|| metric.get("Dimensions").is_none().then(BTreeMap::new));
        if metric.get("Namespace").and_then(Value::as_str) != Some(self.namespace.as_str())
            || metric.get("MetricName").and_then(Value::as_str) != Some(self.metric.as_str())
            || dimensions.as_ref() != Some(&self.dimensions)
            || statistic != Some(self.statistic.as_str())
            || period.and_then(Value::as_u64) != Some(self.period)
            || unit.is_some_and(|unit| unit != self.unit.as_str())
            || account.is_some_and(|account| account != self.account.as_str())
        {
            return Err(failed(
                "the selected metric identity, statistic, period or unit differs",
            ));
        }
        Ok(())
    }
}
