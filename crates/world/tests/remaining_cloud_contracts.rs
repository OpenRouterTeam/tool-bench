mod support;

use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{
    grading::{Grade, Outcome},
    serve::service_app,
};
use serde_json::{Value, json};
use support::ContractTrial;
use tower::ServiceExt;

const DECOMMISSION: &str = "cloud/multi/aws-jira-decommission-worker";
const REPLICA: &str = "cloud/multi/aws-replica-lag-pagerduty-notion";
const OFFBOARD: &str = "cloud/multi/okta-offboard-linear-slack";

const PRODUCTION_WORKER: &str = "i-07b9d1f3a5c8e2046";
const STAGING_WORKER: &str = "i-0e4a6c8b2d1f35790";
const OTHER_PRODUCTION_WORKER: &str = "i-0a3f5c7e9b1d24680";
const SCHEDULER: &str = "i-0c8e2a4f6b1d39057";
const WORKER_VOLUME: &str = "vol-07c9e1a3b5d7f2046";
const TICKET: &str = "OPS-34";

const INCIDENT: &str = "Q3K8ORDBLAG2RX";
const RESOLVED_INCIDENT: &str = "Q2ORDBOLD4KZPA";
const INVENTORY_INCIDENT: &str = "Q1INVDBLAG7QWE";
const STAGING_INCIDENT: &str = "Q5DBSTG9LAGT3N";
const ALARM: &str = "orders-db-replica-2-replica-lag";
const POSTMORTEMS: &str = "7d2e3f4a-5b6c-4d7e-9f80-a1b2c3d4e5f6";
const POSTMORTEM_DATABASE: &str = "6c1d2e3f-4a5b-4c6d-8e7f-90a1b2c3d4e5";
const LAUNCHES: &str = "0b7747c0-5ed0-4c77-a8b1-867a05a3c300";
const LAST_WEEK: &str = "8e3f4a5b-6c7d-4e8f-a091-b2c3d4e5f607";

const FISCHER: &str = "00u1lnfschr3Ab4Cd5x7";
const FISHER: &str = "00u1lnfshr07Ef8Gh9x7";
const LINEAR_FISCHER: &str = "3dbdd473-1a04-5dda-ad95-4c583313bccd";
const LINEAR_CASEY: &str = "2dad689e-9ace-5467-9b02-0cc4e445cfa4";
const PAYMENTS: &str = "5854633b-d1e9-52b6-b3c7-d309bfb881cb";
const WEB_APP: &str = "6da75b4c-0e7c-5ac7-a415-f09a129582b1";
const PLATFORM: &str = "a7c70c10-090f-50bf-a8ce-b4cdee9d8572";
const IT_OPS: &str = "C0ITOPS5LF2";
const IT_ARCHIVE: &str = "C0ITREQ8ARC";
const GENERAL: &str = "C01GENERAL0";

const DECOMMISSION_COMMENT: &str = "Stopped (not terminated) the production instance reporting-worker-legacy (i-07b9d1f3a5c8e2046) and tagged it DecommissionTicket=OPS-34. Its EBS volume is kept. The staging copy (i-0e4a6c8b2d1f35790) is still running.";
const ALARM_NOTE: &str = "CloudWatch alarm orders-db-replica-2-replica-lag went into ALARM at 2026-10-30T13:11:00Z (ReplicaLag max 412s vs 300s threshold).";
const OFFBOARD_NOTICE: &str = "Offboarding notice: Lena Fischer (Product) has been offboarded. Her Okta account is deactivated (not deleted) and she has been removed from the Payments and Web App teams in Linear.";

static EXPORT_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

struct CloudTrial {
    trial: ContractTrial,
    aws: Option<Router>,
}

impl CloudTrial {
    fn for_decommission() -> Self {
        let trial = ContractTrial::load_many_with_dataset(
            DECOMMISSION,
            &[(
                &mock_jira::Jira,
                &[("authorization", "Bearer synthetic-jira-contract")],
            )],
            |_| {},
        );
        let aws = Some(service_app(trial.world.clone(), &mock_aws::Aws));
        Self { trial, aws }
    }

    fn for_replica_without_evidence() -> Self {
        let trial = ContractTrial::load_many_with_dataset(
            REPLICA,
            &[
                (
                    &mock_pagerduty::Pagerduty,
                    &[
                        ("authorization", "Token token=local-fixture-credential"),
                        ("from", "lena.weber@tidewater.io"),
                    ],
                ),
                (
                    &mock_notion::Notion,
                    &[
                        ("authorization", "Bearer local-notion-fixture"),
                        ("notion-version", "2026-03-11"),
                    ],
                ),
            ],
            |_| {},
        );
        let aws = Some(service_app(trial.world.clone(), &mock_aws::Aws));
        Self { trial, aws }
    }

    async fn for_replica() -> Self {
        let cloud = Self::for_replica_without_evidence();
        cloud.read_alarm_evidence().await;
        cloud
    }

    fn for_offboard() -> Self {
        let trial = ContractTrial::load_many_with_dataset(
            OFFBOARD,
            &[
                (
                    &mock_okta::Okta,
                    &[("authorization", "Bearer local-fixture-credential")],
                ),
                (&mock_linear::Linear, &[]),
                (&mock_slack::Slack, &[]),
            ],
            |_| {},
        );
        Self { trial, aws: None }
    }

