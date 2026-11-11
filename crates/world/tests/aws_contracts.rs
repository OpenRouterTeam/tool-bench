use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{
    dataset::Dataset,
    grading::{Grade, GradeInput, Outcome, grade},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use scenario::Scenario;
use serde_json::{Value, json};
use tower::ServiceExt;

const IAM: &str = "cloud/aws/grant-auditor-access";
const ALARM: &str = "cloud/aws/orders-api-cpu-alarm";
const STOP: &str = "cloud/aws/stop-legacy-reporting-worker";
const KMS_LOOKUP: &str = "cloud/aws/find-unencrypted-bucket";
const AUDITOR: &str = "priya.raman-audit";
const POLICY: &str = "arn:aws:iam::aws:policy/SecurityAudit";
const INSTANCE: &str = "i-07b9d1f3a5c8e2046";
const CPU_INSTANCE: &str = "i-04c1a7e2b9d35f018";
const ALARM_NAME: &str = "orders-api-high-cpu";
const TOPIC: &str = "arn:aws:sns:us-east-1:481562039417:ops-pager";

struct Trial {
    scenario: Scenario,
    world: SharedWorld,
    app: Router,
}

impl Trial {
    fn new(id: &str) -> Self {
        Self::with_dataset(id, |_| {})
    }

    fn with_dataset(id: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios")
            .join(id);
        let scenario = Scenario::load(&root).unwrap();
        let mut dataset = Dataset::load(&root).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_aws::Aws);
        Self {
            scenario,
            world,
            app,
        }
    }

    async fn send(
        &self,
        service: &str,
        region: &str,
        operation: &str,
        body: Vec<u8>,
        query: bool,
    ) -> String {
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential=AKIATEST/20260314/{region}/{service}/aws4_request, SignedHeaders=host;x-amz-date, Signature=00"
        );
        let mut request = Request::post("/")
            .header("host", "world:8116")
            .header("authorization", authorization);
        if query {
            request = request.header(
                "content-type",
                "application/x-www-form-urlencoded; charset=utf-8",
            );
        } else {
            request = request
                .header("content-type", "application/x-amz-json-1.0")
                .header(
                    "x-amz-target",
                    format!("GraniteServiceVersion20100801.{operation}"),
                );
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(status, StatusCode::OK, "{service} {operation}: {text}");
        if query {
            assert!(mock_aws::util::parse_xml(&text).is_some(), "{text}");
            assert!(!text.contains("<Error>"), "{text}");
        }
        text
    }

    async fn query(&self, service: &str, action: &str, fields: &[(&str, &str)]) -> String {
        let mut parameters = vec![("Action", action)];
        parameters.extend_from_slice(fields);
        let body = parameters
            .iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    mock_aws::util::percent_encode(name),
                    mock_aws::util::percent_encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        self.send(service, "us-east-1", action, body.into_bytes(), true)
            .await
    }

    async fn cloudwatch(&self, operation: &str, input: Value) -> Value {
        self.cloudwatch_region("us-east-1", operation, input).await
    }

    async fn cloudwatch_region(&self, region: &str, operation: &str, input: Value) -> Value {
        let text = self
            .send(
                "monitoring",
                region,
                operation,
                input.to_string().into_bytes(),
                false,
            )
            .await;
        let result: Value = serde_json::from_str(&text).unwrap();
        assert!(result.get("__type").is_none(), "{result}");
        result
    }

    async fn attach(&self, user: &str, policy: &str) {
        self.query(
            "iam",
            "AttachUserPolicy",
            &[("UserName", user), ("PolicyArn", policy)],
        )
        .await;
    }

    async fn member(&self, user: &str, group: &str) {
        self.query(
            "iam",
            "AddUserToGroup",
            &[("UserName", user), ("GroupName", group)],
        )
        .await;
    }

    async fn grant(&self) {
        self.attach(AUDITOR, POLICY).await;
        self.member(AUDITOR, "auditors").await;
    }

    async fn stop(&self, instance: &str) {
        self.query("ec2", "StopInstances", &[("InstanceId.1", instance)])
            .await;
    }

    async fn tag(&self, resource: &str, key: &str, value: &str) {
        self.query(
            "ec2",
            "CreateTags",
            &[
                ("ResourceId.1", resource),
                ("Tag.1.Key", key),
                ("Tag.1.Value", value),
            ],
        )
        .await;
    }

    async fn shutdown(&self) {
        self.stop(INSTANCE).await;
        self.tag(INSTANCE, "DecommissionTicket", "OPS-4821").await;
    }

    fn state(&self) -> Value {
        self.world.lock().unwrap().snapshot()["services"]["aws"].clone()
    }

    fn assess(&self) -> Grade {
        self.assess_with_answer(None)
    }

    fn assess_with_answer(&self, answer: Option<&str>) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.scenario.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer,
            tools_ips: vec!["unknown".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&world.initial_snapshot()),
            events: Some(world.events().entries()),
        })
    }

    async fn read_bucket_encryption(&self, name: &str) {
        let response = self.app.clone().oneshot(
            Request::get(format!("/{name}?encryption"))
                .header("host", "world:8116")
                .header("authorization", "AWS4-HMAC-SHA256 Credential=AKIATEST/20260314/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-date, Signature=00")
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(mock_aws::util::parse_xml(std::str::from_utf8(&bytes).unwrap()).is_some());
    }
}

