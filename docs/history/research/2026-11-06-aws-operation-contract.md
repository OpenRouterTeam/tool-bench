# AWS operation contracts (task date: 2026-11-06)

## Scope and evidence

This note researches the unchanged instructions in
`scenarios/cloud/aws/grant-auditor-access/instruction.md`,
`scenarios/cloud/aws/orders-api-cpu-alarm/instruction.md`, and
`scenarios/cloud/aws/stop-legacy-reporting-worker/instruction.md`.
Only this note is written. No AWS account, infrastructure, Docker, dependency
installation, benchmark, test execution, commit, or child agent was used.

Evidence labels:

- **Documented:** an official AWS API reference or developer/user guide states it.
- **Pinned model/client:** a versioned first-party SDK model or implementation
  establishes the request shape or client behavior, not necessarily server behavior.
- **Inference:** a proposed interpretation or benchmark implication, not an AWS
  guarantee.
- **Uncertain:** the examined primary sources do not establish the claimed detail.
  Absence of documentation is not proof that production behaves oppositely.

**Local version facts:** `servers/python/aws-api.txt`,
`servers/python/aws-cloudwatch.txt`, and `servers/python/aws-iam.txt` pin both
`boto3` and `botocore` to **1.43.123**. The AWS API wrapper also pins
`awscli==1.46.0`. These are repository pins, not measurements of a running image.
The upstream botocore **1.43.123** models were fetched directly and inspected;
the model API dates are IAM `2010-05-08`, CloudWatch `2010-08-01`, and EC2
`2016-11-15`. [M-IAM][M-CW][M-EC2]

SHA-256 of the fetched `service-2.json` bytes:

| Model | SHA-256 |
| --- | --- |
| IAM | `15c70eaeb15bda321fbf0e9a0db35036d67c6ccd34d38acbcad3a086b89251c5` |
| CloudWatch | `a69261344acec58229068d836356e2484c4dd33a25751629ce2268c399e6bbe4` |
| EC2 | `6f02f8af7732f2e6169552117fe844df7f1a88a4336de87f83ecce73c5aeba00` |

AWS CDK **v2.270.0** is additional first-party implementation evidence for the
single-`MetricStat` alarm representation. It is not a benchmark dependency pin.
Current AWS documentation and that CDK implementation are identified separately
from botocore's pinned model. [CDK]

## 1. IAM: policy identity, membership, retries, and names

### Requested relationships, not merely equivalent permissions

**Documented:** `AttachUserPolicy` attaches a managed policy identified by
`PolicyArn` directly to a user identified by friendly `UserName`.
`PutUserPolicy` instead embeds an inline policy. AWS identifies the AWS-managed
SecurityAudit policy as `arn:aws:iam::aws:policy/SecurityAudit`.
AWS-managed policies are separate identities maintained by AWS; copied customer
policies and inline documents are not that managed-policy identity.
[I-ATTACH][I-SECURITY][I-POLICIES]

**Task interpretation:** success requires that direct SecurityAudit attachment
on the external auditor's user **and** membership in `auditors`.
Attaching a same-named customer-managed policy, embedding equivalent permissions,
or granting permissions only through a group does not establish both requested
relationships. Group permissions may grant access, but do not constitute a direct
user-policy attachment. Do not identify either Priya Raman solely by display name,
and do not broaden the change to the employee or other principals.
[I-ATTACH][I-GROUPS]

### Idempotence and ordering: narrower evidence than a blanket guarantee

**Pinned model/client:** `AttachUserPolicyRequest` requires `UserName` and
`PolicyArn`; `AddUserToGroupRequest` requires `GroupName` and `UserName`.
Neither has a client-token field or modeled operation `idempotent` trait.
Neither operation's modeled errors include `EntityAlreadyExistsException`.
Those are model facts, **not proof** that duplicate requests always succeed.
[M-IAM]

**Uncertain:** the examined `AttachUserPolicy` and `AddUserToGroup` API references
do not explicitly promise success/no-op responses for an already attached policy
or an already present member. A blanket production retry guarantee is therefore
not established here. This does not establish non-idempotence either.
[I-ATTACH][I-ADD][M-IAM]