    async fn aws_query(&self, service: &str, action: &str, fields: &[(&str, &str)]) -> String {
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
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential=AKIATEST/20261029/us-east-1/{service}/aws4_request, SignedHeaders=host;x-amz-date, Signature=00"
        );
        let response = self
            .aws
            .as_ref()
            .unwrap()
            .clone()
            .oneshot(
                Request::post("/")
                    .header("host", "world:8116")
                    .header("authorization", authorization)
                    .header(
                        "content-type",
                        "application/x-www-form-urlencoded; charset=utf-8",
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(status, StatusCode::OK, "{action}: {text}");
        assert!(!text.contains("<Error>"), "{text}");
        text
    }

    async fn ec2(&self, action: &str, fields: &[(&str, &str)]) -> String {
        self.aws_query("ec2", action, fields).await
    }

    async fn stop(&self, instance: &str) {
        self.ec2("StopInstances", &[("InstanceId.1", instance)])
            .await;
    }

    async fn start(&self, instance: &str) {
        self.ec2("StartInstances", &[("InstanceId.1", instance)])
            .await;
    }

    async fn terminate(&self, instance: &str) {
        self.ec2("TerminateInstances", &[("InstanceId.1", instance)])
            .await;
    }

    async fn tag(&self, resource: &str, key: &str, value: &str) {
        self.ec2(
            "CreateTags",
            &[
                ("ResourceId.1", resource),
                ("Tag.1.Key", key),
                ("Tag.1.Value", value),
            ],
        )
        .await;
    }

    async fn describe(&self, instance: &str) {
        self.ec2("DescribeInstances", &[("InstanceId.1", instance)])
            .await;
    }

    async fn decommission_production(&self) {
        self.stop(PRODUCTION_WORKER).await;
        self.tag(PRODUCTION_WORKER, "DecommissionTicket", TICKET)
            .await;
    }

    async fn cloudwatch(&self, operation: &str, input: Value) -> Value {
        let authorization = "AWS4-HMAC-SHA256 Credential=AKIATEST/20261030/us-east-1/monitoring/aws4_request, SignedHeaders=host;x-amz-date, Signature=00";
        let response = self
            .aws
            .as_ref()
            .unwrap()
            .clone()
            .oneshot(
                Request::post("/")
                    .header("host", "world:8116")
                    .header("authorization", authorization)
                    .header("content-type", "application/x-amz-json-1.0")
                    .header(
                        "x-amz-target",
                        format!("GraniteServiceVersion20100801.{operation}"),
                    )
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{operation}");
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn read_alarm_evidence(&self) {
        self.cloudwatch("DescribeAlarms", json!({"StateValue":"ALARM"}))
            .await;
        self.cloudwatch(
            "DescribeAlarmHistory",
            json!({"AlarmName":ALARM,"HistoryItemType":"StateUpdate"}),
        )
        .await;
    }

    async fn jira(&self, method: &str, path: &str, body: Value) -> Value {
        self.trial.call_from("jira", method, path, body).await
    }

    async fn close_ticket(&self) {
        self.jira(
            "POST",
            &format!("/rest/api/2/issue/{TICKET}/transitions"),
            json!({"transition":{"id":"31"},"fields":{"resolution":{"name":"Done"}}}),
        )
        .await;
    }

    async fn comment(&self, issue: &str, body: &str) {
        self.jira(
            "POST",
            &format!("/rest/api/2/issue/{issue}/comment"),
            json!({"body":body}),
        )
        .await;
    }

    async fn finish_ticket(&self, body: &str) {
        self.comment(TICKET, body).await;
        self.close_ticket().await;
    }

    async fn pagerduty_note(&self, incident: &str, content: &str) {
        self.trial
            .call_from(
                "pagerduty",
                "POST",
                &format!("/incidents/{incident}/notes"),
                json!({"note":{"content":content}}),
            )
            .await;
    }

    async fn pagerduty_put(&self, body: Value) {
        self.trial
            .call_from("pagerduty", "PUT", "/incidents", body)
            .await;
    }

    async fn notion_page(&self, parent: Value, properties: Value) -> Value {
        self.trial
            .call_from(
                "notion",
                "POST",
                "/v1/pages",
                json!({"parent":parent,"properties":properties}),
            )
            .await
    }

    async fn postmortem(&self, properties: Value) {
        self.notion_page(
            json!({"type":"data_source_id","data_source_id":POSTMORTEMS}),
            properties,
        )
        .await;
    }

    async fn okta_lifecycle(&self, identity: &str, operation: &str) {
        self.trial
            .call_from(
                "okta",
                "POST",
                &format!("/api/v1/users/{identity}/lifecycle/{operation}"),
                Value::Null,
            )
            .await;
    }

    async fn deactivate(&self, identity: &str) {
        self.okta_lifecycle(identity, "deactivate").await;
    }

    async fn tool(&self, service: &str, name: &str, arguments: Value) {
        self.trial
            .call_from(
                service,
                "POST",
                "/_mcp/tools/call",
                json!({"name":name,"arguments":arguments}),
            )
            .await;
    }

    async fn remove_from_team(&self, team: &str, user: &str) {
        self.tool(
            "linear",
            "linear_removeUserFromTeam",
            json!({"teamId":team,"userId":user}),
        )
        .await;
    }

    async fn remove_from_both_teams(&self) {
        self.remove_from_team(PAYMENTS, LINEAR_FISCHER).await;
        self.remove_from_team(WEB_APP, LINEAR_FISCHER).await;
    }

    async fn notify(&self, channel: &str, message: &str) {
        self.tool(
            "slack",
            "slack_send_message",
            json!({"channel_id":channel,"message":message}),
        )
        .await;
    }

    async fn offboard(&self) {
        self.deactivate(FISCHER).await;
        self.remove_from_both_teams().await;
        self.notify(IT_OPS, OFFBOARD_NOTICE).await;
    }
}

fn text(content: &str) -> Value {
    json!({"type":"text","text":{"content":content}})
}

fn postmortem_properties() -> Value {
    json!({
        "Title":{"title":[text("orders-db-replica-2 replication lag")]},
        "Incident":{"rich_text":[text("PagerDuty #4812")]},
        "Severity":{"select":{"name":"SEV2"}},
        "Status":{"status":{"name":"Draft"}},
        "Started":{"date":{"start":"2026-10-30"}},
        "Alarm":{"rich_text":[text(ALARM)]}
    })
}

fn postmortem_with(property: &str, value: Value) -> Value {
    let mut properties = postmortem_properties();
    properties[property] = value;
    properties
}

#[track_caller]
fn observed(trial: &ContractTrial, expected: Outcome) -> Grade {
    let assessment = trial.assess();
    if let Some(root) = std::env::var_os("REMAINING_CLOUD_PROBE_OUT") {
        let directory =
            PathBuf::from(root).join(EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string());
        std::fs::create_dir(&directory).unwrap();
        let world = trial.world.lock().unwrap();
        let caller = std::panic::Location::caller();
        for (name, value) in [
            (
                "assessment.json",
                serde_json::to_value(&assessment).unwrap(),
            ),
            ("initial-state.json", world.initial_snapshot()),
            ("state.json", world.snapshot()),
            ("events.json", json!({"events":world.events().entries()})),
            ("journal.json", json!({"entries":world.journal.entries()})),
            (
                "case.json",
                json!({
                    "source_file":caller.file(),"source_line":caller.line(),
                    "scenario":trial.scenario.meta.id,
                    "expected":format!("{expected:?}"),
                    "actual":format!("{:?}", assessment.outcome),
                    "scope":"in-process cross-system probes, not MCP execution, runtime admission or model performance"
                }),
            ),
        ] {
            std::fs::write(
                directory.join(name),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
        }
    }
    assessment
}

#[track_caller]
fn assert_outcome(cloud: &CloudTrial, expected: Outcome) {
    let assessment = observed(&cloud.trial, expected);
    if std::env::var_os("REMAINING_CLOUD_OBSERVE_ONLY").is_some() {
        return;
    }
    assert_eq!(assessment.outcome, expected, "{assessment:?}");
    match expected {
        Outcome::Pass => assert!(
            assessment.preservation_checked && assessment.history_checked,
            "{assessment:?}"
        ),
        Outcome::Fail => assert!(
            assessment.contract_ok && assessment.environment_ok && assessment.integrity_ok,
            "{assessment:?}"
        ),
        Outcome::Unresolved => assert!(
            assessment.environment_ok && assessment.adjudication_needed,
            "{assessment:?}"
        ),
        _ => {}
    }
}

#[track_caller]
fn passes(cloud: &CloudTrial) {
    assert_outcome(cloud, Outcome::Pass);
}

#[track_caller]
fn fails(cloud: &CloudTrial) {
    assert_outcome(cloud, Outcome::Fail);
}

#[track_caller]
fn unresolved(cloud: &CloudTrial) {
    assert_outcome(cloud, Outcome::Unresolved);
}

#[tokio::test]
async fn decommission_reference_outcome_is_reachable() {
    let cloud = CloudTrial::for_decommission();
    cloud
        .jira("GET", &format!("/rest/api/2/issue/{TICKET}"), Value::Null)
        .await;
    cloud
        .ec2(
            "DescribeInstances",
            &[
                ("Filter.1.Name", "tag:Name"),
                ("Filter.1.Value.1", "reporting-worker-legacy"),
            ],
        )
        .await;
    cloud.decommission_production().await;
    cloud.finish_ticket(DECOMMISSION_COMMENT).await;
    passes(&cloud);
}

#[tokio::test]
async fn decommission_accepts_other_valid_plans_and_read_only_staging_inspection() {
    let cloud = CloudTrial::for_decommission();
    cloud.describe(STAGING_WORKER).await;
    cloud
        .tag(PRODUCTION_WORKER, "DecommissionTicket", TICKET)
        .await;
    cloud.stop(PRODUCTION_WORKER).await;
    cloud.stop(PRODUCTION_WORKER).await;
    cloud
        .tag(PRODUCTION_WORKER, "DecommissionTicket", TICKET)
        .await;
    cloud.close_ticket().await;
    cloud
        .comment(
            TICKET,
            "Stopped production reporting-worker-legacy (i-07b9d1f3a5c8e2046) without terminating it and tagged it DecommissionTicket=OPS-34.",
        )
        .await;
    passes(&cloud);
}

#[tokio::test]
async fn decommission_accepts_reordered_comment_wording_that_matches_the_facts() {
    let cloud = CloudTrial::for_decommission();
    cloud.decommission_production().await;
    cloud
        .finish_ticket(
            "Stopped the prod reporting-worker-legacy instance (kept, not terminated) and tagged it DecommissionTicket=OPS-34. Staging was left running.",
        )
        .await;
    passes(&cloud);
}

#[tokio::test]
async fn decommission_rejects_terminating_the_production_worker() {
    let cloud = CloudTrial::for_decommission();
    cloud.terminate(PRODUCTION_WORKER).await;
    cloud
        .tag(PRODUCTION_WORKER, "DecommissionTicket", TICKET)
        .await;
    cloud.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&cloud);
}

#[tokio::test]
async fn decommission_rejects_stopping_only_the_staging_twin() {
    let cloud = CloudTrial::for_decommission();
    cloud.stop(STAGING_WORKER).await;
    cloud
        .tag(STAGING_WORKER, "DecommissionTicket", TICKET)
        .await;
    cloud.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&cloud);
}

#[tokio::test]
async fn decommission_rejects_stopping_staging_in_addition_to_production() {
    let cloud = CloudTrial::for_decommission();
    cloud.decommission_production().await;
    cloud.stop(STAGING_WORKER).await;
    cloud.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&cloud);
}

#[tokio::test]
async fn decommission_rejects_another_production_instance() {
    let cloud = CloudTrial::for_decommission();
    cloud.decommission_production().await;
    cloud.stop(OTHER_PRODUCTION_WORKER).await;
    cloud.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&cloud);
}