fn alarm() -> Value {
    json!({
        "AlarmName":ALARM_NAME,"Namespace":"AWS/EC2","MetricName":"CPUUtilization",
        "Dimensions":[{"Name":"InstanceId","Value":CPU_INSTANCE}],
        "Statistic":"Average","Period":300,"EvaluationPeriods":3,"Threshold":80.0,
        "ComparisonOperator":"GreaterThanThreshold","AlarmActions":[TOPIC]
    })
}

fn metric_alarm() -> Value {
    let mut definition = alarm();
    for field in [
        "Namespace",
        "MetricName",
        "Dimensions",
        "Statistic",
        "Period",
    ] {
        definition.as_object_mut().unwrap().remove(field);
    }
    definition["Metrics"] = json!([{
        "Id":"cpu","MetricStat":{"Metric":{"Namespace":"AWS/EC2","MetricName":"CPUUtilization",
            "Dimensions":[{"Name":"InstanceId","Value":CPU_INSTANCE}]},"Period":300,"Stat":"Average","Unit":"Percent"},
        "ReturnData":true
    }]);
    definition
}

fn pass(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
    assert!(grade.preservation_checked && grade.history_checked);
}

fn fail(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(
        grade.contract_ok && grade.environment_ok && grade.integrity_ok,
        "{grade:?}"
    );
}