**Inference for the mock/grader:** represent these as relationships, not an
append-only multiset. A successful repeat should not create a second logical
membership or attachment, and a harmless repeated attempt is not itself an
unauthorized permission change. This state-level idempotence recommendation is
distinct from claiming a documented AWS HTTP/error contract for every retry.
[I-ATTACH][I-ADD][I-LIST-ATTACHED][I-LIST-GROUPS]

**Documented:** IAM changes are eventually consistent; creating or changing users,
groups, policies, and tags can take time to propagate. The attachment and
membership list APIs are paginated, and IAM may return fewer than `MaxItems`
even when more items remain. [I-CONSISTENCY][I-LIST-ATTACHED][I-LIST-GROUPS]

**Uncertain/inference:** the examined APIs impose no attach-before-add or
add-before-attach prerequisite, promise no atomic transaction spanning both
operations, and specify no insertion-order guarantee for the returned relationship
lists. Either operation order can establish the two requested relationships;
compare membership/attachment sets by identity rather than array position.
Do not claim immediate read-after-write visibility or universal response sorting.
[I-ATTACH][I-ADD][I-LIST-ATTACHED][I-LIST-GROUPS][I-CONSISTENCY]

### Case-insensitive uniqueness is not a mutation-lookup specification

**Documented:** `CreateUser` says IAM user, group, role, and policy names are not
distinguished by case for name uniqueness. The identifiers guide separately says
an IAM user name is case-insensitive for the user, such as during sign-in, but
case-sensitive in a policy or ARN. [I-CREATE][I-IDENTIFIERS]

**Unsupported inference:** those statements alone do **not** specify that every
API's `UserName` or `GroupName` lookup/mutation is case-insensitive.
`AttachUserPolicy`, `AddUserToGroup`, and their pinned shapes accept mixed-case
strings but do not explicitly define their comparison/canonicalization rules.
Do not turn a creation uniqueness rule or sign-in rule into an asserted universal
case-folded lookup contract. Preserve discovered canonical names/ARNs; a regression
depending on alternate-case mutation needs more operation-specific evidence or an
explicitly labeled mock policy. [I-ATTACH][I-ADD][I-CREATE][I-IDENTIFIERS][M-IAM]

## 2. CloudWatch: the requested CPU condition

### Defaults and significant fields

**Documented:** `ActionsEnabled` defaults to `true`. Explicit `false` suppresses
actions on state changes, so merely storing the right SNS ARN with disabled
actions does not establish the requested notification configuration.
[C-PUT]

**Documented:** `EvaluationPeriods` is N and `DatapointsToAlarm` is M.
CloudFormation's alarm reference explicitly says omitting `DatapointsToAlarm`
makes CloudWatch use `EvaluationPeriods`. Thus N=3 with M omitted or M=3 expresses
the usual three-consecutive-period configuration; M=2/N=3 is different.
For M-of-N generally, breaches need not be consecutive. [C-CFN][C-EVALUATION]

**Documented qualification:** the normal sliding-window evaluator can retrieve
older datapoints beyond N and use them when newer datapoints are missing.
Missing-data handling and premature-alarm avoidance have additional rules.
Therefore even M=N must not be described as an unconditional promise of three
fresh, contiguous wall-clock observations under every missing-data condition.
Omitted `TreatMissingData` defaults to `missing`; the user did not choose a
different missing-data policy. [C-MISSING][C-PUT]

**Documented:** EC2 `CPUUtilization` uses `Percent`, and `InstanceId` selects the
individual instance. A five-minute metric period is **300 seconds**; three
one-minute averages are not three five-minute averages. The requested statistic
is `Average`, comparison is `GreaterThanThreshold`, and threshold is numerically
80, not 80.5 or greater-than-or-equal-to 80. These values derive from the wording,
not from an alarm-name heuristic. [E-METRICS][C-PUT][C-STAT]

**Documented:** an omitted top-level `Unit` is valid: CloudWatch retrieves the
published unit types. An explicitly wrong unpublished unit can leave the alarm
in `INSUFFICIENT_DATA`. Omitting `Unit` is recommended to avoid accidentally
selecting the wrong one; multiple published units without a selected unit have
undefined alarm behavior. `MetricStat.Unit` also filters the returned data,
and CloudWatch does not convert units. Thus omitted Unit and `Percent` are not
equivalent to explicit `Count` for this EC2 metric. [C-PUT][C-STAT][E-METRICS]

