def firecrawl_string:
  type == "string" and test("\\S");

def firecrawl_count:
  type == "number" and . >= 0 and floor == .;

def firecrawl_record:
  type == "object" and (.id | firecrawl_string)
    and (.name | firecrawl_string) and (.status == "active" or .status == "paused");

def firecrawl_check_record($monitor):
  .status as $status
  | type == "object" and (.id | firecrawl_string) and .monitorId == $monitor
    and (["queued", "running", "completed", "partial", "failed",
      "skipped_no_credits", "skipped_overlap"] | index($status)) != null;

def firecrawl_declared_keys($tool):
  if $tool == "firecrawl_map" then
    ["links", "success", "error", "warning", "id"]
  elif $tool == "firecrawl_crawl" or $tool == "firecrawl_check_crawl_status" then
    ["success", "error", "warning", "id", "status", "completed", "total",
      "creditsUsed", "expiresAt", "next", "url", "data"]
  elif $tool == "firecrawl_monitor_list" or $tool == "firecrawl_monitor_checks" then
    ["success", "error", "data", "pagination"]
  elif $tool == "firecrawl_monitor_check" then
    ["success", "error", "data", "pagination", "next"]
  else
    ["success", "error", "data"]
  end;

def firecrawl_native_result($tool; $arguments):
  . as $result
  | if (.content | type) != "array" or (.content | length) != 1
      or .content[0].type != "text" or (.content[0].text | type) != "string"
      or (.structuredContent | type) != "object" or .isError == true
    then error("Firecrawl result lacks its native JSON text/structured envelope")
    else .content[0].text | fromjson
    end
  | . as $payload
  | firecrawl_declared_keys($tool) as $keys
  | if type != "object" or (.success != null and .success != true) or .error != null
      or $result.structuredContent !=
        ($payload | with_entries(select(.key as $key | $keys | index($key))))
    then error("Firecrawl failed or returned inconsistent native representations")
    else .
    end
  | (if $tool == "firecrawl_map" then
      (.links | type) == "array"
        and all(.links[];
          if type == "string" then firecrawl_string
          elif type == "object" then .url | firecrawl_string
          else false end)
    elif $tool == "firecrawl_crawl" or $tool == "firecrawl_check_crawl_status" then
      (.id | firecrawl_string) and (.completed | firecrawl_count)
        and (.total | firecrawl_count) and .completed <= .total
        and (.data | type) == "array" and .next == null
        and (if $tool == "firecrawl_crawl" then .status == "completed"
             else .status as $status
               | (["queued", "scraping", "completed", "failed", "cancelled"] | index($status)) != null
             end)
        and ($tool != "firecrawl_check_crawl_status" or .id == $arguments.id)
    elif $tool == "firecrawl_monitor_list" then
      .success == true and (.data | type) == "array" and all(.data[]; firecrawl_record)
    elif $tool == "firecrawl_monitor_create" then
      (if ($arguments.body | type) == "object" then $arguments.body.name
       else $arguments.name end) as $name
      | .success == true and (.data | firecrawl_record)
        and ($name == null or ($name | gsub("^\\s+|\\s+$"; "")) == "" or
          .data.name == ($name | gsub("^\\s+|\\s+$"; "")))
    elif $tool == "firecrawl_monitor_get" or $tool == "firecrawl_monitor_update" then
      .success == true and (.data | firecrawl_record) and .data.id == $arguments.id
        and (if $tool == "firecrawl_monitor_update" then
          ($arguments.body.status == null or .data.status == $arguments.body.status)
          and ($arguments.body.name == null or
            .data.name == ($arguments.body.name | gsub("^\\s+|\\s+$"; "")))
          and ($arguments.body.goal == null or
            .data.goal == ($arguments.body.goal | gsub("^\\s+|\\s+$"; "")))
          and ($arguments.body.judgeEnabled == null or .data.judgeEnabled == $arguments.body.judgeEnabled)
        else true end)
    elif $tool == "firecrawl_monitor_run" then
      .success == true and (.data | firecrawl_check_record($arguments.id))
        and (.id == null or .id == .data.id)
        and .data.trigger == "manual"
        and (.data.status == "queued" or .data.status == "running" or .data.status == "completed")
    elif $tool == "firecrawl_monitor_checks" then
      .success == true and (.data | type) == "array"
        and all(.data[]; firecrawl_check_record($arguments.id)
          and ($arguments.status == null or .status == $arguments.status))
    elif $tool == "firecrawl_monitor_check" then
      .success == true and (.data | firecrawl_check_record($arguments.id))
        and .data.id == $arguments.checkId and (.data.pages | type) == "array"
    else false end)
  | if . then $result
    else error("Firecrawl returned invalid lifecycle, identity or request-bound results")
    end;