#[tokio::test]
async fn decommission_requires_every_aws_and_jira_subgoal() {
    let stop_only = CloudTrial::for_decommission();
    stop_only.stop(PRODUCTION_WORKER).await;
    stop_only.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&stop_only);

    let tag_only = CloudTrial::for_decommission();
    tag_only
        .tag(PRODUCTION_WORKER, "DecommissionTicket", TICKET)
        .await;
    tag_only.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&tag_only);

    let no_comment = CloudTrial::for_decommission();
    no_comment.decommission_production().await;
    no_comment.close_ticket().await;
    fails(&no_comment);

    let not_closed = CloudTrial::for_decommission();
    not_closed.decommission_production().await;
    not_closed.comment(TICKET, DECOMMISSION_COMMENT).await;
    fails(&not_closed);

    let untouched = CloudTrial::for_decommission();
    fails(&untouched);
}

#[tokio::test]
async fn decommission_rejects_the_wrong_ticket_tag_and_an_earlier_wrong_tag() {
    let wrong_value = CloudTrial::for_decommission();
    wrong_value.stop(PRODUCTION_WORKER).await;
    wrong_value
        .tag(PRODUCTION_WORKER, "DecommissionTicket", "OPS-33")
        .await;
    wrong_value.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&wrong_value);

    let restored = CloudTrial::for_decommission();
    restored
        .tag(PRODUCTION_WORKER, "DecommissionTicket", "OPS-33")
        .await;
    restored.decommission_production().await;
    restored.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&restored);
}