### Two supported representations

These are illustrative request fragments, **not discovered resource identities**.
Replace the instance ID, account, Region, and SNS ARN with the intended resources.
Both carry the same alarm-level configuration:

```json
{
  "AlarmName": "orders-api-high-cpu",
  "ComparisonOperator": "GreaterThanThreshold",
  "Threshold": 80,
  "EvaluationPeriods": 3,
  "AlarmActions": ["arn:aws:sns:us-east-1:123456789012:ops-pager"]
}
```

**Documented direct-metric form:** merge that configuration with:
[C-PUT][E-METRICS]

```json
{
  "Namespace": "AWS/EC2",
  "MetricName": "CPUUtilization",
  "Dimensions": [{"Name": "InstanceId", "Value": "i-0123456789abcdef0"}],
  "Period": 300,
  "Statistic": "Average"
}
```

**First-party implementation evidence:** a single metric query is also supported;
it need not contain a math `Expression`. CDK's versioned alarm renderer emits
exactly one `MetricStat` query with `ReturnData: true` for a metric needing a
label or account qualification. The API/model define the underlying fields.
Merge the common configuration with: [CDK][C-QUERY][C-STAT][M-CW]

```json
{
  "Metrics": [{
    "Id": "cpu",
    "ReturnData": true,
    "MetricStat": {
      "Metric": {
        "Namespace": "AWS/EC2",
        "MetricName": "CPUUtilization",
        "Dimensions": [{"Name": "InstanceId", "Value": "i-0123456789abcdef0"}]
      },
      "Period": 300,
      "Stat": "Average"
    }
  }]
}
```

**Documented/model facts:** when using `Metrics`, do not also send top-level
`Namespace`, `MetricName`, `Dimensions`, `Period`, `Unit`, `Statistic`, or
`ExtendedStatistic`. A query uses `Expression` **or** `MetricStat`, not both.
The pinned `MetricStat` requires `Metric`, `Period`, and `Stat`; an ordinary
direct metric instead uses top-level `Statistic` or `ExtendedStatistic`, not
both. Do not confuse the two spellings or require top-level metric fields for
the query representation. [C-PUT][C-QUERY][C-STAT][M-CW]

**Pinned-client boundary:** the model's top-level `PutMetricAlarmInput` requires
only `AlarmName`, and `MetricDataQuery` requires only `Id`; that is not a complete
service-validity specification. Botocore's generic validator checks modeled
required/known members, types, and ranges; its string validator does not evaluate
metric-math semantics. SDK-shaped serialization or mock acceptance therefore
proves neither a well-formed query graph nor the requested CPU predicate.
[M-CW][M-VALIDATOR]

**Scope distinction:** query `AccountId` identifies the account containing the
metric; cross-account alarms have documented prerequisites. A syntactically valid
query pointing to another account or instance need not be API-invalid, but does
not establish monitoring of the requested production orders-api instance.
Do not silently discard account identity when normalizing a query. [C-QUERY][C-PUT]

### ReturnData and identity aliases

**Documented:** for ordinary metric-math static-threshold alarms, select one
watched result with `ReturnData: true` and set other inputs/results to `false`.
An expression must return a single time series. Query IDs are variables usable
by other expressions. These are not universal rules for every alarm type:
anomaly-detection configurations have special handling outside this task.
[C-QUERY][CDK]

**Important default boundary:** the API/model explicitly document an omitted
`ReturnData` default of `true` in the **GetMetricData** paragraph. The subsequent
**PutMetricAlarm** paragraph instructs callers to set true/false explicitly;
it does not separately spell out an alarm-specific omission default.
The pinned shape makes the field optional but supplies no modeled default.
Do not misquote the Get API's default as an independently established Put API
guarantee, or infer that omission means false. Ambiguous omitted-output selection
needs adjudication rather than an invented environment or agent failure.
[C-QUERY][M-CW]

**Inference directly from expression-variable semantics:** changing the input
query to `"ReturnData": false` and adding
`{"Id":"condition","Expression":"cpu","ReturnData":true}` is an identity alias:
the watched series is the same `cpu` series, not a different metric.
Recognizing that alias is different from assuming arbitrary arithmetic rewrites
preserve missing datapoints. [C-QUERY][C-MATH]

