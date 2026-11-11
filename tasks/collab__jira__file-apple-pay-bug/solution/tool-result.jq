include "google-replica";
include "firecrawl-native";

def datadog_padded($width):
  tostring | ("000000" + .)[-$width:];

def datadog_iso:
  if type != "number" or floor != . or fabs > 8640000000000
  then error("Datadog host expiry cannot be represented by the pinned native Date model")
  else .
  end
  | gmtime as $date
  | (if $date[0] >= 0 and $date[0] <= 9999
     then ($date[0] | datadog_padded(4))
     else (if $date[0] < 0 then "-" else "+" end) + ($date[0] | fabs | datadog_padded(6))
     end)
    + "-" + ($date[1] + 1 | datadog_padded(2))
    + "-" + ($date[2] | datadog_padded(2))
    + "T" + ($date[3] | datadog_padded(2))
    + ":" + ($date[4] | datadog_padded(2))
    + ":" + ($date[5] | datadog_padded(2)) + ".000Z";

def datadog_scope:
  if type != "array" or length == 0 or any(.[]; type != "string")
  then error("Datadog scope lacks a nonempty text-array representation")
  else [.[] | split(",")[] | gsub("^\\s+|\\s+$"; "")] | unique
  end;

def datadog_api_result($tool; $arguments):
  . as $result
  | if (.content|type) != "array" or (.content|length) != 1
      or .content[0].type != "text" or (.content[0].text|type) != "string"
      or has("structuredContent")
    then error("Datadog result lacks its pinned text-only envelope")
    else .
    end
  | .content[0].text as $text
  | if $tool == "cancel_downtime" then
      ($arguments.downtimeId | type) == "number"
        and ($arguments.downtimeId | floor) == $arguments.downtimeId
        and $arguments.downtimeId > 0
        and $text == ("Cancelled downtime with ID: " + ($arguments.downtimeId|tostring))
    elif $tool == "mute_host" then
      ($arguments.hostname|type) == "string" and ($arguments.hostname|length) > 0
        and ($arguments.message == null or ($arguments.message|type) == "string")
        and ($text | fromjson) == {
          status:"success",
          message:(
            "Host " + $arguments.hostname + " has been muted successfully"
            + (if $arguments.message != null and $arguments.message != ""
               then " with message: " + $arguments.message else "" end)
            + (if $arguments.end != null and $arguments.end != 0
               then " until " + ($arguments.end | datadog_iso) else "" end)
          )
        }
    elif $tool == "schedule_downtime" then
      (if ($text | startswith("Scheduled downtime: ") | not)
      then error("Datadog schedule result lacks its pinned prefix")
      else $text | ltrimstr("Scheduled downtime: ") | fromjson
      end) as $payload
      | ($payload | type) == "object"
        and ($payload.id|type) == "number" and ($payload.id|floor) == $payload.id and $payload.id > 0
        and $payload.disabled == false and $payload.canceled == null
        and ($payload.start|type) == "number"
        and ($payload.end == null or ($payload.end|type) == "number")
        and ($arguments.scope|type) == "string"
        and ($payload.scope | datadog_scope) == ([$arguments.scope] | datadog_scope)
        and ($arguments.start == null or $payload.start == $arguments.start)
        and ($arguments.end == null or $payload.end == $arguments.end)
        and ($arguments.message == null or $payload.message == $arguments.message)
        and ($arguments.timezone == null or $payload.timezone == $arguments.timezone)
        and ($arguments.monitorId == null or $payload.monitorId == $arguments.monitorId)
        and ($arguments.monitorTags == null or
          (($payload.monitorTags|type) == "array" and ($payload.monitorTags|sort) == ($arguments.monitorTags|sort)))
        and (if $arguments.recurrence == null then $payload.recurrence == null
             else
               $arguments.recurrence.until == null
               and $payload.recurrence.type == $arguments.recurrence.type
               and $payload.recurrence.period == $arguments.recurrence.period
               and ($arguments.recurrence.weekDays == null or
                 ($payload.recurrence.weekDays|sort) == ($arguments.recurrence.weekDays|sort))
             end)
    else false
    end
  | if . then $result else error("Datadog write completion or request binding failed") end;