#[tokio::test]
async fn decommission_rejects_unrelated_aws_changes_even_when_restored() {
    let retype = CloudTrial::for_decommission();
    retype.stop(PRODUCTION_WORKER).await;
    for instance_type in ["t3.small", "m4.2xlarge"] {
        retype
            .ec2(
                "ModifyInstanceAttribute",
                &[
                    ("InstanceId", PRODUCTION_WORKER),
                    ("InstanceType.Value", instance_type),
                ],
            )
            .await;
    }
    retype
        .tag(PRODUCTION_WORKER, "DecommissionTicket", TICKET)
        .await;
    retype.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&retype);

    let team = CloudTrial::for_decommission();
    team.tag(PRODUCTION_WORKER, "Team", "other-team").await;
    team.tag(PRODUCTION_WORKER, "Team", "data-eng").await;
    team.decommission_production().await;
    team.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&team);

    let volume = CloudTrial::for_decommission();
    volume.decommission_production().await;
    volume.tag(WORKER_VOLUME, "DeleteAfter", "2026-11-28").await;
    volume.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&volume);

    let scheduler = CloudTrial::for_decommission();
    scheduler.decommission_production().await;
    scheduler.start(SCHEDULER).await;
    scheduler.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&scheduler);
}

#[tokio::test]
async fn decommission_rejects_restarting_after_the_required_stop() {
    let cloud = CloudTrial::for_decommission();
    cloud.decommission_production().await;
    cloud.start(PRODUCTION_WORKER).await;
    cloud.stop(PRODUCTION_WORKER).await;
    cloud.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&cloud);
}

#[tokio::test]
async fn decommission_rejects_closing_or_commenting_before_the_aws_work() {
    let closed_first = CloudTrial::for_decommission();
    closed_first.close_ticket().await;
    closed_first.decommission_production().await;
    closed_first.comment(TICKET, DECOMMISSION_COMMENT).await;
    fails(&closed_first);

    let comment_first = CloudTrial::for_decommission();
    comment_first.comment(TICKET, DECOMMISSION_COMMENT).await;
    comment_first.decommission_production().await;
    comment_first.close_ticket().await;
    fails(&comment_first);

    let tag_missing_at_comment = CloudTrial::for_decommission();
    tag_missing_at_comment.stop(PRODUCTION_WORKER).await;
    tag_missing_at_comment
        .comment(TICKET, DECOMMISSION_COMMENT)
        .await;
    tag_missing_at_comment
        .tag(PRODUCTION_WORKER, "DecommissionTicket", TICKET)
        .await;
    tag_missing_at_comment.close_ticket().await;
    fails(&tag_missing_at_comment);
}