### IF expressions: valid syntax does not prove universal equivalence

**Documented:** metric math supports comparisons and
`IF(condition, trueValue, falseValue)`, including scalar branches.
AWS itself illustrates `IF(latency>threshold, 1, 0)`.
Therefore `IF(cpu > 80, 100, 0)` is not intrinsically an invalid alarm expression.
[C-MATH]

**Limited inference:** with actual CPU datapoints, the same 300-second Average
input, and an outer `GreaterThanThreshold` comparison against 80, that expression
produces 100 exactly when CPU exceeds 80 and 0 otherwise. Its pointwise breach
predicate then matches the direct alarm. That statement does **not** establish
equivalent behavior for missing/late data, different periods, different outer
thresholds, or a differently selected result. AWS documents zero/missing-value
effects for comparisons/IF and a separate alarm missing-data evaluator.
[C-MATH][C-MISSING]

**Benchmark implication:** accept proven direct, single-`MetricStat`, and identity
alias representations. If raw `Metrics` are stored but their expressions cannot
be semantically established, report **unresolved / adjudication needed**.
Do not declare a known supported request a model failure, and do not automatically
pass every expression merely because it references CPU. A behaviorally established
non-equivalent expression can fail the task; an unevaluated one is not proof of
non-equivalence. [C-QUERY][C-MATH][CDK]

## 3. CloudWatch identity, updates, history, and reads

**Documented:** alarm names must be unique within a Region; alarm ARNs include
Region and account, for example
`arn:aws:cloudwatch:us-west-2:123456789012:alarm:...`.
**Inference:** the resource key must include account and Region as well as name;
a global name-only map lets one regional alarm incorrectly overwrite another.
A query's metric `AccountId` is not a replacement for the owning alarm's account.
[C-PUT][C-CFN][C-QUERY]

**SNS action identity and regional evidence boundary:** an SNS action is an ARN,
not just a topic name. SNS `Publish` documents publication only to topics and
endpoints in the same AWS Region. That constrains the SNS publication endpoint
and target; **by itself it does not prove which regional endpoint CloudWatch
uses for a cross-Region action ARN**. The examined CloudWatch API/model do not
explicitly establish cross-Region SNS action support, a universal rejection rule,
or the exact synchronous error for such an ARN. Treat direct cross-Region action
behavior as **unresolved here**, not as a fabricated `PutMetricAlarm` validation
guarantee. Reusing the actual existing `ops-pager` ARN avoids substituting a
different regional topic with the same name. Do not conflate cross-account metric
support with cross-Region SNS action support. [C-PUT][S-PUBLISH][C-QUERY]

**Documented create/update behavior:** for the standard metric alarms at issue,
creation initially sets state to `INSUFFICIENT_DATA` and then evaluation determines
state. Updating an existing alarm leaves its state unchanged at update time but
completely overwrites its previous configuration. That is not a merge-patch
contract, nor a promise that subsequent evaluations never change state.
Tags supplied to `PutMetricAlarm` are applied on creation but ignored on update;
use `TagResource`/`UntagResource` to change existing tags. [C-PUT]

**Inference about defaults on replacement:** do not preserve arbitrary omitted
old configuration fields simply because the operation is an update. Apply
documented defaults such as `ActionsEnabled=true` and `TreatMissingData=missing`
consistently with configuration replacement. Do not generalize this into an
invented default for every optional field or into deleting tags on an update.
The task asks to configure an alarm, not force it into ALARM or prove a real
notification has already been delivered. [C-PUT]

**Documented history:** CloudWatch exposes `ConfigurationUpdate`, `StateUpdate`,
and `Action` history types, among others. `DescribeAlarmHistory` supports
date/type filtering and timestamp ordering, and history survives deletion.
The user guide states a 30-day retention period. `HistoryData` is described as
JSON, with a separate textual summary and timestamp. [C-HISTORY][C-HISTORY-ITEM][C-ALARMS]