def no_failures:
  . == null or (type == "array" and length == 0);

def aws_api_result($commands; $permissions):
  . as $result
  | if (.structuredContent.result | type) != "array"
      or (.structuredContent.result | length) != ($commands | length)
    then error("AWS command result count differs from the actual request")
    else .
    end
  | if any(.structuredContent.result[]; (.response | type) == "string")
    then error("AWS CLI stdout/stderr result form is not covered by this oracle result contract")
    else .
    end
  | .structuredContent.result
  | to_entries
  | all(.[];
      .key as $index
      | .value as $entry
      | ($entry | type) == "object"
        and $entry.cli_command == $commands[$index]
        and $entry.error == null
        and ($entry.validation_failures | no_failures)
        and ($entry.missing_context_failures | no_failures)
        and ($entry.failed_constraints | no_failures)
        and ($entry.response | type) == "object"
        and ($entry.response.status_code | type) == "number"
        and ($entry.response.status_code | floor) == $entry.response.status_code
        and (
          ($entry.response.status_code >= 200 and $entry.response.status_code < 300
            and $entry.response.error == null and $entry.response.error_code == null)
          or (
            ($entry.response.error | type) == "string"
            and ($entry.response.error | length) > 0
            and any($permissions[];
              .command == $commands[$index]
                and .status == $entry.response.status_code
                and .code == $entry.response.error_code
            )
          )
        )
    )
  | if . then $result else error("AWS command execution, validation or request binding failed") end;

def argo_api_result:
  . as $result
  | if (.content|type) != "array" or (.content|length) != 1
      or .content[0].type != "text" or (.content[0].text|type) != "string"
    then error("Argo CD result lacks its pinned JSON text envelope")
    else .
    end
  | .content[0].text | fromjson
  | if type == "object" and ((keys - ["code","details","error","message"])|length) == 0
      and has("code") and has("error") and has("message")
    then if (.code|type) == "number" and (.code|floor) == .code and .code >= 1 and .code <= 16
        and (.error|type) == "string" and (.message|type) == "string"
      then error("Argo CD returned a structured gRPC-gateway error")
      else error("Argo CD returned a malformed gateway error envelope")
      end
    else $result
    end;

def okta_failure_envelope:
  type == "object" and (
    (keys == ["error"])
    or (keys == ["exception"])
    or (has("ok") and ((keys - ["error","ok","raw","status_code"])|length) == 0)
  );

def okta_api_result($tool; $arguments):
  . as $result
  | if (.content|type) != "array"
      or any(.content[]; .type != "text" or (.text|type) != "string")
    then error("Okta result lacks its pinned JSON text blocks")
    else .
    end
  | [.content[].text | fromjson] as $payloads
  | if any($payloads[];
      if type == "array" then any(.[]; okta_failure_envelope) else okta_failure_envelope end)
    then error("Okta returned a native error, exception or serialization envelope")
    elif has("structuredContent") and (
      (.structuredContent|type) != "object"
      or (.structuredContent|keys) != ["result"]
      or (.structuredContent.result != $payloads
        and (($payloads|length) != 1 or .structuredContent.result != $payloads[0]))
    )
    then error("Okta structured and text results disagree")
    elif $tool == "deactivate_user" and (
      ($payloads|length) != 1 or ($payloads[0]|type) != "object"
      or ($payloads[0]|keys) != ["message"]
      or $payloads[0].message != ("User " + $arguments.user_id + " deactivated successfully.")
    )
    then error("The pinned Okta deactivation tool did not return its request-bound completion message")
    else $result
    end;