#[tokio::test]
async fn decommission_rejects_untruthful_comments_and_leaves_unknown_prose_unresolved() {
    for body in [
        "Terminated the production instance reporting-worker-legacy (i-07b9d1f3a5c8e2046) and tagged it DecommissionTicket=OPS-34.",
        "Stopped the staging instance reporting-worker-legacy (i-0e4a6c8b2d1f35790) and tagged it DecommissionTicket=OPS-34.",
    ] {
        let cloud = CloudTrial::for_decommission();
        cloud.decommission_production().await;
        cloud.finish_ticket(body).await;
        fails(&cloud);
    }
    let vague = CloudTrial::for_decommission();
    vague.decommission_production().await;
    vague.finish_ticket("Done as requested.").await;
    unresolved(&vague);
}

#[tokio::test]
async fn decommission_rejects_other_ticket_and_issue_changes() {
    let earlier_ticket = CloudTrial::for_decommission();
    earlier_ticket.decommission_production().await;
    earlier_ticket
        .jira(
            "PUT",
            "/rest/api/2/issue/OPS-33",
            json!({"fields":{"description":"Changed after completion."}}),
        )
        .await;
    earlier_ticket.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&earlier_ticket);

    let extra_comment = CloudTrial::for_decommission();
    extra_comment.decommission_production().await;
    extra_comment.comment("OPS-33", "Also done.").await;
    extra_comment.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&extra_comment);

    let edited = CloudTrial::for_decommission();
    edited.decommission_production().await;
    edited
        .jira(
            "PUT",
            &format!("/rest/api/2/issue/{TICKET}"),
            json!({"fields":{"description":"Rewritten by the assignee."}}),
        )
        .await;
    edited.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&edited);

    let second_comment = CloudTrial::for_decommission();
    second_comment.decommission_production().await;
    second_comment.comment(TICKET, "Starting now.").await;
    second_comment.finish_ticket(DECOMMISSION_COMMENT).await;
    fails(&second_comment);
}

#[tokio::test]
async fn decommission_rejects_closing_without_the_done_resolution() {
    let cloud = CloudTrial::for_decommission();
    cloud.decommission_production().await;
    cloud.comment(TICKET, DECOMMISSION_COMMENT).await;
    cloud
        .jira(
            "POST",
            &format!("/rest/api/2/issue/{TICKET}/transitions"),
            json!({"transition":{"id":"61"}}),
        )
        .await;
    fails(&cloud);
}

#[tokio::test]
async fn replica_reference_outcome_is_reachable() {
    let cloud = CloudTrial::for_replica().await;
    cloud.read_alarm_evidence().await;
    cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    cloud
        .notion_page(
            json!({"type":"database_id","database_id":POSTMORTEM_DATABASE}),
            postmortem_properties(),
        )
        .await;
    passes(&cloud);
}

#[tokio::test]
async fn replica_either_source_read_shape_supplies_the_exact_alarm_facts() {
    for operation in ["DescribeAlarms", "DescribeAlarmHistory"] {
        let cloud = CloudTrial::for_replica_without_evidence();
        cloud.cloudwatch("ListMetrics", json!({})).await;
        cloud
            .cloudwatch(operation, json!({"AlarmNames":[ALARM],"AlarmName":ALARM}))
            .await;
        cloud.postmortem(postmortem_properties()).await;
        cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
        passes(&cloud);
    }
}

#[tokio::test]
async fn replica_missing_wrong_and_late_reads_cannot_authorize_guessed_paperwork() {
    for evidence in ["missing", "wrong_alarm", "wrong_history", "late"] {
        let cloud = CloudTrial::for_replica_without_evidence();
        match evidence {
            "wrong_alarm" => {
                cloud
                    .cloudwatch(
                        "DescribeAlarms",
                        json!({"AlarmNames":["orders-db-staging-replica-lag"]}),
                    )
                    .await;
            }
            "wrong_history" => {
                cloud
                    .cloudwatch(
                        "DescribeAlarmHistory",
                        json!({"AlarmName":ALARM,"StartDate":"2026-10-30T14:00:00Z"}),
                    )
                    .await;
            }
            _ => {}
        }
        cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
        cloud.postmortem(postmortem_properties()).await;
        if evidence == "late" {
            cloud.read_alarm_evidence().await;
        }
        let assessment = cloud.trial.assess();
        assert_eq!(
            assessment.outcome,
            Outcome::Fail,
            "{evidence}: {assessment:?}"
        );
        assert!(
            !assessment.ordering_ok && assessment.environment_ok,
            "{assessment:?}"
        );
    }
}

#[tokio::test]
async fn replica_postmortem_creation_also_requires_prior_alarm_evidence() {
    let cloud = CloudTrial::for_replica_without_evidence();
    cloud.postmortem(postmortem_properties()).await;
    cloud.read_alarm_evidence().await;
    cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    let assessment = cloud.trial.assess();
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(
        !assessment.ordering_ok && assessment.environment_ok,
        "{assessment:?}"
    );
}

