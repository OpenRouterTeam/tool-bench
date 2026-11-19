def google_nonempty:
  type == "string" and length > 0;

def google_argument($arguments; $field; $default):
  $arguments[$field] | if type == "string" and test("\\S") then . else $default end;

def google_file_url($identity):
  capture("^https://(?<host>[^/]+)(?<path>/[^?#]*)(?:\\?[^#]*)?(?:#.*)?$") as $url
  | ($identity | google_nonempty)
    and (if $url.host == "drive.google.com" then
      ($url.path == ("/file/d/" + $identity + "/view")
        or $url.path == ("/drive/folders/" + $identity))
    elif $url.host == "docs.google.com" then
      (["document", "spreadsheets", "presentation", "forms"] | any(.[];
        $url.path == ("/" + . + "/d/" + $identity + "/edit")))
    else false end);

def google_permission($recipient):
  google_argument($recipient; "share_type"; "user") as $kind
  | google_argument($recipient; "role"; "reader") as $role
  | (if $kind == "anyone" then "Anyone with the link"
     elif $kind == "user" then "User: " + ($recipient.email // $recipient.share_with)
     elif $kind == "group" then "Group: " + ($recipient.email // $recipient.share_with)
     elif $kind == "domain" then "Domain: " + ($recipient.domain // $recipient.share_with)
     else error("unreviewed Google permission type") end) as $principal
  | "  - " + $principal + " (" + $role + ") [id: ";

def google_permission_line($recipient):
  . as $line
  | google_permission($recipient) as $prefix
  | ($line | startswith($prefix)) and ($line | endswith("]"))
    and ($line[($prefix | length):-1] | test("^[^\\r\\n\\[\\]]+$"));

def google_drive_write($arguments):
  . as $text
  | ($arguments.action | ascii_downcase | gsub("^\\s+|\\s+$"; "")) as $action
  | if $action == "grant" then
      ($text | split("\n")) as $lines
      | ($lines | length) == 6
        and ($lines[0] | test("^Successfully shared '.+'$"))
        and $lines[1] == "" and $lines[2] == "Permission created:"
        and ($lines[3] | google_permission_line($arguments))
        and $lines[4] == "" and ($lines[5] | startswith("View link: "))
    elif $action == "grant_batch" then
      ($text | split("\n")) as $lines
      | ($arguments.recipients | type) == "array" and ($arguments.recipients | length) > 0
        and ($lines[0] | test("^Batch share results for '.+'$"))
        and $lines[1] == "" and $lines[2] == ("Summary: " + ($arguments.recipients | length | tostring) + " succeeded, 0 failed")
        and $lines[3] == "" and $lines[4] == "Results:"
        and ($lines | length) == (($arguments.recipients | length) + 7)
        and all($arguments.recipients | to_entries[];
          . as $item | $lines[5 + $item.key] | google_permission_line($item.value))
        and $lines[-2] == "" and ($lines[-1] | startswith("View link: "))
        and ($lines[-1] | ltrimstr("View link: ") | google_file_url($arguments.file_id))
    elif $action == "update" then
      ($text | split("\n")) as $lines
      | ($arguments.permission_id | google_nonempty) and ($arguments.role | google_nonempty)
        and ($lines | length) == 4
        and ($lines[0] | test("^Successfully updated permission on '.+'$"))
        and $lines[1] == "" and $lines[2] == "Updated permission:"
        and ($lines[3] | startswith("  - "))
        and ($lines[3] | endswith(" (" + $arguments.role + ") [id: " + $arguments.permission_id + "]"))
    elif $action == "revoke" then
      ($text | split("\n")) as $lines
      | ($arguments.permission_id | google_nonempty) and ($lines | length) == 3
        and ($lines[0] | test("^Successfully removed permission from '.+'$"))
        and $lines[1] == ""
        and $lines[2] == ("Permission ID '" + $arguments.permission_id + "' has been revoked.")
    else error("unreviewed Google Drive completion action") end;

def google_sheet_result($arguments):
  . as $text
  | ($arguments.spreadsheet_id | google_nonempty) and ($arguments.range_name | google_nonempty) as $context
  | if $arguments.clear_values == true then
      $context and ($text | startswith("Successfully cleared range '"))
        and ($text | endswith("' in spreadsheet " + $arguments.spreadsheet_id + " for ops@octo-corp.example."))
        and (if ($arguments.range_name | contains("!")) then
          $text == ("Successfully cleared range '" + $arguments.range_name + "' in spreadsheet "
            + $arguments.spreadsheet_id + " for ops@octo-corp.example.")
        else
          ($text | ltrimstr("Successfully cleared range '")
            | split("' in spreadsheet ")[0] | endswith("!" + $arguments.range_name))
        end)
    else
      ($arguments.values | if type == "string" then fromjson else . end) as $rows
      | if ($rows | type) != "array" or any($rows[]; type != "array")
        then error("unreviewed Google values representation") else . end
      | [$rows | to_entries[] | .key as $row | .value | to_entries[]
          | select(.value != null) | {row:$row,column:.key}] as $cells
      | $context and $text == ("Successfully updated range '" + $arguments.range_name
          + "' in spreadsheet " + $arguments.spreadsheet_id + " for ops@octo-corp.example. Updated: "
          + ($cells | length | tostring) + " cells, "
          + ($cells | map(.row) | unique | length | tostring) + " rows, "
          + ($cells | map(.column) | unique | length | tostring) + " columns.")
    end;

def google_event_result($arguments):
  . as $text
  | ($arguments.action | ascii_downcase | gsub("^\\s+|\\s+$"; "")) as $action
  | google_argument($arguments; "calendar_id"; "primary") as $requested_calendar
  | (if $requested_calendar == "primary" then "ops@octo-corp.example" else $requested_calendar end) as $calendar
  | if $action == "delete" then
      ($arguments.event_id | google_nonempty) and $text == ("Successfully deleted event (ID: "
        + $arguments.event_id + ") from calendar '" + $requested_calendar + "' for ops@octo-corp.example.")
    elif $action == "rsvp" then
      ($arguments.event_id | google_nonempty) and ($arguments.response | google_nonempty)
        and ($text | startswith("Successfully updated RSVP for '"))
        and ($text | endswith("' (ID: " + $arguments.event_id + ") to '" + $arguments.response
          + "' for ops@octo-corp.example."))
    elif $action == "create" or $action == "update" then
      (if $action == "create" then
        "Successfully created event '" + $arguments.summary + "' for ops@octo-corp.example. Link: "
      elif $arguments.summary != null then
        "Successfully modified event '" + $arguments.summary + "' (ID: " + $arguments.event_id
          + ") for ops@octo-corp.example. Link: "
      else
        ($text | split("' (ID: " + $arguments.event_id + ") for ops@octo-corp.example. Link: ")) as $parts
        | if ($parts | length) != 2 or ($parts[0] | startswith("Successfully modified event '") | not)
          then error("Google update identity is not bound")
          else $parts[0] + "' (ID: " + $arguments.event_id + ") for ops@octo-corp.example. Link: " end
      end) as $prefix
      | if ($text | startswith($prefix) | not) then false
        else
          ($text[($prefix | length):] | split("\n")) as $lines
          | ($lines[0] | ltrimstr("https://www.google.com/calendar/event?eid=")) as $encoded
          | (try ($encoded | @base64d | split(" ")) catch []) as $identity
          | ($lines[3] | capture("^(?<duration>Elapsed duration: (?<minutes>[0-9]+([.][0-9]+)?) minutes [(](?<seconds>[0-9]+) seconds[)]|All-day span: [0-9]+ days? [(]end date exclusive[)])(?<suffix>.*)$")) as $timing
          | ($lines | length) == 4
            and ($lines[0] | startswith("https://www.google.com/calendar/event?eid="))
            and ($identity | length) == 2 and ($identity[0] | google_nonempty) and $identity[1] == $calendar
            and ($action != "update" or $identity[0] == $arguments.event_id)
            and ($lines[1] | test("^Saved start: [^\\r\\n]+$"))
            and ($lines[2] | test("^Saved end: [^\\r\\n]+$"))
            and ($timing.minutes == null or
              ($timing.minutes | tonumber) == (($timing.seconds | tonumber) / 60))
            and (if $arguments.add_google_meet == true then
              ($timing.suffix | test("^ Google Meet: https://meet[.]google[.]com/[a-z]{3}-[a-z]{4}-[a-z]{3}$"))
            elif $arguments.conference_uri != null then
              $timing.suffix == (" Conference: " + $arguments.conference_uri)
            elif $arguments.conference_data != null then
              ($timing.suffix | test("^( Conference: [^\\r\\n]+)?$"))
            elif $action == "update" and $arguments.add_google_meet == false then
              $timing.suffix == " (Google Meet removed)"
            else $timing.suffix == "" end)
        end
    else error("unreviewed Google Calendar completion action") end;

def google_permission_snapshot:
  . as $lines
  | [$lines[] | select(startswith("    - "))] as $rows
  | [$lines[] | select(startswith("  Number of permissions: "))] as $counts
  | any($lines[]; . == "Sharing Status:")
    and any($lines[]; . == "  Shared: True" or . == "  Shared: False")
    and (if ($counts | length) == 0 then
      ($rows | length) == 0 and any($lines[]; . == "  No additional permissions (private file)")
    elif ($counts | length) == 1 then
      ($counts[0] | capture("^  Number of permissions: (?<count>[0-9]+)$").count | tonumber) as $count
      | $count > 0 and $count == ($rows | length)
        and all($rows[]; test(" [(](reader|commenter|writer|owner|organizer|fileOrganizer)[)] \\[id: [^\\[\\]\\r\\n]+\\]( [|] expires: [^\\r\\n]+)?$"))
    else false end);

def google_read_result($tool; $arguments):
  split("\n") as $lines
  | ($arguments.file_id | google_nonempty) and ($lines[0] | startswith("File: "))
    and $lines[1] == ("ID: " + $arguments.file_id) and ($lines[2] | startswith("Type: "))
    and (if $tool == "get_drive_file_permissions" then
      ($lines | google_permission_snapshot)
        and any($lines[]; startswith("  View Link: ") and (ltrimstr("  View Link: ") | google_file_url($arguments.file_id)))
    else
      ($lines[3] == "Shared: True" or $lines[3] == "Shared: False")
        and $lines[4] == "" and $lines[5] == "Links:"
        and ($lines[6] | startswith("  View: "))
        and ($lines[6] | ltrimstr("  View: ") | google_file_url($arguments.file_id))
    end);

def google_replica_result($tool; $arguments):
  . as $result
  | if (.content | type) != "array" or (.content | length) != 1
      or .content[0].type != "text" or (.content[0].text | type) != "string"
      or (.structuredContent | type) != "object" or (.structuredContent | keys) != ["result"]
      or .structuredContent.result != .content[0].text
      or (has("isError") and .isError != false)
      or ($arguments.user_google_email != null and $arguments.user_google_email != "ops@octo-corp.example")
    then error("Google replica result lacks consistent selected-condition text envelopes")
    else .
    end
  | (.content[0].text | if $tool == "modify_sheet_values" then google_sheet_result($arguments)
      elif $tool == "manage_event" then google_event_result($arguments)
      elif $tool == "manage_drive_access" then
        google_drive_write($arguments) and
          (if ($arguments.action | ascii_downcase | gsub("^\\s+|\\s+$"; "")) == "grant" then
            split("\n") as $lines
            | ($lines[-1] | ltrimstr("View link: ") | google_file_url($arguments.file_id))
          else true end)
      elif $tool == "get_drive_file_permissions" or $tool == "get_drive_shareable_link"
        then google_read_result($tool; $arguments)
      else false end)
  | if . then $result else error("Google replica completion or printed request binding failed") end;
