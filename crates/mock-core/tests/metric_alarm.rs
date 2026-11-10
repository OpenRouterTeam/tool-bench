use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector":{"collection":"/services/aws/alarms","matching":{"AlarmName":"cpu"}},
        "equals":1,
        "metric_alarm":{
            "account":"481562039417","namespace":"AWS/EC2","metric":"CPUUtilization",
            "dimensions":{"InstanceId":"i-prod"},"statistic":"Average","period":300,
            "evaluation_periods":3,"threshold":80,"comparison":"GreaterThanThreshold",
            "unit":"Percent"
        }
    }))
    .unwrap()
}

fn direct() -> Value {
    json!({
        "AlarmName":"cpu","Namespace":"AWS/EC2","MetricName":"CPUUtilization",
        "Dimensions":[{"Name":"InstanceId","Value":"i-prod"}],
        "Statistic":"Average","Period":300,"EvaluationPeriods":3,
        "Threshold":80.0,"ComparisonOperator":"GreaterThanThreshold"
    })
}

fn queries() -> Value {
    let mut alarm = direct();
    for field in [
        "Namespace",
        "MetricName",
        "Dimensions",
        "Statistic",
        "Period",
    ] {
        alarm.as_object_mut().unwrap().remove(field);
    }
    alarm["Metrics"] = json!([{
        "Id":"cpu","ReturnData":true,
        "MetricStat":{
            "Metric":{"Namespace":"AWS/EC2","MetricName":"CPUUtilization",
                "Dimensions":[{"Name":"InstanceId","Value":"i-prod"}]},
            "Period":300,"Stat":"Average","Unit":"Percent"
        }
    }]);
    alarm
}

fn assess(alarm: Value) -> Result<(), RecordCheckError> {
    check().check(
        None,
        &json!({"services":{"aws":{"alarms":{"actual":alarm}}}}),
    )
}

fn failed(alarm: Value) {
    assert!(matches!(assess(alarm), Err(RecordCheckError::Failed(_))));
}