def terraform_api_result($tool; $arguments):
  . as $result
  | if (.content|type) != "array" or (.content|length) != 1
      or .content[0].type != "text" or (.content[0].text|type) != "string"
      or (.content[0].text|length) == 0 or has("structuredContent")
    then error("Terraform result lacks its pinned single-text envelope")
    else .content[0].text
    end
  | if $tool == "action_run" then
      ({"apply":"Run approved and applied successfully, run the `get_run_details` tool to get more information about the run.",
        "discard":"Run discarded successfully","cancel":"Run canceled successfully"}[$arguments.run_action]) as $expected
      | if $expected == null or . != $expected
        then error("Terraform run-action response differs from its requested action")
        else $result end
    elif $tool == "force_unlock_workspace" then
      ($arguments.workspace_id | gsub("^\\s+|\\s+$";"")) as $workspace
      | if . != ("Workspace " + ($workspace|tojson) + " is now unlocked")
        then error("Terraform force-unlock response differs from its requested workspace")
        else $result end
    elif $tool == "update_workspace_variable" then
      if . != ("Updated variable " + $arguments.key + " with ID " + $arguments.variable_id)
        then error("Terraform variable-update response differs from its requested identity")
        else $result end
    else
      fromjson as $payload
      | if ($payload|type) != "object" or ($payload|has("errors"))
          or ($payload|has("error")) or ($payload|has("exception"))
        then error("Terraform returned an invalid or error payload")
        elif $tool == "create_run" or $tool == "get_run_details" then
          if ($payload.data|type) != "object" or $payload.data.type != "runs"
              or ($payload.data.id|type) != "string" or ($payload.data.id|length) == 0
              or ($payload.data.attributes.status|type) != "string"
              or ($tool == "get_run_details" and $payload.data.id != $arguments.run_id)
              or ($tool == "create_run" and ($arguments|has("message"))
                and $payload.data.attributes.message != $arguments.message)
            then error("Terraform did not return its request-bound run record")
            else $result end
        elif $tool == "list_workspace_variables" then
          if ($payload.data|type) != "array"
              or any($payload.data[];
                .type != "vars" or (.id|type) != "string" or (.id|length) == 0
                or (.attributes.key|type) != "string" or (.attributes.category|type) != "string")
            then error("Terraform did not return its variable collection")
            else $result end
        elif $tool == "list_runs" or $tool == "list_workspaces" or $tool == "list_terraform_projects" then
          if ($payload.items|type) != "array"
              or ($payload["current-page"]|type) != "number"
              or ($payload["total-pages"]|type) != "number"
              or any($payload.items[];
                if $tool == "list_terraform_projects" then
                  (.project_id|type) != "string" or (.project_name|type) != "string"
                elif $tool == "list_workspaces" then
                  (.id|type) != "string" or (.workspace_name|type) != "string"
                else (.id|type) != "string" or (.status|type) != "string" end)
            then error("Terraform did not return its pinned paginated collection")
            else $result end
        else error("Terraform result validation lacks a reviewed tool contract")
        end
    end;

def nonempty_string:
  type == "string" and length > 0;

def positive_integer:
  type == "number" and floor == . and . > 0;