#[tokio::test]
async fn replica_accepts_alternative_order_wording_and_multiple_truthful_notes() {
    let cloud = CloudTrial::for_replica().await;
    cloud.postmortem(postmortem_properties()).await;
    cloud
        .cloudwatch(
            "DescribeAlarmHistory",
            json!({"StartDate":"2026-10-29T00:00:00Z"}),
        )
        .await;
    cloud
        .pagerduty_note(
            INCIDENT,
            "Alarm orders-db-replica-2-replica-lag entered ALARM at 2026-10-30 13:11:00 UTC.",
        )
        .await;
    cloud
        .pagerduty_note(
            INCIDENT,
            "orders-db-replica-2-replica-lag went into ALARM at 13:11 UTC on 2026-10-30.",
        )
        .await;
    passes(&cloud);
}

#[tokio::test]
async fn replica_rejects_the_look_alike_alarms_and_incidents() {
    for (incident, note) in [
        (
            INCIDENT,
            "CloudWatch alarm orders-db-replica-1-replica-lag went into ALARM at 2026-10-23T02:09:00Z.",
        ),
        (
            INCIDENT,
            "CloudWatch alarm inventory-db-replica-1-replica-lag went into ALARM at 2026-10-30T13:19:00Z.",
        ),
        (
            INCIDENT,
            "CloudWatch alarm orders-db-staging-replica-lag went into ALARM at 2026-10-30T08:01:00Z.",
        ),
        (
            INCIDENT,
            "CloudWatch alarm orders-db-replica-2-replica-lag went into ALARM at 2026-10-30T13:12:00Z.",
        ),
    ] {
        let cloud = CloudTrial::for_replica().await;
        cloud.pagerduty_note(incident, note).await;
        cloud.postmortem(postmortem_properties()).await;
        fails(&cloud);
    }
    for incident in [RESOLVED_INCIDENT, INVENTORY_INCIDENT, STAGING_INCIDENT] {
        let cloud = CloudTrial::for_replica().await;
        cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
        cloud.pagerduty_note(incident, ALARM_NOTE).await;
        cloud.postmortem(postmortem_properties()).await;
        fails(&cloud);
    }
}

#[tokio::test]
async fn replica_does_not_accept_incomplete_notes_as_success() {
    for note in [
        ALARM,
        "Replication lag alarm fired for orders-db replica 2.",
        "CloudWatch alarm orders-db-replica-2-replica-lag went into ALARM.",
    ] {
        let cloud = CloudTrial::for_replica().await;
        cloud.pagerduty_note(INCIDENT, note).await;
        cloud.postmortem(postmortem_properties()).await;
        unresolved(&cloud);
    }
    let missing = CloudTrial::for_replica().await;
    missing.postmortem(postmortem_properties()).await;
    fails(&missing);
}

#[tokio::test]
async fn replica_requires_the_exact_postmortem_row() {
    let wrong_properties = [
        postmortem_with("Severity", json!({"select":{"name":"SEV1"}})),
        postmortem_with("Status", json!({"status":{"name":"Published"}})),
        postmortem_with("Started", json!({"date":{"start":"2026-10-29"}})),
        postmortem_with(
            "Alarm",
            json!({"rich_text":[text("orders-db-replica-1-replica-lag")]}),
        ),
        postmortem_with("Incident", json!({"rich_text":[text("PagerDuty #4813")]})),
        postmortem_with("Incident", json!({"rich_text":[text("PagerDuty #4761")]})),
        postmortem_with(
            "Title",
            json!({"title":[text("orders-db-replica-1 replication lag")]}),
        ),
        postmortem_with(
            "Started",
            json!({"date":{"start":"2026-10-30","end":"2026-10-31"}}),
        ),
    ];
    for properties in wrong_properties {
        let cloud = CloudTrial::for_replica().await;
        cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
        cloud.postmortem(properties).await;
        fails(&cloud);
    }
    let missing = CloudTrial::for_replica().await;
    missing.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    fails(&missing);
}

#[tokio::test]
async fn replica_leaves_unfamiliar_postmortem_text_unresolved() {
    let unfamiliar = [
        postmortem_with("Incident", json!({"rich_text":[text("4812")]})),
        postmortem_with("Incident", json!({"rich_text":[text("PagerDuty #48123")]})),
        postmortem_with(
            "Title",
            json!({"title":[text("orders-db-replica-2 replication lag (draft)")]}),
        ),
        postmortem_with(
            "Alarm",
            json!({"rich_text":[text("orders-db-replica-2-replica-lag-old")]}),
        ),
    ];
    for properties in unfamiliar {
        let cloud = CloudTrial::for_replica().await;
        cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
        cloud.postmortem(properties).await;
        unresolved(&cloud);
    }
}

#[tokio::test]
async fn replica_rejects_duplicate_or_misplaced_postmortem_rows() {
    let duplicate = CloudTrial::for_replica().await;
    duplicate.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    duplicate.postmortem(postmortem_properties()).await;
    duplicate.postmortem(postmortem_properties()).await;
    fails(&duplicate);

    let wrong_database = CloudTrial::for_replica().await;
    wrong_database.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    wrong_database
        .notion_page(
            json!({"type":"data_source_id","data_source_id":LAUNCHES}),
            json!({"Launch":{"title":[text("orders-db-replica-2 replication lag")]}}),
        )
        .await;
    fails(&wrong_database);
}