fn unresolved(alarm: Value) {
    assert!(matches!(
        assess(alarm),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn metric_alarm_uses_raw_direct_and_query_configurations_not_a_mock_projection() {
    for mut alarm in [direct(), queries()] {
        alarm["DatapointsToAlarm"] = json!(3);
        alarm["TreatMissingData"] = json!("missing");
        assess(alarm).unwrap();
    }
    assess(direct()).unwrap();
    let mut alarm = direct();
    alarm["Unit"] = json!("Percent");
    assess(alarm).unwrap();
}

#[test]
fn selected_identity_aliases_and_unselected_queries_do_not_change_the_cpu_series() {
    let mut alarm = queries();
    alarm["Metrics"][0]["ReturnData"] = json!(false);
    alarm["Metrics"].as_array_mut().unwrap().extend([
        json!({"Id":"alias","Expression":" cpu ","ReturnData":false}),
        json!({"Id":"result","Expression":"alias","Period":300,"ReturnData":true}),
        json!({"Id":"unused","Expression":"cpu * 2","ReturnData":false}),
    ]);
    assess(alarm).unwrap();
}

#[test]
fn known_wrong_metric_identity_statistics_windows_and_thresholds_fail() {
    for (field, value) in [
        ("Namespace", json!("AWS/Other")),
        ("MetricName", json!("NetworkIn")),
        (
            "Dimensions",
            json!([{"Name":"InstanceId","Value":"i-stage"}]),
        ),
        ("Dimensions", json!([])),
        (
            "Dimensions",
            json!([
                {"Name":"InstanceId","Value":"i-prod"},
                {"Name":"InstanceId","Value":"i-prod"}
            ]),
        ),
        ("Statistic", json!("Maximum")),
        ("Unit", json!("Count")),
        ("Period", json!(60)),
        ("DatapointsToAlarm", json!(2)),
        ("EvaluationPeriods", json!(2)),
        ("Threshold", json!(80.5)),
        ("ComparisonOperator", json!("GreaterThanOrEqualToThreshold")),
    ] {
        let mut alarm = direct();
        alarm[field] = value;
        failed(alarm);
    }
    let mut alarm = queries();
    alarm["Metrics"][0]["AccountId"] = json!("999999999999");
    failed(alarm);
}

#[test]
fn explicit_same_account_and_dimension_order_are_semantic_not_serialization_constraints() {
    let mut alarm = queries();
    alarm["Metrics"][0]["AccountId"] = json!("481562039417");
    assess(alarm).unwrap();
    let mut checker = check();
    let mut definition = serde_json::to_value(&checker).unwrap();
    definition["metric_alarm"]["dimensions"]["Environment"] = json!("prod");
    checker = serde_json::from_value(definition).unwrap();
    let mut alarm = direct();
    alarm["Dimensions"] = json!([
        {"Name":"Environment","Value":"prod"},{"Name":"InstanceId","Value":"i-prod"}
    ]);
    checker
        .check(
            None,
            &json!({"services":{"aws":{"alarms":{"actual":alarm}}}}),
        )
        .unwrap();
}

#[test]
fn metric_math_and_undocumented_output_defaults_require_adjudication() {
    for expression in ["IF(cpu > 80, 100, 0)", "cpu * 2", "METRICS()", "missing"] {
        let mut alarm = queries();
        alarm["Metrics"][0]["ReturnData"] = json!(false);
        alarm["Metrics"]
            .as_array_mut()
            .unwrap()
            .push(json!({"Id":"condition","Expression":expression,"ReturnData":true}));
        unresolved(alarm);
    }
    let mut alarm = queries();
    alarm["Metrics"][0]
        .as_object_mut()
        .unwrap()
        .remove("ReturnData");
    unresolved(alarm);
    let mut alarm = queries();
    alarm["Metrics"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Id":"other","Expression":"cpu","ReturnData":true}));
    unresolved(alarm);
}

#[test]
fn alias_cycles_period_changes_and_nondefault_missing_data_are_not_proved_equivalent() {
    let mut alarm = queries();
    alarm["Metrics"][0]["ReturnData"] = json!(false);
    alarm["Metrics"].as_array_mut().unwrap().extend([
        json!({"Id":"first","Expression":"second","ReturnData":true}),
        json!({"Id":"second","Expression":"first","ReturnData":false}),
    ]);
    unresolved(alarm);
    let mut alarm = queries();
    alarm["Metrics"][0]["Period"] = json!(60);
    unresolved(alarm);
}

#[test]
fn missing_data_policies_pass_only_when_the_alarm_still_needs_real_breaching_datapoints() {
    for policy in ["missing", "notBreaching", "ignore"] {
        let mut alarm = direct();
        alarm["TreatMissingData"] = json!(policy);
        assess(alarm).unwrap_or_else(|error| panic!("{policy}: {error}"));
    }
    for policy in [
        json!("breaching"),
        json!("sometimes"),
        json!(null),
        json!(1),
    ] {
        let mut alarm = direct();
        alarm["TreatMissingData"] = policy.clone();
        assert!(
            matches!(assess(alarm), Err(RecordCheckError::Failed(_))),
            "{policy}"
        );
    }
}

#[test]
fn unknown_semantics_do_not_hide_a_separate_known_requirement_failure() {
    let mut checker = check();
    checker.fields.insert("/ActionsEnabled".into(), json!(true));
    let mut alarm = queries();
    alarm["Metrics"][0]
        .as_object_mut()
        .unwrap()
        .remove("ReturnData");
    alarm["ActionsEnabled"] = json!(false);
    assert!(matches!(
        checker.check(
            None,
            &json!({"services":{"aws":{"alarms":{"actual":alarm}}}})
        ),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn metric_goals_require_well_formed_nonzero_exact_business_declarations() {
    for (field, value) in [
        ("account", json!("")),
        ("period", json!(0)),
        ("evaluation_periods", json!(0)),
        ("threshold", json!("80")),
        ("comparison", json!("unknown")),
        ("statistic", json!("")),
        ("dimensions", json!({"": "value"})),
    ] {
        let mut definition = serde_json::to_value(check()).unwrap();
        definition["metric_alarm"][field] = value;
        assert!(
            serde_json::from_value::<RecordCheck>(definition)
                .map_or(true, |check| check.validate().is_err())
        );
    }
    let mut definition = serde_json::to_value(check()).unwrap();
    definition["equals"] = json!(0);
    assert!(
        serde_json::from_value::<RecordCheck>(definition)
            .unwrap()
            .validate()
            .is_err()
    );
    for predicate in ["fields", "json_fields", "value_sets"] {
        let mut definition = serde_json::to_value(check()).unwrap();
        definition[predicate] = match predicate {
            "value_sets" => json!({"/Namespace":[["AWS/EC2"]]}),
            _ => json!({"/Namespace":"AWS/EC2"}),
        };
        assert!(
            serde_json::from_value::<RecordCheck>(definition)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