#[tokio::test]
async fn aws_writes_require_real_outcomes_and_audited_preservation() {
    for id in [IAM, ALARM, STOP] {
        let trial = Trial::new(id);
        fail(&trial.assess());
        match id {
            IAM => trial.grant().await,
            ALARM => {
                trial.cloudwatch("PutMetricAlarm", alarm()).await;
            }
            _ => trial.shutdown().await,
        }
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn iam_operations_may_be_reversed_and_retried_without_losing_existing_members() {
    let trial = Trial::with_dataset(IAM, |dataset| {
        dataset
            .services
            .get_mut("aws")
            .unwrap()
            .collection_mut("groups")
            .get_mut("auditors")
            .unwrap()["Members"] = json!(["jchen", "lnovak"]);
    });
    trial.member(AUDITOR, "auditors").await;
    trial.attach(AUDITOR, POLICY).await;
    trial.grant().await;
    pass(&trial.assess());
}

#[tokio::test]
async fn granting_the_employee_or_extra_permissions_is_not_the_external_auditor_task() {
    let wrong = Trial::new(IAM);
    wrong.attach("praman", POLICY).await;
    wrong.member("praman", "auditors").await;
    fail(&wrong.assess());
    let extra = Trial::new(IAM);
    extra.grant().await;
    extra
        .attach("jchen", "arn:aws:iam::aws:policy/AdministratorAccess")
        .await;
    fail(&extra.assess());
}

#[tokio::test]
async fn managed_policy_success_cannot_hide_an_inline_administrator_policy() {
    let trial = Trial::new(IAM);
    let policy=json!({"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}).to_string();
    trial
        .query(
            "iam",
            "PutUserPolicy",
            &[
                ("UserName", AUDITOR),
                ("PolicyName", "extra-admin"),
                ("PolicyDocument", &policy),
            ],
        )
        .await;
    trial.grant().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn unrelated_group_changes_remain_failures_after_their_membership_is_restored() {
    let trial = Trial::new(IAM);
    trial.member("praman", "admins").await;
    trial
        .query(
            "iam",
            "RemoveUserFromGroup",
            &[("UserName", "praman"), ("GroupName", "admins")],
        )
        .await;
    trial.grant().await;
    let grade = trial.assess();
    fail(&grade);
    assert!(!grade.history_ok, "{grade:?}");
}

#[tokio::test]
async fn removing_an_existing_auditor_temporarily_is_not_authorized_by_the_final_member_list() {
    let trial = Trial::new(IAM);
    trial
        .query(
            "iam",
            "RemoveUserFromGroup",
            &[("UserName", "lnovak"), ("GroupName", "auditors")],
        )
        .await;
    trial.member("lnovak", "auditors").await;
    trial.grant().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn unrelated_user_tags_are_protected_even_when_managed_permissions_are_correct() {
    let trial = Trial::new(IAM);
    trial
        .query(
            "iam",
            "TagUser",
            &[
                ("UserName", AUDITOR),
                ("Tags.member.1.Key", "company"),
                ("Tags.member.1.Value", "Other Firm"),
            ],
        )
        .await;
    trial.grant().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn direct_and_single_metric_stat_alarms_are_valid_configuration_representations() {
    for mut definition in [alarm(), metric_alarm()] {
        let trial = Trial::new(ALARM);
        definition["Threshold"] = json!(80);
        definition["DatapointsToAlarm"] = json!(3);
        definition["ActionsEnabled"] = json!(true);
        trial.cloudwatch("PutMetricAlarm", definition).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn an_explicit_percent_unit_description_and_quiet_missing_data_are_the_same_alarm() {
    let trial = Trial::new(ALARM);
    let mut definition = alarm();
    definition.as_object_mut().unwrap().extend(
        json!({"Unit":"Percent","AlarmDescription":"orders-api prod CPU above 80% for 15 minutes","TreatMissingData":"notBreaching","DatapointsToAlarm":3})
            .as_object()
            .unwrap()
            .clone(),
    );
    trial.cloudwatch("PutMetricAlarm", definition).await;
    pass(&trial.assess());
    // Paging on recovery, or firing when the metric goes missing, was not asked for.
    for extra in [
        json!({"OKActions":[TOPIC]}),
        json!({"TreatMissingData":"breaching"}),
    ] {
        let trial = Trial::new(ALARM);
        let mut definition = alarm();
        definition
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        trial.cloudwatch("PutMetricAlarm", definition).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn a_metric_query_identity_alias_does_not_change_the_requested_cpu_condition() {
    let trial = Trial::new(ALARM);
    let mut definition = metric_alarm();
    definition["Metrics"][0]["ReturnData"] = json!(false);
    definition["Metrics"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Id":"condition","Expression":"cpu","ReturnData":true}));
    trial.cloudwatch("PutMetricAlarm", definition).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn disabled_actions_two_of_three_or_wrong_units_do_not_establish_the_requested_alarm() {
    for extra in [
        json!({"ActionsEnabled":false}),
        json!({"DatapointsToAlarm":2}),
        json!({"Unit":"Count"}),
        json!({"Period":60}),
        json!({"Threshold":80.5}),
        json!({"ComparisonOperator":"GreaterThanOrEqualToThreshold"}),
    ] {
        let trial = Trial::new(ALARM);
        let mut definition = alarm();
        definition
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        trial.cloudwatch("PutMetricAlarm", definition).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn a_single_metric_query_cannot_hide_a_wrong_instance_or_foreign_metric_account() {
    for foreign in [false, true] {
        let trial = Trial::new(ALARM);
        let mut definition = metric_alarm();
        if foreign {
            definition["Metrics"][0]["AccountId"] = json!("999999999999");
        } else {
            definition["Metrics"][0]["MetricStat"]["Metric"]["Dimensions"][0]["Value"] =
                json!("i-0e4a6c8b2d1f35790");
        }
        trial.cloudwatch("PutMetricAlarm", definition).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn unproved_metric_math_requires_semantic_review_not_an_invented_model_failure() {
    let trial = Trial::new(ALARM);
    let mut definition = metric_alarm();
    definition["Metrics"][0]["ReturnData"] = json!(false);
    definition["Metrics"].as_array_mut().unwrap().push(json!({
        "Id":"condition","Expression":"IF(cpu > 80, 100, 0)","ReturnData":true
    }));
    trial.cloudwatch("PutMetricAlarm", definition).await;
    let grade = trial.assess();
    assert_eq!(grade.outcome, Outcome::Unresolved, "{grade:?}");
    assert!(grade.adjudication_needed && grade.environment_ok);
}

#[tokio::test]
async fn existing_alarm_action_changes_cannot_be_hidden_by_unchanged_configuration_timestamps() {
    let trial = Trial::new(ALARM);
    trial
        .cloudwatch(
            "DisableAlarmActions",
            json!({"AlarmNames":["orders-api-status-check"]}),
        )
        .await;
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn restored_alarm_action_changes_leave_a_committed_preservation_failure() {
    let trial = Trial::new(ALARM);
    trial
        .cloudwatch(
            "DisableAlarmActions",
            json!({"AlarmNames":["orders-api-status-check"]}),
        )
        .await;
    trial
        .cloudwatch(
            "EnableAlarmActions",
            json!({"AlarmNames":["orders-api-status-check"]}),
        )
        .await;
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn additional_alarms_are_not_authorized_by_a_correct_requested_alarm() {
    let trial = Trial::new(ALARM);
    let mut extra = alarm();
    extra["AlarmName"] = json!("unrequested-extra-alarm");
    trial.cloudwatch("PutMetricAlarm", extra).await;
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn alarm_names_are_regional_and_cannot_overwrite_another_regions_alarm() {
    let trial = Trial::new(ALARM);
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    let eastern = trial
        .cloudwatch("DescribeAlarms", json!({"AlarmNames":[ALARM_NAME]}))
        .await["MetricAlarms"][0]
        .clone();
    let mut western = alarm();
    western["Threshold"] = json!(90.0);
    western["AlarmActions"] = json!([]);
    trial
        .cloudwatch_region("us-west-2", "PutMetricAlarm", western)
        .await;
    let after = trial
        .cloudwatch("DescribeAlarms", json!({"AlarmNames":[ALARM_NAME]}))
        .await["MetricAlarms"][0]
        .clone();
    assert_eq!(after, eastern);
    let western = trial
        .cloudwatch_region(
            "us-west-2",
            "DescribeAlarms",
            json!({"AlarmNames":[ALARM_NAME]}),
        )
        .await;
    assert_eq!(western["MetricAlarms"][0]["Threshold"], 90.0);
}

#[tokio::test]
async fn stopping_and_tagging_can_be_reversed_and_retried_while_keeping_the_disk() {
    let trial = Trial::new(STOP);
    let volumes = trial.state()["volumes"].clone();
    trial.tag(INSTANCE, "DecommissionTicket", "OPS-4821").await;
    trial.stop(INSTANCE).await;
    trial.shutdown().await;
    assert_eq!(trial.state()["volumes"], volumes);
    pass(&trial.assess());
}

#[tokio::test]
async fn a_wrong_staging_stop_or_an_unrequested_worker_stop_is_not_safe_shutdown() {
    for instance in ["i-0e4a6c8b2d1f35790", "i-0a3f5c7e9b1d24680"] {
        let trial = Trial::new(STOP);
        trial.stop(instance).await;
        trial.shutdown().await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn changing_the_target_instance_type_is_not_implied_by_stopping_it() {
    let trial = Trial::new(STOP);
    trial.shutdown().await;
    trial
        .query(
            "ec2",
            "ModifyInstanceAttribute",
            &[("InstanceId", INSTANCE), ("InstanceType.Value", "t3.small")],
        )
        .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn existing_instance_tag_changes_are_not_hidden_by_the_requested_ticket_tag() {
    let trial = Trial::new(STOP);
    trial.tag(INSTANCE, "Team", "other-team").await;
    trial.shutdown().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn the_protected_volume_cannot_receive_an_unrequested_deletion_tag() {
    let trial = Trial::new(STOP);
    trial
        .tag("vol-07c9e1a3b5d7f2046", "DeleteAfter", "2026-03-14")
        .await;
    trial.shutdown().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn a_restored_ticket_value_cannot_erase_an_earlier_wrong_tag() {
    let trial = Trial::new(STOP);
    trial.tag(INSTANCE, "DecommissionTicket", "WRONG").await;
    trial.shutdown().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn restarting_the_decommissioned_worker_and_stopping_it_again_remains_visible() {
    let trial = Trial::new(STOP);
    trial.shutdown().await;
    trial
        .query("ec2", "StartInstances", &[("InstanceId.1", INSTANCE)])
        .await;
    trial.stop(INSTANCE).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn alarm_reads_with_state_filters_do_not_count_as_state_mutations() {
    let trial = Trial::new(ALARM);
    trial
        .cloudwatch("DescribeAlarms", json!({"StateValue":"ALARM"}))
        .await;
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn a_manual_state_override_is_forbidden_even_when_its_reason_is_empty() {
    let trial = Trial::new(ALARM);
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    trial
        .cloudwatch(
            "SetAlarmState",
            json!({"AlarmName":ALARM_NAME,"StateValue":"ALARM","StateReason":""}),
        )
        .await;
    let grade = trial.assess();
    fail(&grade);
    assert!(!grade.forbidden_calls_ok, "{grade:?}");
}

#[tokio::test]
async fn harmless_alarm_retries_and_representation_changes_preserve_existing_resources() {
    let trial = Trial::new(ALARM);
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    trial.cloudwatch("PutMetricAlarm", metric_alarm()).await;
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn put_metric_alarm_replaces_configuration_preserves_state_and_ignores_update_tags() {
    let trial = Trial::new(ALARM);
    let mut first = alarm();
    first["Tags"] = json!([{"Key":"Owner","Value":"orders"}]);
    first["ActionsEnabled"] = json!(false);
    first["AlarmDescription"] = json!("old configuration");
    trial.cloudwatch("PutMetricAlarm", first).await;
    trial
        .cloudwatch(
            "SetAlarmState",
            json!({"AlarmName":ALARM_NAME,"StateValue":"ALARM","StateReason":"manual"}),
        )
        .await;
    let mut update = alarm();
    update["Tags"] = json!([{"Key":"Owner","Value":"unauthorized update tag"}]);
    trial.cloudwatch("PutMetricAlarm", update).await;
    let actual = trial
        .cloudwatch("DescribeAlarms", json!({"AlarmNames":[ALARM_NAME]}))
        .await["MetricAlarms"][0]
        .clone();
    assert_eq!(actual["StateValue"], "ALARM");
    assert_eq!(actual["StateReason"], "manual");
    assert_eq!(actual["ActionsEnabled"], true);
    assert!(actual.get("AlarmDescription").is_none());
    let tags = trial
        .cloudwatch(
            "ListTagsForResource",
            json!({"ResourceARN":format!("arn:aws:cloudwatch:us-east-1:481562039417:alarm:{ALARM_NAME}")}),
        )
        .await;
    assert_eq!(tags["Tags"], json!([{"Key":"Owner","Value":"orders"}]));
}

#[tokio::test]
async fn regional_mutations_tags_and_history_follow_the_actual_alarm_not_its_storage_key() {
    let trial = Trial::new(ALARM);
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    trial
        .cloudwatch_region("us-west-2", "PutMetricAlarm", alarm())
        .await;
    trial
        .cloudwatch_region(
            "us-west-2",
            "DisableAlarmActions",
            json!({"AlarmNames":[ALARM_NAME]}),
        )
        .await;
    let western_arn = format!("arn:aws:cloudwatch:us-west-2:481562039417:alarm:{ALARM_NAME}");
    trial
        .cloudwatch_region(
            "us-west-2",
            "TagResource",
            json!({"ResourceARN":western_arn,"Tags":[{"Key":"Region","Value":"west"}]}),
        )
        .await;
    let eastern = trial
        .cloudwatch("DescribeAlarms", json!({"AlarmNames":[ALARM_NAME]}))
        .await["MetricAlarms"][0]
        .clone();
    assert_eq!(eastern["ActionsEnabled"], true);
    let western = trial
        .cloudwatch_region(
            "us-west-2",
            "DescribeAlarms",
            json!({"AlarmNames":[ALARM_NAME]}),
        )
        .await["MetricAlarms"][0]
        .clone();
    assert_eq!(western["ActionsEnabled"], false);
    assert!(western.get("_region").is_none());
    let tags = trial
        .cloudwatch_region(
            "us-west-2",
            "ListTagsForResource",
            json!({"ResourceARN":western_arn}),
        )
        .await;
    assert_eq!(tags["Tags"], json!([{"Key":"Region","Value":"west"}]));
    for region in ["us-east-1", "us-west-2"] {
        let history = trial
            .cloudwatch_region(
                region,
                "DescribeAlarmHistory",
                json!({"AlarmName":ALARM_NAME}),
            )
            .await;
        let items = history["AlarmHistoryItems"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].get("_region").is_none());
        assert!(
            items[0]["HistoryData"]
                .as_str()
                .unwrap()
                .contains(&format!("cloudwatch:{region}:"))
        );
    }
    trial
        .cloudwatch_region(
            "us-west-2",
            "DeleteAlarms",
            json!({"AlarmNames":[ALARM_NAME]}),
        )
        .await;
    assert_eq!(
        trial
            .cloudwatch("DescribeAlarms", json!({"AlarmNames":[ALARM_NAME]}))
            .await["MetricAlarms"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        trial
            .cloudwatch_region(
                "us-west-2",
                "DescribeAlarmHistory",
                json!({"AlarmName":ALARM_NAME})
            )
            .await["AlarmHistoryItems"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn regional_storage_and_history_allocation_do_not_overwrite_existing_colliding_keys() {
    let trial = Trial::with_dataset(ALARM, |dataset| {
        let service = dataset.services.get_mut("aws").unwrap();
        service.collection_mut("alarms").insert(
            format!("us-west-2/{ALARM_NAME}#0"),
            json!({"AlarmName":format!("us-west-2/{ALARM_NAME}#0"),
                "AlarmArn":format!("arn:aws:cloudwatch:us-east-1:481562039417:alarm:us-west-2/{ALARM_NAME}#0")}),
        );
        service.collection_mut("alarm_history").insert(
            format!("{ALARM_NAME}#1"),
            json!({"AlarmName":"existing","HistorySummary":"retain this history"}),
        );
    });
    let before = trial.state();
    trial.cloudwatch("PutMetricAlarm", alarm()).await;
    trial
        .cloudwatch_region("us-west-2", "PutMetricAlarm", alarm())
        .await;
    assert_eq!(
        trial.state()["alarms"][format!("us-west-2/{ALARM_NAME}#0")],
        before["alarms"][format!("us-west-2/{ALARM_NAME}#0")]
    );
    assert_eq!(
        trial.state()["alarm_history"][format!("{ALARM_NAME}#1")],
        before["alarm_history"][format!("{ALARM_NAME}#1")]
    );
    assert_eq!(
        trial
            .cloudwatch_region(
                "us-west-2",
                "DescribeAlarms",
                json!({"AlarmNames":[ALARM_NAME]})
            )
            .await["MetricAlarms"][0]["AlarmName"],
        ALARM_NAME
    );
}

#[tokio::test]
async fn missing_query_output_selection_remains_reviewable_without_hiding_known_action_failures() {
    for disabled in [false, true] {
        let trial = Trial::new(ALARM);
        let mut definition = metric_alarm();
        definition["Metrics"][0]
            .as_object_mut()
            .unwrap()
            .remove("ReturnData");
        definition["ActionsEnabled"] = json!(!disabled);
        trial.cloudwatch("PutMetricAlarm", definition).await;
        let grade = trial.assess();
        if disabled {
            fail(&grade);
        } else {
            assert_eq!(grade.outcome, Outcome::Unresolved, "{grade:?}");
        }
    }
}

#[tokio::test]
async fn modern_encryption_lookup_identifies_a_policy_mismatch_not_an_unencrypted_bucket() {
    let trial = Trial::new(KMS_LOOKUP);
    let buckets = trial.state()["buckets"].clone();
    let non_kms: Vec<_> = buckets
        .as_object()
        .unwrap()
        .values()
        .filter(|bucket| bucket["Encryption"]["SSEAlgorithm"] != "aws:kms")
        .collect();
    assert_eq!(non_kms.len(), 1);
    assert_eq!(non_kms[0]["Name"], "northwind-legacy-exports");
    assert_eq!(non_kms[0]["Encryption"]["SSEAlgorithm"], "AES256");
    fail(&trial.assess_with_answer(Some("northwind-legacy-exports")));
    trial
        .read_bucket_encryption("northwind-legacy-exports")
        .await;
    pass(&trial.assess_with_answer(Some("northwind-legacy-exports")));
    fail(&trial.assess_with_answer(Some("northwind-legacy-exports-2019")));
    assert_eq!(trial.state()["buckets"], buckets);
}

#[tokio::test]
async fn a_similarly_named_compliant_bucket_is_not_the_required_lookup_evidence() {
    let trial = Trial::new(KMS_LOOKUP);
    trial
        .read_bucket_encryption("northwind-legacy-exports-2019")
        .await;
    let grade = trial.assess_with_answer(Some("northwind-legacy-exports"));
    fail(&grade);
    assert!(!grade.required_calls_ok);
}