**Not established:** these references do not specify a stable nested
`HistoryData` schema for every configuration API, one record per successful
no-op/retry, strict timestamp monotonicity, or exactly which fields must change
for `EnableAlarmActions`/`DisableAlarmActions`. The model describes
`AlarmConfigurationUpdatedTimestamp` as the last configuration update, not a
complete mutation ledger. Unchanged timestamps or a restored final configuration
cannot establish that an existing alarm was untouched.
**Benchmark inference:** use committed operation/effect history to enforce the
user's explicit preservation constraint; do not invent AWS history payloads as
the justification. [C-HISTORY][C-HISTORY-ITEM][C-ALARM][M-CW]

**Documented read versus mutation:** `DescribeAlarms(StateValue="ALARM")` filters
results to alarms currently in that state; it does not set state.
`SetAlarmState` is a mutation for testing: a state change can invoke the action
for that state, and metric alarms generally return to evaluated state quickly.
Read-only StateValue filters must not be treated as `SetAlarmState`, and a manual
state flip is not evidence that the requested threshold configuration is correct.
[C-DESCRIBE][C-SET]

## 4. EC2 StopInstances: asynchronous completion and preservation

**Documented:** `StopInstances` stops EBS-backed instances, and its sample response
returns previous `running` and current `stopping` state. The lifecycle is
`running -> stopping -> stopped`; a successful request is not itself proof that
shutdown has completed. The pinned `InstanceStopped` waiter polls
`DescribeInstances` for all described instance states to equal `stopped`
(default 15-second delay, 40 attempts). [E-STOP][E-LIFECYCLE][M-WAITER]

**Mock implication:** immediately storing `stopped` while returning `stopping`
collapses a documented transition. If intentionally using deterministic immediate
completion, label that simplification; do not claim the response demonstrates
eventual completion. The task's completed outcome is a stopped target, whereas
`stopping` is in progress. Do not treat an asynchronous response as an agent
failure before the modeled completion opportunity. [E-STOP][E-LIFECYCLE][M-WAITER]

**Documented disk behavior:** stopping retains the EBS root and data volumes;
it does not terminate the instance. Instance-store data is erased on stop, and
instance-store-root instances cannot be stopped. `DeleteOnTermination` controls
termination behavior, not whether an ordinary stop deletes an EBS volume.
Do not generalize simplified API prose about termination into a claim that every
attached EBS data volume is always deleted on termination. [E-STOP][E-START][E-LIFECYCLE]

**Task interpretation:** keep the target's existing EBS disk/data and its
attachment relationship; a stop is not authorization to detach, replace, delete,
or independently retag the disk. Do not demand byte-for-byte equality of every
incidental provider metadata field as proof of preservation. "For another month"
explains retaining the disk, not authorization to schedule deletion or add
`DeleteAfter`, snapshot, disk-tag, or lifecycle-policy requirements.
[E-STOP][E-LIFECYCLE]

**Documented networking:** private IPv4 addresses remain associated through
stop/start. An associated Elastic IP remains associated. An automatically assigned
public IPv4 is released on stop and a new one is normally assigned on start
(AWS documents exceptions to reassignment for certain additional-interface/address
configurations). Do not require preservation of an auto-assigned public IPv4 or
clear all IP information indiscriminately. These are production side effects,
not extra user-requested networking work. [E-IP][E-LIFECYCLE]

### Retry guarantee: do not borrow TerminateInstances' documentation

**Uncertain:** the examined StopInstances API/model do not explicitly promise
that every stop retry against `stopping`/`stopped` returns success. The EC2
idempotency guide lists several operations as idempotent by default but does not
list `StopInstances`; the pinned request also has no client token.
This is an evidence gap, **not proof that StopInstances is non-idempotent**.
By contrast, `TerminateInstances` explicitly promises idempotence. [E-STOP][E-IDEMPOTENCY][E-TERMINATE][M-EC2]

**First-party example/inference:** AWS's Ruby stopping example checks
`stopping`/`stopped` and treats them as already satisfied without issuing another
stop; examples in other languages wait for completion. Idempotent
desired-state/retry handling is therefore appropriate for this task, but that
does not establish identical retry response bodies, request IDs, notifications,
or raw API success under every race/intervening restart. Do not fail a harmless
same-target repeat merely for being a repeat. [E-STOP-EXAMPLE][M-WAITER]

### Shutdown modes and multi-instance requests