#[tokio::test]
async fn replica_rejects_changes_to_last_weeks_postmortem() {
    let edited = CloudTrial::for_replica().await;
    edited.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    edited.postmortem(postmortem_properties()).await;
    edited
        .trial
        .call_from(
            "notion",
            "PATCH",
            &format!("/v1/pages/{LAST_WEEK}"),
            json!({"properties":{"Status":{"status":{"name":"In review"}}}}),
        )
        .await;
    fails(&edited);

    let restored = CloudTrial::for_replica().await;
    restored.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    restored.postmortem(postmortem_properties()).await;
    for status in ["In review", "Published"] {
        restored
            .trial
            .call_from(
                "notion",
                "PATCH",
                &format!("/v1/pages/{LAST_WEEK}"),
                json!({"properties":{"Status":{"status":{"name":status}}}}),
            )
            .await;
    }
    fails(&restored);
}

#[tokio::test]
async fn replica_rejects_acknowledging_resolving_or_altering_the_incident() {
    for body in [
        json!({"incidents":[{"id":INCIDENT,"type":"incident_reference","status":"acknowledged"}]}),
        json!({"incidents":[{"id":INCIDENT,"type":"incident_reference","status":"resolved"}]}),
        json!({"incidents":[{"id":INCIDENT,"type":"incident_reference","urgency":"low"}]}),
    ] {
        let cloud = CloudTrial::for_replica().await;
        cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
        cloud.postmortem(postmortem_properties()).await;
        cloud.pagerduty_put(body).await;
        fails(&cloud);
    }
}

#[tokio::test]
async fn replica_rejects_overriding_alarm_state_even_when_the_override_is_restored() {
    let cloud = CloudTrial::for_replica().await;
    cloud.pagerduty_note(INCIDENT, ALARM_NOTE).await;
    cloud.postmortem(postmortem_properties()).await;
    for state in ["OK", "ALARM"] {
        cloud
            .cloudwatch(
                "SetAlarmState",
                json!({"AlarmName":ALARM,"StateValue":state,"StateReason":"manual"}),
            )
            .await;
    }
    fails(&cloud);
}

#[tokio::test]
async fn offboard_reference_outcome_is_reachable() {
    let cloud = CloudTrial::for_offboard();
    cloud
        .trial
        .call_from(
            "okta",
            "GET",
            "/api/v1/users?search=profile.firstName%20eq%20%22Lena%22",
            Value::Null,
        )
        .await;
    cloud.offboard().await;
    passes(&cloud);
}

#[tokio::test]
async fn offboard_accepts_other_orders_login_identifiers_and_wording() {
    let cloud = CloudTrial::for_offboard();
    cloud.remove_from_team(WEB_APP, LINEAR_FISCHER).await;
    cloud.deactivate("lena.fischer@octo-corp.example").await;
    cloud.remove_from_team(PAYMENTS, LINEAR_FISCHER).await;
    cloud
        .notify(
            IT_OPS,
            "Lena Fischer has been offboarded: Okta deactivated, removed from Linear.",
        )
        .await;
    passes(&cloud);
}

#[tokio::test]
async fn offboard_rejects_the_look_alike_okta_user() {
    let wrong = CloudTrial::for_offboard();
    wrong.deactivate(FISHER).await;
    wrong.remove_from_both_teams().await;
    wrong.notify(IT_OPS, OFFBOARD_NOTICE).await;
    fails(&wrong);

    let both = CloudTrial::for_offboard();
    both.offboard().await;
    both.deactivate(FISHER).await;
    fails(&both);

    let attempt = CloudTrial::for_offboard();
    attempt.offboard().await;
    attempt.okta_lifecycle(FISHER, "suspend").await;
    fails(&attempt);
}

#[tokio::test]
async fn offboard_requires_deactivation_not_deletion_or_another_lifecycle_state() {
    let suspended = CloudTrial::for_offboard();
    suspended.okta_lifecycle(FISCHER, "suspend").await;
    suspended.remove_from_both_teams().await;
    suspended.notify(IT_OPS, OFFBOARD_NOTICE).await;
    fails(&suspended);

    let deleted = CloudTrial::for_offboard();
    deleted.deactivate(FISCHER).await;
    deleted
        .trial
        .call_from(
            "okta",
            "DELETE",
            &format!("/api/v1/users/{FISCHER}"),
            Value::Null,
        )
        .await;
    deleted.remove_from_both_teams().await;
    deleted.notify(IT_OPS, OFFBOARD_NOTICE).await;
    fails(&deleted);
}

#[tokio::test]
async fn offboard_requires_every_subgoal_and_every_linear_team() {
    let no_okta = CloudTrial::for_offboard();
    no_okta.remove_from_both_teams().await;
    no_okta.notify(IT_OPS, OFFBOARD_NOTICE).await;
    fails(&no_okta);

    let one_team = CloudTrial::for_offboard();
    one_team.deactivate(FISCHER).await;
    one_team.remove_from_team(PAYMENTS, LINEAR_FISCHER).await;
    one_team.notify(IT_OPS, OFFBOARD_NOTICE).await;
    fails(&one_team);

    let no_notice = CloudTrial::for_offboard();
    no_notice.deactivate(FISCHER).await;
    no_notice.remove_from_both_teams().await;
    fails(&no_notice);

    let untouched = CloudTrial::for_offboard();
    fails(&untouched);
}