def pagerduty_instant:
  if type != "string" then error("PagerDuty timestamp is not a string") else . end
  | [capture("^(?<year>[0-9]{4})-(?<month>[0-9]{2})-(?<day>[0-9]{2})[Tt ](?<hour>[0-9]{2}):(?<minute>[0-9]{2}):(?<second>[0-9]{2})(?:\\.(?<fraction>[0-9]{1,9}))?(?<zone>[Zz]|[+-][0-9]{2}:[0-9]{2})?$")] as $captures
  | if ($captures | length) != 1 then error("PagerDuty timestamp is outside the reviewed precision") else $captures[0] end
  | . as $stamp
  | (.year | tonumber) as $year | (.month | tonumber) as $month | (.day | tonumber) as $day
  | (.hour | tonumber) as $hour | (.minute | tonumber) as $minute | (.second | tonumber) as $second
  | ($year % 4 == 0 and ($year % 100 != 0 or $year % 400 == 0)) as $leap
  | [31,(if $leap then 29 else 28 end),31,30,31,30,31,31,30,31,30,31] as $days
  | (if .zone == null or .zone == "Z" or .zone == "z" then 0
      else (.zone[1:3] | tonumber) as $hours | (.zone[4:6] | tonumber) as $minutes
        | if $hours > 23 or $minutes > 59 then error("PagerDuty timestamp offset is invalid")
          else ($hours * 3600 + $minutes * 60) * (if $stamp.zone[0:1] == "-" then -1 else 1 end) end
      end) as $offset
  | if $year == 0 or $month < 1 or $month > 12 or $day < 1 or $day > $days[$month - 1]
      or $hour > 23 or $minute > 59 or $second > 59
    then error("PagerDuty timestamp calendar fields are invalid")
    else ($year - 1) as $previous
      | [((365 * $previous + (($previous / 4) | floor) - (($previous / 100) | floor)
            + (($previous / 400) | floor) + (($days[0:($month - 1)] | add) // 0)
            + $day - 1 - 719162) * 86400 + $hour * 3600 + $minute * 60 + $second - $offset),
          ((($stamp.fraction // "") + "000000000")[0:9])]
    end;

def pagerduty_native_instant:
  if type != "string" then error("PagerDuty native timestamp is not a string") else . end
  | sub("(?<fraction>\\.[0-9]{6})[0-9]+"; "\(.fraction)")
  | pagerduty_instant;

def pagerduty_payload($wrapped):
  . as $result
  | if .isError == true or (.content | type) != "array"
      or any(.content[]; .type != "text" or (.text | type) != "string")
    then error("PagerDuty result lacks its pinned JSON text blocks")
    else [.content[].text | fromjson]
    end
  | (if $wrapped then
        if length == 1 and (.[0] | type) == "array" then .[0] else . end
      elif length == 1 and (.[0] | type) == "object" then .[0]
      else error("PagerDuty typed result lacks its single object") end) as $payload
  | if ($result | has("structuredContent")) and
        (if $wrapped then $result.structuredContent != {"result":$payload}
         else $result.structuredContent != $payload end)
    then error("PagerDuty text and structured results disagree")
    else $payload end;

def pagerduty_api_result($tool; $arguments):
  . as $result
  | (if $tool == "create_schedule_override" then pagerduty_payload(true)
      else pagerduty_payload(false) end) as $payload
  | (if $tool == "create_schedule_override" then
      $arguments.override_request.overrides as $requested
      | ($arguments.schedule_id | nonempty_string)
        and ($requested | type) == "array" and ($requested | length) > 0
        and all($requested[]; (.user_id | nonempty_string)
          and (.start | pagerduty_native_instant) < (.end | pagerduty_native_instant))
        and ($payload | type) == "array" and ($payload | length) == ($requested | length)
        and ([$payload[].override.id] | unique | length) == ($payload | length)
        and all($payload[];
          . as $item
          | ($item | type) == "object" and $item.status == 201 and ($item.errors | no_failures)
            and ($item.override | type) == "object" and ($item.override.id | nonempty_string)
            and ($item.override.user.id | nonempty_string))
        and ([$payload[] | [.override.user.id, (.override.start | pagerduty_instant), (.override.end | pagerduty_instant)]] | sort)
          == ([$requested[] | [.user_id, (.start | pagerduty_native_instant), (.end | pagerduty_native_instant)]] | sort)
    elif $tool == "add_note_to_incident" then
      ($arguments.incident_id | nonempty_string) and ($arguments.note | nonempty_string)
        and ($payload | type) == "object" and ($payload.id | nonempty_string)
        and $payload.content == $arguments.note and ($payload.user.id | nonempty_string)
        and ($payload.created_at | nonempty_string)
    elif $tool == "manage_incidents" then
      $arguments.manage_request as $manage
      | ($manage.incident_ids | type) == "array" and ($manage.incident_ids | length) > 0
        and all($manage.incident_ids[]; nonempty_string)
        and ($manage.status == "acknowledged" or $manage.status == "resolved"
          or $manage.urgency == "high" or $manage.urgency == "low"
          or ($manage.assignement.id | nonempty_string) or ($manage.escalation_level | positive_integer))
        and ($payload.response | type) == "array"
        and ($payload.response | length) == ($manage.incident_ids | length)
        and ([$payload.response[].id] | sort) == ($manage.incident_ids | sort)
        and all($payload.response[];
          (.incident_number | positive_integer) and (.title | type) == "string"
            and (.service.id | nonempty_string) and (.created_at | nonempty_string)
            and (.updated_at | nonempty_string)
            and (.status == "triggered" or .status == "acknowledged" or .status == "resolved")
            and (if $manage.assignement != null and $manage.escalation_level == null then
                (.assignments | type) == "array" and (.assignments | length) == 1
                  and .assignments[0].assignee.id == $manage.assignement.id
              else true end)
            and (if $manage.escalation_level != null or $manage.assignement != null then .status == "triggered"
              elif $manage.status != null then .status == $manage.status else true end))
    else false end)
  | if . then $result else error("PagerDuty operation, nested result or request binding failed") end;

def grafana_silence:
  type == "object" and (.id | nonempty_string)
    and (.startsAt | nonempty_string) and (.endsAt | nonempty_string)
    and (.matchers | type) == "array" and (.matchers | length) > 0
    and all(.matchers[]; (.name | nonempty_string) and (.value | type) == "string"
      and (.isRegex | type) == "boolean" and (.isEqual == null or (.isEqual | type) == "boolean"))
    and (.status.state == "pending" or .status.state == "active" or .status.state == "expired");

def grafana_api_result($tool; $arguments):
  . as $result
  | if (.content | type) != "array" or (.content | length) != 1
      or .content[0].type != "text" or (.content[0].text | type) != "string"
      or has("structuredContent")
    then error("Grafana result lacks its pinned single JSON text envelope")
    else .content[0].text | fromjson
    end
  | (if $tool == "grafana_api_request" then
      type == "object" and (.status | type) == "number" and (.status | floor) == .status
        and .status >= 200 and .status < 300 and has("data")
        and (.headers == null or (.headers | type) == "object")
    elif $tool == "create_annotation" then
      type == "object" and (.Payload | type) == "object" and (.Payload.id | positive_integer)
        and .Payload.message == (if $arguments.format == "graphite" then "Graphite annotation added" else "Annotation added" end)
    elif $tool == "get_annotations" then
      type == "object" and (.Payload | type) == "array"
        and all(.Payload[]; type == "object" and (.id | positive_integer)
          and (.text | type) == "string" and (.time | type) == "number"
          and (.timeEnd | type) == "number")
    elif $tool == "search_dashboards" then
      type == "object" and (.dashboards | type) == "array"
        and .total == (.dashboards | length) and (.hasMore | type) == "boolean"
        and all(.dashboards[]; (.uid | nonempty_string) and (.title | type) == "string"
          and (.url | type) == "string")
    elif $tool == "update_alert_group" then
      ($arguments.state | ascii_downcase | gsub("^\\s+|\\s+$"; "")) as $state
      | ({"acknowledged":"acknowledge","unacknowledged":"unacknowledge",
          "resolved":"resolve","unresolved":"unresolve"}[$state]) as $action
      | type == "object" and $action != null and .alertGroupId == $arguments.alertGroupId
        and .action == $action and .updated == true
        and ((has("stateWarning") | not) and (.state == "new" or .state == "acknowledged"
              or .state == "resolved" or .state == "silenced")
          or ((has("state") | not) and (.stateWarning | nonempty_string)))
    elif $tool == "get_current_oncall_users" then
      type == "object" and .scheduleId == $arguments.scheduleId
        and (.scheduleName | type) == "string" and (.users | type) == "array"
        and all(.users[]; (.id | nonempty_string) and (.username | type) == "string")
    elif $tool == "get_alert_group" then
      type == "object" and .id == $arguments.alertGroupId and (.state | nonempty_string)
    elif $tool == "list_alert_groups" then
      type == "array" and all(.[]; (.id | nonempty_string) and (.state | nonempty_string))
    elif $tool == "list_oncall_schedules" or $tool == "list_oncall_teams" then
      type == "array" and all(.[]; (.id | nonempty_string) and (.name | type) == "string")
    elif $tool == "list_oncall_users" then
      type == "array" and all(.[]; (.id | nonempty_string) and (.username | type) == "string")
    elif $tool == "alerting_silences_write" or $tool == "alerting_silences_read" then
      if $arguments.operation == "create" or $arguments.operation == "update" then
        type == "object" and (.silenceID | nonempty_string)
      elif $arguments.operation == "get" then
        grafana_silence and .id == $arguments.silence_id
      elif $arguments.operation == "list" then
        type == "array" and all(.[]; grafana_silence)
      elif $arguments.operation == "delete" then
        type == "object" and .status == "deleted" and .silence_id == $arguments.silence_id
      else false end
    else false end)
  | if . then $result else error("Grafana returned a failed HTTP result or an invalid request-bound native payload") end;

. as $decoded
| if ($contracts | length) != 1 or $contracts[0].schema_version != 1
    or $contracts[0].protocol != "oracle-tool-results-v1"
    or ($contracts[0].profiles | type) != "object"
    or ($contracts[0].default_profiles != null and ($contracts[0].default_profiles|type) != "object")
    or ($contracts[0].allowed_aws_api_errors | type) != "array"
    or ($request | length) != 1 or ($request[0] | type) != "object"
    or $request[0].jsonrpc != "2.0" or $request[0].id != 2
    or $request[0].method != "tools/call"
    or ($request[0].params.name | type) != "string"
    or ($request[0].params.name | length) == 0
    or ($request[0].params.arguments | type) != "object"
    or ($contracts[0].profiles | has($endpoint) | not)
    or ($contracts[0].profiles[$endpoint] | type) != "object"
then error("oracle result validation lacks a complete declared request context")
else .
end
| (if ($contracts[0].profiles[$endpoint] | has($request[0].params.name))
   then $contracts[0].profiles[$endpoint][$request[0].params.name]
   elif (($contracts[0].default_profiles // {}) | has($endpoint))
   then $contracts[0].default_profiles[$endpoint]
   else "mcp_envelope_v1" end) as $profile
| if $profile == "mcp_envelope_v1" then .
  elif $profile == "firecrawl_native_v1" then firecrawl_native_result($request[0].params.name; $request[0].params.arguments)
  elif $profile == "argo_api_v1" then argo_api_result
  elif $profile == "okta_api_v1" then okta_api_result($request[0].params.name; $request[0].params.arguments)
  elif $profile == "terraform_api_v1" then terraform_api_result($request[0].params.name; $request[0].params.arguments)
  elif $profile == "grafana_api_v1" then grafana_api_result($request[0].params.name; $request[0].params.arguments)
  elif $profile == "pagerduty_api_v1" then pagerduty_api_result($request[0].params.name; $request[0].params.arguments)
  elif $profile == "datadog_api_v1" then datadog_api_result($request[0].params.name; $request[0].params.arguments)
  elif $profile == "google_workspace_replica_v1" then google_replica_result($request[0].params.name; $request[0].params.arguments)
  elif $profile == "aws_api_v1" then
    $request[0].params.arguments.cli_command
    | (if type == "string" then [.] else . end) as $commands
    | if ($commands | type) != "array"
        or any($commands[]; type != "string" or (test("\\S") | not))
      then error("AWS result validation requires complete CLI command strings")
      elif any($commands[]; test("--region\\s+\\*(?=\\s|$)"))
      then error("AWS wildcard-region result expansion is not covered by this oracle result contract")
      else $commands
      end
    | $decoded | aws_api_result($commands; $contracts[0].allowed_aws_api_errors)
  else error("unrecognized oracle tool-result contract")
  end