**Documented defaults/caveats:** `Force`, `Hibernate`, and `SkipOsShutdown` default
to false. Force first attempts graceful shutdown, then forcibly shuts down if the
graceful attempt times out; AWS warns of filesystem check/repair needs and does
not recommend it for Windows. Skip-OS-shutdown bypasses graceful shutdown and can
lose/corrupt data or skip shutdown scripts. Hibernation needs launch-time
enablement/prerequisites, persists RAM to disk, and falls back to normal shutdown
if hibernation fails. [E-STOP][M-EC2]

**Scope implication:** ordinary stop is the straightforward reading; do not
require force, hibernation, or skipped shutdown. Their documented risks matter
to disk preservation, but the presence of one flag alone is not evidence that
data actually disappeared. Assess an alternative against requested shutdown and
preservation outcomes, rather than inventing an unrelated disk or RAM requirement.
[E-STOP][E-LIFECYCLE]

**Explicit batch evidence limit:** StopInstances accepts multiple instance IDs.
The examined StopInstances reference, pinned model, and stop-protection guide
do **not** specify transaction-wide rollback, per-ID partial success, processing
order, or exact effects for a batch mixing valid, missing, protected, or
wrong-state IDs. Stop protection prevents API stopping of the protected
instance, but that alone does not establish what happens to its batch peers.
Do not invent either all-or-nothing or partial-effect guarantees.
[E-STOP][E-PROTECTION][M-EC2]

**Contrast only:** TerminateInstances explicitly documents no terminations for
certain failed multi-ID requests and Availability-Zone-specific partial effects
with termination protection. Those rules belong to **TerminateInstances** and
must not be copied to StopInstances without a source. An unauthorized stop
attempt and an observed committed stop of another instance are also distinct
facts; do not claim a side effect from an error alone. [E-TERMINATE][E-STOP]

## 5. EC2 CreateTags: merge by key, not whole-list replacement

**Documented:** `CreateTags` adds or overwrites **only the specified tags** on
specified resources. An existing key's value is overwritten; tag keys are unique
per resource. Therefore adding `DecommissionTicket=OPS-4821` must preserve other
existing tag keys/values, rather than replace the entire tag collection.
Keys and values are case-sensitive in the pinned model documentation.
[E-TAGS][M-EC2]

**Documented ordering limit:** `DescribeTags` explicitly warns that response
element order, including nested structures, can vary. Neither CreateTags nor
StopInstances documents a dependency requiring the ticket tag to precede the
stop or vice versa. **Inference:** compare final tags as a key/value map and
accept either successful operation order; the task requires both results, not
an invented ordering ritual. Do not impose that response-array order equals
request or insertion order. [E-DESCRIBE-TAGS][E-TAGS][E-STOP]

**Uncertain duplicate-key request semantics:** the examined EC2 CreateTags
reference/model establishes uniqueness of stored keys, but not which value wins
or which error occurs if a *single request* repeats the same key with conflicting
values. Its SDK request is a list of tags, not a uniqueness-enforcing map.
Do not claim documented first-wins, last-wins, duplicate acceptance, or rejection
from that shape alone. This is distinct from documented overwrite of an already
stored tag by a subsequent valid request. [E-TAGS][M-EC2]

## Concise implications for the 23 red regressions

- Normalize supported CPU alarm representations and defaults, not just raw JSON
  field placement; validate the selected metric/account/instance and watched
  series. Bare identity aliases are not arbitrary unproved math. [C-PUT][C-QUERY][CDK]
- An unevaluated but supported metric-math request needs adjudication, not an
  invented model failure. Missing-data equivalence must not be assumed.
  [C-MATH][C-MISSING]
- Alarm storage/lookup needs regional and account identity. The evidence above
  does not license an invented cross-Region SNS validation error. [C-PUT][C-CFN][S-PUBLISH]
- Preserve existing alarms, other IAM principals, other instances, existing
  instance tags, and the retained disk according to the task wording. A restored
  final state need not erase an earlier committed unauthorized change.
  This is task interpretation, not a claim about an AWS audit payload schema.
- Separate asynchronous stop acceptance from completed stopped state; preserve
  documented EBS/private-IP/Elastic-IP behavior without demanding retention of
  automatically assigned public IPv4. [E-STOP][E-LIFECYCLE][E-IP]