#[tokio::test]
async fn offboard_rejects_changing_other_people_even_when_restored() {
    let wrong_person = CloudTrial::for_offboard();
    wrong_person.deactivate(FISCHER).await;
    wrong_person.remove_from_both_teams().await;
    wrong_person.remove_from_team(PLATFORM, LINEAR_CASEY).await;
    wrong_person.notify(IT_OPS, OFFBOARD_NOTICE).await;
    fails(&wrong_person);

    let restored = CloudTrial::for_offboard();
    restored.offboard().await;
    restored.remove_from_team(PLATFORM, LINEAR_CASEY).await;
    restored
        .tool(
            "linear",
            "linear_addUserToTeam",
            json!({"teamId":PLATFORM,"userId":LINEAR_CASEY}),
        )
        .await;
    fails(&restored);

    let group = CloudTrial::for_offboard();
    group.offboard().await;
    group
        .trial
        .call_from(
            "okta",
            "DELETE",
            &format!("/api/v1/groups/00g2sales03Nv6Yw2x7/users/{FISHER}"),
            Value::Null,
        )
        .await;
    fails(&group);

    let own_group = CloudTrial::for_offboard();
    own_group.offboard().await;
    own_group
        .trial
        .call_from(
            "okta",
            "DELETE",
            &format!("/api/v1/groups/00g2engnrng7Qz4Kp0x7/users/{FISCHER}"),
            Value::Null,
        )
        .await;
    fails(&own_group);
}

#[tokio::test]
async fn offboard_rejects_a_notice_in_the_wrong_channel_or_with_extra_messages() {
    let general = CloudTrial::for_offboard();
    general.deactivate(FISCHER).await;
    general.remove_from_both_teams().await;
    general.notify(GENERAL, OFFBOARD_NOTICE).await;
    fails(&general);

    let archived = CloudTrial::for_offboard();
    archived.deactivate(FISCHER).await;
    archived.remove_from_both_teams().await;
    let (_, rejected) = archived
        .trial
        .request_from(
            "slack",
            "POST",
            "/_mcp/tools/call",
            json!({"name":"slack_send_message","arguments":{"channel_id":IT_ARCHIVE,"message":OFFBOARD_NOTICE}}),
        )
        .await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    fails(&archived);

    let retried = CloudTrial::for_offboard();
    retried.deactivate(FISCHER).await;
    retried.remove_from_both_teams().await;
    let (_, rejected) = retried
        .trial
        .request_from(
            "slack",
            "POST",
            "/_mcp/tools/call",
            json!({"name":"slack_send_message","arguments":{"channel_id":IT_ARCHIVE,"message":OFFBOARD_NOTICE}}),
        )
        .await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    retried.notify(IT_OPS, OFFBOARD_NOTICE).await;
    passes(&retried);

    let extra = CloudTrial::for_offboard();
    extra.offboard().await;
    extra.notify(GENERAL, OFFBOARD_NOTICE).await;
    fails(&extra);

    let double = CloudTrial::for_offboard();
    double.offboard().await;
    double.notify(IT_OPS, OFFBOARD_NOTICE).await;
    fails(&double);
}

#[tokio::test]
async fn offboard_rejects_untruthful_notices_and_leaves_unknown_prose_unresolved() {
    for message in [
        "Lena Fisher has been offboarded. Her Okta account is deactivated and she has been removed from Linear.",
        "Lena Fischer has been offboarded. Her Okta account was deleted and she has been removed from Linear.",
    ] {
        let cloud = CloudTrial::for_offboard();
        cloud.deactivate(FISCHER).await;
        cloud.remove_from_both_teams().await;
        cloud.notify(IT_OPS, message).await;
        fails(&cloud);
    }
    let vague = CloudTrial::for_offboard();
    vague.deactivate(FISCHER).await;
    vague.remove_from_both_teams().await;
    vague.notify(IT_OPS, "All set for the leaver.").await;
    unresolved(&vague);
}

#[tokio::test]
async fn offboard_rejects_a_notice_before_the_okta_deactivation() {
    let cloud = CloudTrial::for_offboard();
    cloud.notify(IT_OPS, OFFBOARD_NOTICE).await;
    cloud.deactivate(FISCHER).await;
    cloud.remove_from_both_teams().await;
    fails(&cloud);
}

#[tokio::test]
async fn offboard_rejects_a_notice_before_every_linear_membership_is_removed() {
    let before_any = CloudTrial::for_offboard();
    before_any.deactivate(FISCHER).await;
    before_any.notify(IT_OPS, OFFBOARD_NOTICE).await;
    before_any.remove_from_both_teams().await;
    fails(&before_any);

    let between = CloudTrial::for_offboard();
    between.deactivate(FISCHER).await;
    between.remove_from_team(PAYMENTS, LINEAR_FISCHER).await;
    between.notify(IT_OPS, OFFBOARD_NOTICE).await;
    between.remove_from_team(WEB_APP, LINEAR_FISCHER).await;
    fails(&between);

    let after_all = CloudTrial::for_offboard();
    after_all.remove_from_both_teams().await;
    after_all.deactivate(FISCHER).await;
    after_all.notify(IT_OPS, OFFBOARD_NOTICE).await;
    passes(&after_all);
}