- Retry success details, alternate-case IAM mutation, alarm ReturnData omission,
  mixed-batch StopInstances effects, and same-request duplicate-tag keys remain
  evidence-sensitive boundaries. Do not turn an unsupported inference into a
  confidently asserted production guarantee. [M-IAM][M-CW][M-EC2]

## Primary sources

All source links below are official AWS documentation or versioned first-party
source. API field anchors and named SDK shapes identify the specific contract.

[M-IAM]: https://github.com/boto/botocore/blob/1.43.123/botocore/data/iam/2010-05-08/service-2.json#L89
[M-CW]: https://github.com/boto/botocore/blob/1.43.123/botocore/data/cloudwatch/2010-08-01/service-2.json#L3834
[M-EC2]: https://github.com/boto/botocore/blob/1.43.123/botocore/data/ec2/2016-11-15/service-2.json#L74830
[M-WAITER]: https://github.com/boto/botocore/blob/1.43.123/botocore/data/ec2/2016-11-15/waiters-2.json
[M-VALIDATOR]: https://github.com/boto/botocore/blob/1.43.123/botocore/validate.py#L272
[CDK]: https://github.com/aws/aws-cdk/blob/v2.270.0/packages/aws-cdk-lib/aws-cloudwatch/lib/alarm.ts#L436
[I-ATTACH]: https://docs.aws.amazon.com/IAM/latest/APIReference/API_AttachUserPolicy.html
[I-ADD]: https://docs.aws.amazon.com/IAM/latest/APIReference/API_AddUserToGroup.html
[I-SECURITY]: https://docs.aws.amazon.com/aws-managed-policy/latest/reference/SecurityAudit.html
[I-POLICIES]: https://docs.aws.amazon.com/IAM/latest/UserGuide/access_policies_managed-vs-inline.html
[I-GROUPS]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_groups_manage_add-remove-users.html
[I-LIST-ATTACHED]: https://docs.aws.amazon.com/IAM/latest/APIReference/API_ListAttachedUserPolicies.html
[I-LIST-GROUPS]: https://docs.aws.amazon.com/IAM/latest/APIReference/API_ListGroupsForUser.html
[I-CONSISTENCY]: https://docs.aws.amazon.com/IAM/latest/UserGuide/troubleshoot.html#troubleshoot_general_eventual-consistency
[I-CREATE]: https://docs.aws.amazon.com/IAM/latest/APIReference/API_CreateUser.html#API_CreateUser_RequestParameters
[I-IDENTIFIERS]: https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_identifiers.html#identifiers-friendly-names
[C-PUT]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_PutMetricAlarm.html
[C-CFN]: https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/aws-resource-cloudwatch-alarm.html#cfn-cloudwatch-alarm-datapointstoalarm
[C-EVALUATION]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/alarm-evaluation.html
[C-MISSING]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/alarms-and-missing-data.html
[C-QUERY]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_MetricDataQuery.html
[C-STAT]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_MetricStat.html
[C-MATH]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/using-metric-math.html
[C-HISTORY]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_DescribeAlarmHistory.html
[C-HISTORY-ITEM]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_AlarmHistoryItem.html
[C-ALARMS]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Alarms.html
[C-ALARM]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_MetricAlarm.html
[C-DESCRIBE]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_DescribeAlarms.html#ACW-DescribeAlarms-request-StateValue
[C-SET]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_SetAlarmState.html
[S-PUBLISH]: https://docs.aws.amazon.com/sns/latest/api/API_Publish.html
[E-METRICS]: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/viewing_metrics_with_cloudwatch.html#ec2-cloudwatch-metrics
[E-STOP]: https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_StopInstances.html
[E-START]: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/Stop_Start.html
[E-LIFECYCLE]: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-instance-lifecycle.html#instance-stop-start
[E-IP]: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/using-instance-addressing.html#concepts-public-addresses
[E-IDEMPOTENCY]: https://docs.aws.amazon.com/ec2/latest/devguide/ec2-api-idempotency.html
[E-TERMINATE]: https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_TerminateInstances.html
[E-STOP-EXAMPLE]: https://docs.aws.amazon.com/ec2/latest/devguide/example_ec2_StopInstances_section.html
[E-PROTECTION]: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-stop-protection.html
[E-TAGS]: https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_CreateTags.html
[E-DESCRIBE-TAGS]: https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_DescribeTags.html
